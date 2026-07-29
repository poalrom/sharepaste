package com.sharepaste.android

import com.sharepaste.core.Clipboard
import com.sharepaste.core.CoreEvent
import com.sharepaste.core.EventSink
import com.sharepaste.core.Keychain
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.TimeUnit

/**
 * Kotlin implementations of two of the three platform traits, wrapped around
 * the real ones so a test can both drive the platform and see what the core
 * asked of it.
 *
 * These exist to make the crossing observable, not to avoid the platform:
 * [RecordingClipboard] delegates every call to the real `ClipboardManager`.
 */
class RecordingClipboard(private val delegate: Clipboard) : Clipboard {
    val written = CopyOnWriteArrayList<String>()

    override fun readText(): String? = delegate.readText()

    override fun writeText(text: String) {
        written += text
        delegate.writeText(text)
    }
}

/**
 * An [EventSink] that records, and can be waited on.
 *
 * The waiting is the point of criterion 5: the events that matter are raised by
 * the session loop's own tokio tasks, on threads that belong to the core's
 * runtime and to no foreign runtime at all. A test that only inspected the list
 * afterwards would not prove that those threads can reach Kotlin — it would
 * pass just as well if nothing ever arrived and the assertion were loose.
 */
class RecordingSink : EventSink {

    /** One event, and the thread the core raised it on. */
    data class Received(val event: CoreEvent, val thread: String)

    private val lock = Object()
    private val received = mutableListOf<Received>()

    override fun emit(event: CoreEvent) {
        val entry = Received(event, Thread.currentThread().name)
        synchronized(lock) {
            received += entry
            lock.notifyAll()
        }
    }

    fun snapshot(): List<Received> = synchronized(lock) { received.toList() }

    /** Every distinct thread the core has called [emit] on so far. */
    fun threads(): List<String> = snapshot().map { it.thread }.distinct()

    /**
     * Block until an event matching [predicate] arrives, or the timeout
     * elapses. Returns the event together with the thread that raised it, or
     * `null`.
     */
    fun await(timeout: Long, unit: TimeUnit, predicate: (CoreEvent) -> Boolean): Received? {
        val deadline = System.nanoTime() + unit.toNanos(timeout)
        synchronized(lock) {
            var index = 0
            while (true) {
                while (index < received.size) {
                    val candidate = received[index++]
                    if (predicate(candidate.event)) return candidate
                }
                val remainingMs = (deadline - System.nanoTime()) / 1_000_000
                if (remainingMs <= 0) return null
                lock.wait(remainingMs)
            }
        }
    }
}

/**
 * A keychain that lives and dies with the test.
 *
 * The inviting side of a pairing test must **not** share
 * `AndroidKeychain`'s `EncryptedSharedPreferences` with the phone under test:
 * both store under `<user_id>:key`, and while the ids differ, a test that
 * scribbles a second User's key into the app's real keystore-backed store is a
 * test that leaves the app in a state no user could reach.
 */
class InMemoryKeychain : Keychain {
    private val entries = ConcurrentHashMap<String, String>()

    override fun put(account: String, secret: String) {
        entries[account] = secret
    }

    override fun get(account: String): String? = entries[account]

    override fun delete(account: String) {
        entries.remove(account)
    }
}

/** A clipboard for a facade that is standing in for another device. */
object NoClipboard : Clipboard {
    override fun readText(): String? = null

    override fun writeText(text: String) = Unit
}

/** An [EventSink] for a facade whose events no test reads. */
object SilentSink : EventSink {
    override fun emit(event: CoreEvent) = Unit
}
