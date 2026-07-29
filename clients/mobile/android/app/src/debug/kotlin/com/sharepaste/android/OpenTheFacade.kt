package com.sharepaste.android

import android.util.Log
import java.io.File

/**
 * How a **debug** build opens its facade: the shipped policy, unless a marker
 * file says a test is standing behind it.
 *
 * Read the release copy of this file first — `app/src/release` — because that
 * one is what ships and it has no branch at all. This copy exists for exactly
 * one caller.
 *
 * **Why the concession is needed.** Every other instrumented test in this module
 * opens a facade of its own with `requireHttps = false`, which the Android
 * contract permits: the test Relay is plain HTTP and there is no publicly
 * trusted certificate to put in front of it from inside an emulator. Ticket 12's
 * Standing Actions cannot do that. They run with the app **not running** — no
 * instrumentation is attached, the process is started by an `Intent` — so the
 * only facade they can possibly use is `SharepasteApplication.repository`, the
 * app's own. With `requireHttps = true` that facade can never reach the test
 * Relay, and "the Offer action captured an Entry and it reached the Relay" is
 * unprovable rather than untrue.
 *
 * **Why a marker file rather than a flag.** The process under test is restarted
 * between the arranging step and the acted-upon step (`am force-stop`), so the
 * decision has to survive a process death. `filesDir` is app-private, so nothing
 * outside this application can plant one.
 *
 * **Why it is loud.** A security relaxation that says nothing is how a debug
 * affordance quietly becomes load-bearing. When the marker takes effect this
 * logs at WARN, names the file, names where the branch lives, and says how to
 * undo it — so that "why did cleartext work?" is one `logcat` grep away rather
 * than an afternoon.
 */
internal fun openTheFacade(app: SharepasteApplication): SharepasteRepository {
    val marker = File(app.filesDir, RELAXED_TRANSPORT_POLICY_MARKER)
    val relaxed = marker.exists()
    if (relaxed) {
        Log.w(
            RELAXED_TRANSPORT_POLICY_TAG,
            "TRANSPORT POLICY RELAXED. ${marker.absolutePath} exists, so this facade will accept " +
                "a cleartext http:// Relay instead of refusing it with InsecureRelay. This branch " +
                "lives only in app/src/debug and only for ticket 12's force-stop acceptance " +
                "sequence; the release source set has no code path that can reach it. " +
                "BuildConfig.REQUIRE_HTTPS is ${BuildConfig.REQUIRE_HTTPS}; delete the marker to " +
                "get it back.",
        )
    }
    return SharepasteRepository.open(app, requireHttps = BuildConfig.REQUIRE_HTTPS && !relaxed)
}

/**
 * The marker's name inside `filesDir`.
 *
 * Written and — in a `finally`, never on a happy path — deleted by
 * `StandingActionsOnAClosedPhoneTest`. A run that fails midway must not leave an
 * emulator permanently relaxed for every later run on it.
 */
internal const val RELAXED_TRANSPORT_POLICY_MARKER = "relax-transport-policy-for-a-test"

/** One tag, so the warning above is greppable without knowing the wording. */
internal const val RELAXED_TRANSPORT_POLICY_TAG = "SharepasteTransportPolicy"
