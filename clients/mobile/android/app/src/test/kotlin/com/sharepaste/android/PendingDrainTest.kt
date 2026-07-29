package com.sharepaste.android

import com.sharepaste.core.AppException
import com.sharepaste.core.CoreEvent
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The session a Standing Action raises comes back down, on every way out.
 *
 * `drainPending` is the whole of "Offer without opening the app": it brings a
 * session up so the uploader exists, waits for the queue to empty, and puts the
 * session back down. **The last clause is the one worth a test**, because ADR
 * 0007's rule is that nothing runs while nobody is looking — and the way this
 * broke was silent. `stopSession` suspends, so a `finally` reached by
 * *cancellation* threw at its first suspension point, before the FFI, and the
 * `runCatching` around it ate the evidence. `StandingActionActivity.onDestroy`
 * cancels its scope, so an activity destroyed inside the ten-second drain left a
 * session running with no window on screen: unattended sync, arrived at by
 * accident, reported nowhere.
 *
 * A **JVM** test rather than an instrumented one, deliberately. Cancelling a real
 * activity at a chosen instant of a real drain is a race to arrange and a race to
 * assert; the ordering rule itself needs no device, and this way it runs in CI,
 * which the instrumented suite does not. `SharepasteRepository` implements
 * [PendingQueue] with the same four members [Queue] fakes, and
 * `SharepasteRepository.sendPending` is one line delegating here, so there is
 * nothing between what ships and what is asserted.
 */
class PendingDrainTest {

    /**
     * The core, as far as a drain can tell.
     *
     * **Every member hops to another dispatcher, exactly as
     * `SharepasteRepository.io` does, and that is load-bearing rather than
     * decorative.** The hop is the suspension point at which a cancelled
     * coroutine throws — so a fake whose `stopSession` merely incremented a
     * counter would sail through [the_session_comes_down_even_when_the_drain_is_cancelled]
     * with the bug still in place, and this class would be worse than no class at
     * all.
     */
    private class Queue(
        private val pending: Long?,
        private val noRouteToTheRelay: Boolean = false,
    ) : PendingQueue {

        private val sink = MutableSharedFlow<CoreEvent>(extraBufferCapacity = 8)

        override val events: SharedFlow<CoreEvent> = sink

        var started = 0
            private set

        var stopped = 0
            private set

        /** How many times the queue depth was read across the boundary. */
        var depthReads = 0
            private set

        /** Whether anything is collecting [events] yet. */
        val listeners: Int get() = sink.subscriptionCount.value

        suspend fun raise(event: CoreEvent) = sink.emit(event)

        override suspend fun startSession(userId: String) {
            io {
                started++
                if (noRouteToTheRelay) throw AppException.Network("no route to the relay")
            }
        }

        override suspend fun stopSession(userId: String) {
            io { stopped++ }
        }

        override suspend fun pendingOn(userId: String): Long? = io {
            depthReads++
            pending
        }

        private suspend fun <T> io(call: () -> T): T = withContext(Dispatchers.IO) { call() }
    }

    /**
     * The activity was destroyed mid-drain, and the session came down anyway.
     *
     * This is the regression. Take `NonCancellable` out of `drainPending` and
     * this fails while every other test in this class still passes, which is the
     * property that makes it worth keeping: the bug is invisible from every angle
     * except this one.
     */
    @Test
    fun the_session_comes_down_even_when_the_drain_is_cancelled() = runBlocking {
        val queue = Queue(pending = 3L)
        // The activity's own scope, in the shape the activity holds it: separate
        // from the caller, and cancelled wholesale when the window goes.
        val window = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val drain = window.launch { queue.drainPending(USER, timeoutMs = FOREVER) }

        awaitTrue("the drain must bring a session up and start listening") {
            queue.started == 1 && queue.listeners == 1
        }
        // `onDestroy`, in the middle of the ten seconds an Offer with a slow
        // connection spends here.
        drain.cancelAndJoin()

        assertEquals(
            "the drain was cancelled mid-flight and left the session running. A session with no " +
                "window on screen is the unattended sync ADR 0007 forbids, and nothing reports it.",
            1,
            queue.stopped,
        )
    }

    /**
     * The ordinary ending: the core says the queue is empty and the drain agrees.
     *
     * Also the anti-poll assertion. The queue depth is read across the FFI
     * boundary **once** — not once per 200ms, which was up to fifty times per
     * Offer, each one rebuilding every Pairing this device holds to learn a
     * number the core was already emitting.
     */
    @Test
    fun it_ends_on_the_count_the_core_emits_and_reads_the_depth_once() = runBlocking {
        val queue = Queue(pending = 2L)
        val drained = async(Dispatchers.Default) { queue.drainPending(USER, timeoutMs = FOREVER) }
        awaitTrue("the drain must be listening before anything is raised") { queue.listeners == 1 }

        queue.raise(CoreEvent.PendingCount(USER, 0L))

        assertTrue("an emptied queue is a drain that succeeded", drained.await())
        assertEquals("the session must come down on the ordinary path too", 1, queue.stopped)
        assertEquals(
            "the queue depth was read more than once, which is the 200ms poll coming back",
            1,
            queue.depthReads,
        )
    }

    /**
     * Somebody else's queue emptying is not this drain's ending.
     *
     * Asserted by letting the drain **time out** rather than by looking at it
     * mid-flight: a `SharedFlow` emission is buffered rather than delivered, so
     * "it has not finished yet" a millisecond after raising an event is true of a
     * correct drain and of a sloppy predicate alike. Timing out is the only
     * observation that tells them apart. A drain that answered these events would
     * come back `true`, and quickly.
     */
    @Test
    fun a_count_for_another_pairing_is_not_this_drains_ending() = runBlocking {
        val queue = Queue(pending = 1L)
        val drained = async(Dispatchers.Default) { queue.drainPending(USER, timeoutMs = SHORT_MS) }
        awaitTrue("the drain must be listening before anything is raised") { queue.listeners == 1 }

        // Another Pairing's queue reaching empty, this Pairing's queue going down
        // without reaching it, and an event of an altogether different kind.
        queue.raise(CoreEvent.PendingCount("some-other-pairing", 0L))
        queue.raise(CoreEvent.PendingCount(USER, 1L))
        queue.raise(CoreEvent.HistoryChanged(USER))

        assertFalse(
            "the drain ended on an event that was not \"$USER's queue is empty\"",
            drained.await(),
        )
        assertEquals(1, queue.stopped)
    }

    /**
     * A queue that was already empty ends the drain, with no event at all.
     *
     * The gap events cannot close. A session an open screen still holds can empty
     * the queue between the Offer and this collector existing, and the count that
     * would have ended the wait was emitted with nobody listening — so awaiting
     * the event alone would hold an invisible window for the full timeout every
     * time somebody pressed Offer with the app on screen behind it.
     */
    @Test
    fun a_queue_already_empty_needs_no_event() = runBlocking {
        val queue = Queue(pending = 0L)

        assertTrue(
            "nothing was going to be emitted, so this must end on the one read it takes",
            queue.drainPending(USER, timeoutMs = FOREVER),
        )
        assertEquals(1, queue.stopped)
    }

    /**
     * A stalled upload ends the drain instead of holding the window.
     *
     * Nothing is ever emitted here and the queue is not empty, which is exactly
     * an Offer made on a phone with no usable connection. The bound is what stops
     * an invisible activity sitting in a task while somebody waits for nothing;
     * not draining in time is the ordinary offline outcome and is deliberately
     * not reported twice.
     */
    @Test
    fun a_stalled_upload_ends_the_drain_rather_than_holding_the_window() = runBlocking {
        val queue = Queue(pending = 1L)
        val began = System.nanoTime()

        assertFalse(
            "a queue that never emptied is a drain that answers false, not one that throws",
            queue.drainPending(USER, timeoutMs = STALLED_MS),
        )

        val elapsed = (System.nanoTime() - began) / 1_000_000
        assertTrue("it gave up after ${elapsed}ms, before its own bound", elapsed >= STALLED_MS)
        assertEquals("a timed-out drain still has to put the session down", 1, queue.stopped)
    }

    /**
     * No route to the Relay is not an error, and the teardown still runs.
     *
     * `startSession` on a phone with no connection raises `AppException`. The
     * Entry is already enqueued and the History already says so, so there is
     * nothing to report and nothing to fail — but there may still be a session
     * half up, and it is not this function's business to guess.
     */
    @Test
    fun no_route_to_the_relay_is_not_an_error() = runBlocking {
        val queue = Queue(pending = 1L, noRouteToTheRelay = true)

        assertFalse(queue.drainPending(USER, timeoutMs = STALLED_MS))
        assertEquals("the session start failed; the teardown must run regardless", 1, queue.stopped)
    }

    /**
     * Poll for a condition another coroutine will bring about, or say what was
     * being waited for.
     *
     * The drain runs on [Dispatchers.Default] because that is where a cancelled
     * activity's coroutine is cancelled *from* — a second thread — so there is
     * nothing to await on here but the fake's own counters.
     */
    private suspend fun awaitTrue(what: String, condition: () -> Boolean) {
        val deadline = System.nanoTime() + WAIT_MS * 1_000_000
        while (System.nanoTime() < deadline) {
            if (condition()) return
            delay(5)
        }
        throw AssertionError("$what: it never became true in ${WAIT_MS}ms")
    }

    private companion object {
        const val USER = "the-pairing-under-test"

        /**
         * Longer than any of these tests, because none of them is about the
         * timeout except the two that name one.
         */
        const val FOREVER = 60_000L

        /** Short enough to wait for in a unit test, long enough not to be flaky. */
        const val STALLED_MS = 250L

        /**
         * A bound for a drain that is *expected* to time out with room to raise a
         * few events into it first.
         */
        const val SHORT_MS = 1_000L

        /** How long a set-up condition another thread has to bring about may take. */
        const val WAIT_MS = 10_000L
    }
}
