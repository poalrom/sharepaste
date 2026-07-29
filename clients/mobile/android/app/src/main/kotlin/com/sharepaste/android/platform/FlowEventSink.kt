package com.sharepaste.android.platform

import android.util.Log
import com.sharepaste.core.CoreEvent
import com.sharepaste.core.EventSink
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow

/**
 * Where the core's events arrive, and the one place the thread they arrive on
 * is dealt with.
 *
 * [emit] is called from the session loop's **own** tokio tasks — the SSE
 * reader, the uploader, the pair poll. Those are worker threads of the core's
 * private runtime, attached to the JVM only for the duration of the call. Two
 * rules follow, and both are load-bearing:
 *
 *  * **Never block.** The core holds its connection-state lock across this
 *    call. Anything that waits here stalls the session.
 *  * **Never touch UI state.** This is not the main thread. Handing the event
 *    to a [SharedFlow] is what moves it: a collector running in a
 *    `Dispatchers.Main` scope receives it there, so the marshalling happens
 *    once, at the collector, instead of at every call site.
 */
class FlowEventSink : EventSink {

    private val _events = MutableSharedFlow<CoreEvent>(
        replay = 0,
        // Deep enough that a burst of backfill events during a reconnect is
        // absorbed while the collector is still being resumed. `tryEmit` only
        // fails once this is full, which is a real signal rather than noise.
        extraBufferCapacity = 256,
    )

    val events: SharedFlow<CoreEvent> = _events.asSharedFlow()

    override fun emit(event: CoreEvent) {
        if (!_events.tryEmit(event)) {
            // Dropped rather than suspended, deliberately: a stalled collector
            // must not stall the protocol. Logged because a dropped event is a
            // desynchronised UI and someone has to be able to see that it
            // happened.
            Log.w(TAG, "event buffer full; dropped ${event::class.java.simpleName}")
        }
    }

    private companion object {
        const val TAG = "SharepasteEvents"
    }
}
