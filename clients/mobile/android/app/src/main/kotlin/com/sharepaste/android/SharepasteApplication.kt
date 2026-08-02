package com.sharepaste.android

import android.app.Application
import android.os.StrictMode
import com.sharepaste.android.platform.UiPreferences
import com.sharepaste.android.standing.StandingActions

/**
 * The process, and the one facade in it.
 *
 * **One facade per process, not one per screen.** Two `Sharepaste` objects over
 * the same SQLite file would each own a tokio runtime, each hold a connection and
 * each run a session loop against the same Relay — two writers, two SSE streams
 * and two uploaders racing over one `last_seen_seq`. Holding it here also means
 * ticket 12's transparent activity gets the *same* live session as the screen
 * that is already open, rather than standing a second one up to do one Recall.
 *
 * Its other job is to make the boundary rule loud. Every call into the core
 * blocks, and a blocking call on the main thread is an ANR waiting for a slow
 * network — but only sometimes, and only on someone else's phone. The
 * `StrictMode` policies below turn that into a log line on the first violation,
 * in development, where it is cheap.
 */
class SharepasteApplication : Application() {

    /**
     * The facade, opening from the moment it is first asked for.
     *
     * `by lazy` rather than opened in [onCreate]: the process is also started for
     * things that never touch the protocol, and `Application.onCreate` blocks the
     * first frame. [SharepasteRepository.open] itself returns immediately and does
     * the blocking work on `Dispatchers.IO`, so this is cheap to touch from the
     * main thread.
     *
     * `openTheFacade` is a **per-variant** function, and which copy is compiled
     * in is the whole of the transport policy. The release one is a single
     * expression handing `BuildConfig.REQUIRE_HTTPS` — which is `true` — to the
     * core, with no branch and nothing it reads; the debug one adds one, for
     * ticket 12's Standing Actions, which run with the app not running and so
     * cannot be handed a test facade the way every other instrumented test is.
     * Read `app/src/release/kotlin/.../OpenTheFacade.kt`; it is shorter than this
     * comment. The core is the only layer that can actually refuse a cleartext
     * Relay, because Android's network security configuration is honoured by the
     * Java stack and never consulted by the core's Rust HTTP client.
     */
    val repository: SharepasteRepository by lazy { openTheFacade(this) }

    /**
     * The two things this phone remembers about its own chrome.
     *
     * Here for the same reason the facade is: one DataStore file may have one
     * instance in a process, and a second one over the same file would be two
     * writers with no lock between them. Both surfaces need it — the state
     * holder collects it, and a Standing Action reads one value out of it with
     * no state holder anywhere — so the process is the only place both can
     * reach.
     *
     * `by lazy` and not opened in [onCreate]: it touches disk on first read, and
     * the process is started for things that never look at a preference.
     */
    val uiPreferences: UiPreferences by lazy { UiPreferences(this) }

    override fun onCreate() {
        super.onCreate()
        if (BuildConfig.DEBUG) {
            StrictMode.setThreadPolicy(
                StrictMode.ThreadPolicy.Builder()
                    .detectDiskReads()
                    .detectDiskWrites()
                    .detectNetwork()
                    .penaltyLog()
                    .build(),
            )
        }
        // The Standing Actions go up whenever this process exists, which is the
        // only rule that holds for every way it comes into being: a launcher tap,
        // the boot receiver, a share, or one of the actions itself after the app
        // was force-stopped (which cancels an app's notifications). Posting is
        // idempotent — one id, one channel — and it touches nothing but the
        // notification manager, so it opens no facade and starts no session.
        StandingActions.post(this)
    }
}
