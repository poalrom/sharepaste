package com.sharepaste.android

import java.io.Closeable
import java.io.IOException
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.Socket
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.Executors

/**
 * A byte-for-byte TCP forwarder in front of the relay, so a test can take the
 * relay away and give it back.
 *
 * Two criteria need "no route to the Relay" to be a *fact*, not a mock: Recall
 * Latest falling back to the newest cached Entry, and an Offer queueing instead
 * of uploading. The emulator's own switches — `svc data disable`, airplane mode —
 * are global and sticky: they take the network away from every other test in the
 * run, and `spike35` already has one networking trap that survives longer than it
 * should (ticket 08, deviation 4).
 *
 * A pairing made against a port this test owns is the same failure through a
 * blast radius of one. [close] shuts the listener and kills every live
 * connection, so the next request gets a real connection refusal from the real
 * network stack; [reopen] binds the same port again, which matters because the
 * port is baked into the Pairing's stored `server_url` and cannot move.
 *
 * Forwarding is unbuffered and flushed on every read, because the session runs
 * on SSE: a frame held in a buffer here is a frame the phone never sees, and the
 * bug would look like a broken session loop.
 */
class RelayProxy private constructor(
    private val relayHost: String,
    private val relayPort: Int,
    private var listener: ServerSocket,
) : Closeable {

    private val port = listener.localPort
    private val pumps = Executors.newCachedThreadPool()
    private val connections = CopyOnWriteArrayList<Socket>()

    /**
     * Whether the relay is meant to be reachable.
     *
     * Closing the listener alone is not enough, and the race it leaves is the kind
     * that shows up as one flaky test in a long run: a connection accepted a
     * moment before [close] can be registered a moment *after* the sweep, leaving
     * a live tunnel through a proxy that is supposed to be gone. Registration and
     * the sweep share this flag under one lock, so a connection either dies in the
     * sweep or is refused outright.
     */
    private var open = true

    /** What to pair against. Fixed for the lifetime of this object. */
    val url: String = "http://127.0.0.1:$port"

    init {
        accept(listener)
    }

    /** Take the relay away: no listener, and every open connection dropped. */
    override fun close() = synchronized(connections) {
        open = false
        listener.closeQuietly()
        connections.forEach { it.closeQuietly() }
        connections.clear()
    }

    /**
     * Give the relay back, on the same port the Pairing was made against.
     *
     * Idempotent, so a test's `@After` can restore it without knowing whether the
     * body already did — re-binding a live listener is `EADDRINUSE`, and a teardown
     * that throws hides whatever the test was actually reporting.
     */
    fun reopen() = synchronized(connections) {
        if (!listener.isClosed) return@synchronized
        open = true
        listener = bind(port)
        accept(listener)
    }

    /**
     * Fail unless the port really does refuse a connection.
     *
     * Asserted rather than assumed: a test that thinks it has taken the network away
     * and has not would quietly prove the opposite of what it claims — an Offer that
     * uploaded, a Recall Latest that fetched — and pass for the wrong reason.
     */
    fun assertUnreachable() {
        try {
            Socket().use { it.connect(InetSocketAddress("127.0.0.1", port), CONNECT_MS) }
        } catch (e: IOException) {
            return
        }
        throw AssertionError("the proxy on port $port is still accepting connections")
    }

    /** Release the threads. The proxy is unusable afterwards. */
    fun shutdown() {
        close()
        pumps.shutdownNow()
    }

    private fun accept(server: ServerSocket) = pumps.execute {
        while (!server.isClosed) {
            val downstream = try {
                server.accept()
            } catch (e: IOException) {
                return@execute // The listener was closed. That is this class's job.
            }
            val upstream = try {
                Socket().apply { connect(InetSocketAddress(relayHost, relayPort), CONNECT_MS) }
            } catch (e: IOException) {
                downstream.closeQuietly()
                continue
            }
            val registered = synchronized(connections) {
                if (open) connections.addAll(listOf(downstream, upstream)) else false
            }
            if (!registered) {
                downstream.closeQuietly()
                upstream.closeQuietly()
                continue
            }
            pumps.execute { pump(downstream, upstream) }
            pumps.execute { pump(upstream, downstream) }
        }
    }

    private fun pump(from: Socket, to: Socket) {
        val buffer = ByteArray(8 * 1024)
        try {
            val input = from.getInputStream()
            val output = to.getOutputStream()
            while (true) {
                val read = input.read(buffer)
                if (read < 0) break
                output.write(buffer, 0, read)
                // Flushed per read: the session is an SSE stream and a frame
                // sitting in a buffer is a frame that never arrived.
                output.flush()
            }
        } catch (e: IOException) {
            // A dropped connection is the point of this class, not a failure.
        } finally {
            from.closeQuietly()
            to.closeQuietly()
        }
    }

    companion object {
        private const val CONNECT_MS = 5_000

        /**
         * A proxy in front of whatever [TestRelay] points at, on a port the OS
         * picks.
         *
         * The port is taken from a real bind rather than guessed, so two runs on
         * one emulator cannot collide.
         */
        fun inFrontOfTheTestRelay(): RelayProxy {
            TestRelay.assertReachable()
            val authority = TestRelay.url.substringAfter("://")
            val host = authority.substringBefore(':')
            val port = authority.substringAfter(':', "80").substringBefore('/').toInt()
            return RelayProxy(host, port, bind(0))
        }

        private fun bind(port: Int): ServerSocket = ServerSocket().apply {
            // The listener is closed and re-bound mid-test, so the port is in
            // TIME_WAIT when `reopen` asks for it back.
            reuseAddress = true
            bind(InetSocketAddress(InetAddress.getByName("127.0.0.1"), port), 50)
        }
    }
}

private fun Closeable.closeQuietly() {
    try {
        close()
    } catch (e: IOException) {
        // Nothing to do about a socket that will not close.
    }
}
