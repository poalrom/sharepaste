package com.sharepaste.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.platform.AndroidClipboard
import com.sharepaste.android.platform.AndroidKeychain
import com.sharepaste.core.ConnectionState
import com.sharepaste.core.CoreEvent
import com.sharepaste.core.OfferOutcome
import com.sharepaste.core.Sharepaste
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.concurrent.TimeUnit

/**
 * Events raised by the session loop's **own** tasks arrive in Kotlin.
 *
 * This is the awkward crossing and the reason it gets a live relay rather than
 * a stub. The events under test are not replies to a call: the SSE reader, the
 * uploader and the contact stamp all run on worker threads of the core's
 * private tokio runtime, attached to no foreign runtime and to no Android
 * looper. If the generated binding could not reach Kotlin from there, every
 * test that only ever calls *into* the core would still pass and the app would
 * be deaf.
 *
 * The relay is the one `docker compose` serves on the host; from inside the
 * emulator that is `10.0.2.2:8443`. Pass an invite created with the operator
 * CLI:
 *
 * ```
 * docker exec sharepaste node /app/dist/index.js user create <name>
 * ./gradlew :app:connectedDebugAndroidTest \
 *     -Pandroid.testInstrumentationRunnerArguments.inviteToken=<token>,<more-tokens>
 * ```
 */
@RunWith(AndroidJUnit4::class)
class SessionEventsTest {

    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val arguments = InstrumentationRegistry.getArguments()

    private val serverUrl: String = arguments.getString("relayUrl") ?: DEFAULT_RELAY
    private val inviteToken: String by lazy { TestRelay.nextInvite() }

    private lateinit var sink: RecordingSink
    private lateinit var core: Sharepaste
    private var pairedUserId: String? = null

    @Before
    fun open() {
        val dbFile = File(context.filesDir, "session-proof.db")
        listOf("", "-wal", "-shm").forEach { File(dbFile.path + it).delete() }
        sink = RecordingSink()
        core = Sharepaste.open(
            dbPath = dbFile.absolutePath,
            keychain = AndroidKeychain(context),
            clipboard = AndroidClipboard(context),
            events = sink,
            // The test relay is plain HTTP, so this suite says so. The shipped
            // app passes `true` — see `TransportPolicyTest`.
            requireHttps = false,
        )
    }

    @After
    fun close() {
        core.stopAllSessions()
        pairedUserId?.let { runCatching { core.forgetPairing(it) } }
        core.close()
    }

    @Test
    fun a_live_session_raises_events_on_its_own_threads() {
        val token = inviteToken
        val callerThread = Thread.currentThread().name
        Evidence.log("relay         = $serverUrl")
        Evidence.log("caller thread = $callerThread")
        assertRelayIsReachable()


        val paired = core.pairWithInvite(serverUrl, token, "instrumented emulator")
        pairedUserId = paired.userId
        Evidence.log("paired        = user=${paired.userId} device=${paired.deviceId}")

        // Brings the session up: the SSE reader, the uploader and the contact
        // stamp all go onto the core's runtime here.
        core.setActivePairing(paired.userId)
        core.startSession(paired.userId)

        val online = sink.await(SESSION_TIMEOUT_SECONDS, TimeUnit.SECONDS) {
            it is CoreEvent.ConnectionState && it.state == ConnectionState.ONLINE
        }
        assertNotNull(
            "no ConnectionState(Online) reached Kotlin within ${SESSION_TIMEOUT_SECONDS}s; " +
                "events seen: ${sink.snapshot().map { it.event::class.java.simpleName }}",
            online,
        )
        Evidence.log("online event  = ${online!!.event} on thread ${online.thread}")
        // Polled rather than read once. `conn_states` is a live reading, and a
        // session that reached Online can be back in Connecting a moment later
        // after a reconnect — which happens on a loaded emulator and is not what
        // this test is about. The criterion is the event above, and the thread it
        // arrived on; this only confirms the session is genuinely up.
        awaitConnectionState(paired.userId, ConnectionState.ONLINE)

        // Hand the protocol some text; the uploader task takes it from here and
        // reports the queue depth back through the same sink.
        val text = "instrumented-${System.currentTimeMillis()}"
        val outcome = core.offer(paired.userId, text)
        Evidence.log("offer         = $outcome")
        assertTrue("an Offer must be honoured", outcome is OfferOutcome.Queued)

        val drained = sink.await(SESSION_TIMEOUT_SECONDS, TimeUnit.SECONDS) {
            it is CoreEvent.PendingCount && it.count == 0L
        }
        assertNotNull(
            "the uploader never reported an empty queue; events seen: " +
                sink.snapshot().map { it.event::class.java.simpleName },
            drained,
        )
        Evidence.log("uploaded      = ${drained!!.event} on thread ${drained.thread}")

        // The point of the whole test. Neither of those two events came back on
        // the thread that called in, and neither arrived on the main looper:
        // they were raised by the SSE reader and the uploader, on threads of
        // the core's own runtime, which the binding attached to the JVM for the
        // duration of the call. (Those threads surface under JNA's own names,
        // `Thread-N`, not the Rust runtime's `sharepaste-core`.)
        val mainThread = android.os.Looper.getMainLooper().thread.name
        Evidence.log("emit threads  = ${sink.threads()}")
        Evidence.log("caller/main   = $callerThread / $mainThread")
        listOf("ConnectionState(Online)" to online, "PendingCount(0)" to drained).forEach { (what, received) ->
            assertNotEquals(
                "$what arrived on the calling thread, which proves nothing about the session loop",
                callerThread,
                received.thread,
            )
            assertNotEquals("$what arrived on the main looper", mainThread, received.thread)
        }

        Evidence.log("events        = ${sink.snapshot().map { "${it.event::class.java.simpleName}@${it.thread}" }}")
    }

    /**
     * A plain TCP connect before the protocol is asked to do anything.
     *
     * Without it, a relay that is simply not running looks exactly like a
     * broken FFI boundary — the failure surfaces as `AppException.InsecureRelay`
     * from deep inside `pairWithInvite` and says nothing about which of the two
     * it was.
     */
    private fun assertRelayIsReachable() {
        val authority = serverUrl.substringAfter("://")
        val host = authority.substringBefore(':')
        val port = authority.substringAfter(':', "80").substringBefore('/').toInt()
        try {
            java.net.Socket().use { socket ->
                socket.connect(java.net.InetSocketAddress(host, port), 5_000)
                Evidence.log("relay socket  = connected to $host:$port from the emulator")
            }
        } catch (e: Exception) {
            throw AssertionError(
                "the relay at $serverUrl is not reachable from this emulator (${e.message}). " +
                    "Start it on the host and check that 10.0.2.2 maps to the host loopback.",
                e,
            )
        }
    }

    private fun awaitConnectionState(userId: String, expected: ConnectionState) {
        val deadline = System.nanoTime() + SESSION_TIMEOUT_SECONDS * 1_000_000_000L
        var last: ConnectionState? = null
        while (System.nanoTime() < deadline) {
            last = core.connectionState(userId)
            if (last == expected) return
            Thread.sleep(100)
        }
        throw AssertionError("the core never reported $expected; it says $last")
    }

    private companion object {
        /** The host's loopback, as seen from inside the emulator. */
        const val DEFAULT_RELAY = "http://10.0.2.2:8443"
        const val SESSION_TIMEOUT_SECONDS = 60L
    }
}
