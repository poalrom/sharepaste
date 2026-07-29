package com.sharepaste.android.standing

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.util.Log
import android.widget.Toast
import com.sharepaste.android.OfferAttempt
import com.sharepaste.android.R
import com.sharepaste.android.RecallAttempt
import com.sharepaste.android.SharepasteApplication
import com.sharepaste.android.SharepasteRepository
import com.sharepaste.android.ui.offerRefusalMessage
import com.sharepaste.core.AppException
import com.sharepaste.core.OfferOutcome
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

/**
 * The window a Standing Action needs in order to touch the clipboard, and
 * nothing else.
 *
 * **Why an activity at all.** Since Android 10 the clipboard is readable only by
 * the application holding window focus, or by the default input method. A
 * `BroadcastReceiver` wired to a notification action has no window: it reads the
 * clipboard as empty, every time, indistinguishably from "nothing was copied".
 * That is the platform's rule, not a bug with a workaround, and it is why ADR
 * 0007 exists in the shape it does. So the action opens a window, and this is
 * it — invisible, unanimated, and gone again as soon as the verb it was opened
 * for is done.
 *
 * **Why [onWindowFocusChanged] and not `onResume`.** `onResume` is the activity
 * saying it is interactive; focus is the window manager saying this window is
 * the one the input system is pointed at. The clipboard check asks the second
 * question. An activity can be `RESUMED` with focus somewhere else — behind a
 * permission dialog, mid-transition, on a device just waking — and a clipboard
 * read in that window comes back empty. Waiting for focus is what turns an
 * intermittent, unreproducible "the Offer did nothing" into a thing that always
 * works.
 *
 * **Why it is invisible rather than a one-frame screen.** The result of both
 * verbs is a Toast and a change to the clipboard. A window that flashed up and
 * vanished would be the only visible part of the operation and would read as a
 * crash. `Theme.Sharepaste.Invisible` has a transparent background and no
 * animation, so what a person sees is the Toast.
 *
 * It uses `SharepasteApplication.repository` — the process's one facade, and
 * whatever session an open screen already has. Opening a second one over the
 * same database would put two tokio runtimes, two SQLite connections and two
 * uploaders in a race over one `last_seen_id`.
 */
class StandingActionActivity : Activity() {

    /**
     * Scoped to this activity, and that is safe for a reason worth stating.
     *
     * The window is finished only once every verb it has been handed has
     * finished and reported — see [Presses] — so the ordinary path never races
     * the cancellation. And a call already inside the blocking FFI boundary is
     * not interruptible anyway: cancelling the coroutine cannot abandon a
     * half-written Entry.
     *
     * What cancellation *could* cut short is the session teardown after an
     * Offer, because `stopSession` suspends and this scope dies in [onDestroy].
     * `SharepasteRepository.drainPending` brings the session down under
     * `NonCancellable` for exactly that reason, so a destroyed window cannot
     * leave one running. Past that, cancelling can only cost the sentence
     * afterwards.
     */
    private val scope = MainScope()

    /** Which verb this window owes, and when it may close. See [Presses]. */
    private val presses = Presses()

    private val repository: SharepasteRepository
        get() = (application as SharepasteApplication).repository

    /**
     * Nothing is drawn and nothing is read here — the verb waits for focus, which
     * is the only moment the clipboard is readable. What `onCreate` does is
     * *record* the press, so that [onWindowFocusChanged] has something to run.
     *
     * There is no window animation to suppress either:
     * `Theme.Sharepaste.Invisible` sets `windowAnimationStyle` to null, which is
     * the declarative form of the deprecated `overridePendingTransition`.
     */
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        presses.arrived(intent?.action)?.let(::run)
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        presses.focus(hasFocus)?.let(::run)
    }

    /**
     * A second press, handed to the window the first one is still using.
     *
     * `launchMode="singleTask"` with an empty `taskAffinity` means the platform
     * does not stack a second invisible activity — it delivers the new `Intent`
     * here, and `onCreate` does not run again. Without this override that
     * `Intent` goes nowhere and [getIntent] still answers with the **first**
     * press's, so pressing Recall while an Offer was inside its ten-second drain
     * did nothing whatsoever and said nothing about it. Recall then Offer is a
     * plausible pair of presses in a hurry, and a control that silently does
     * nothing reads as a broken app.
     *
     * [setIntent] keeps [getIntent] honest about what this window is doing;
     * [Presses] is what decides whether the verb runs now or waits for focus.
     */
    override fun onNewIntent(intent: Intent?) {
        super.onNewIntent(intent)
        setIntent(intent)
        presses.arrived(intent?.action)?.let(::run)
    }

    /** One verb, from the moment [Presses] says it may start. */
    private fun run(action: String) {
        scope.launch {
            try {
                when (action) {
                    StandingActions.ACTION_OFFER -> {
                        val (message, queuedOn) = offer()
                        // Reported first: the person pressed a control and is
                        // owed an answer now, not when the network has finished.
                        report(action, message)
                        // And then actually sent. `offer` only enqueues — the
                        // uploader lives on a session, which a phone with no
                        // screen open does not have, so without this "Offer
                        // without opening the app" would mean an Entry that waits
                        // until the app *is* opened. That is not the feature. ADR
                        // 0007 is intact: the session comes up because somebody
                        // pressed a control, it is bounded, and it is down again
                        // before this window closes.
                        queuedOn?.let { repository.sendPending(it) }
                    }
                    StandingActions.ACTION_RECALL_LATEST -> report(action, recallLatest())
                    // Not reachable from the notification, and the activity is not
                    // exported. Nothing to say, so nothing is said.
                    else -> Unit
                }
            } finally {
                // The window closes when the *last* verb is done, not the first:
                // finishing under a second press would cancel the Offer it
                // arrived beside.
                if (presses.finished() && !isFinishing) finish()
            }
        }
    }

    /**
     * A window with nothing working that has lost focus is a window that will
     * never do anything.
     *
     * Without this an invisible activity could sit in a task forever — after a
     * `Recall` on a locked screen, say, where focus never arrives. It cannot cut
     * a verb short: [Presses.idle] is false from the moment a verb starts until
     * it is done, so a press still inside its drain keeps the window.
     */
    override fun onStop() {
        super.onStop()
        if (presses.idle) finish()
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    /**
     * Offered Capture of whatever is on the clipboard now.
     *
     * Answers the sentence to show, and — when there is one — the Pairing whose
     * queue now has something in it. See [SharepasteRepository.sendPending] for
     * why that second half exists.
     */
    private suspend fun offer(): Pair<String, String?> = try {
        when (val attempt = repository.offerClipboard()) {
            OfferAttempt.Unpaired -> getString(R.string.action_unpaired) to null
            is OfferAttempt.Settled -> when (val outcome = attempt.outcome) {
                is OfferOutcome.Queued -> getString(R.string.offer_queued) to attempt.userId
                // The same three sentences the screen uses, from the same
                // function. A refusal a person cannot act on is a control that
                // did nothing.
                is OfferOutcome.Rejected -> getString(offerRefusalMessage(outcome.reason)) to null
            }
        }
    } catch (e: AppException) {
        getString(R.string.offer_failed) to null
    }

    /**
     * The newest Entry onto this device's clipboard — and the cache fallback
     * said out loud.
     *
     * Recall Latest always attempts the round trip. When the fetch fails, the
     * newest Entry this phone already had is still the best answer available,
     * but it may be yesterday's link and only the person can tell. There is no
     * screen here to put a band on, so the Toast is the surface that has to
     * carry it — and it carries the same sentence the in-app band does, because
     * it is the same fact.
     */
    private suspend fun recallLatest(): String = try {
        when (val attempt = repository.recallLatestOnActivePairing()) {
            RecallAttempt.Unpaired -> getString(R.string.action_unpaired)
            is RecallAttempt.Done -> getString(
                if (attempt.fromCache) R.string.recall_from_cache else R.string.recall_done,
            )
        }
    } catch (e: AppException.NotFound) {
        getString(R.string.recall_nothing_to_recall)
    } catch (e: AppException) {
        getString(R.string.recall_failed)
    }

    /**
     * The one surface a Standing Action has.
     *
     * The Toast goes to the **application** context and is shown *before*
     * [finish]. Both matter: a Toast is queued by the system rather than drawn
     * by the activity, so it outlives the window that asked for it — but a Toast
     * asked for after `finish` has already run is one the system may drop, and a
     * cache-fallback warning that is silently swallowed turns a correct
     * operation into a wrong one.
     *
     * The log line is the same sentence, under the verb that produced it. The
     * verb is passed in rather than read back off [getIntent], because [intent]
     * answers with whatever was delivered *most recently* — a second press
     * arriving while this one is still working would otherwise relabel this
     * line as the other verb.
     *
     * It names what happened and never what was in the Entry: neither verb has
     * the plaintext to leak — `RecallAttempt` deliberately carries none — and
     * this must stay true of anything added here.
     */
    private fun report(action: String?, message: String) {
        Toast.makeText(applicationContext, message, Toast.LENGTH_LONG).show()
        Log.i(TAG, "$action: $message")
    }

    private companion object {
        /** `adb logcat -s SharepasteStandingAction` is the whole diagnostic. */
        const val TAG = "SharepasteStandingAction"
    }
}

/**
 * Which verb an invisible window owes, and when it may close.
 *
 * Two facts have to be combined and neither arrives with the other. A press
 * arrives as an `Intent`; the verb it names can only run once the window has
 * **focus**, because focus is the only state in which Android lets an app read
 * the clipboard. So a press is recorded when it arrives and started when focus
 * allows, and that is the whole of what this holds.
 *
 * `launchMode="singleTask"` is what makes it more than a pair of fields. A second
 * press is not a second window: the platform delivers the new `Intent` to the
 * instance already running. So the verb is not a value fixed at creation, and
 * "the window may close" is not "the verb finished" but **"no verb is
 * outstanding"** — an Offer spends up to ten seconds draining its queue, and
 * finishing the window under it would cancel the upload the person asked for.
 *
 * A value rather than three fields on the activity, for the reason `sharedFrom`
 * is a function rather than three private methods on the share target: this is
 * the whole of the judgement and it is the part worth testing. Driving it needs
 * no window, no focus and no facade, so `StandingActionPressesTest` pins it on
 * the JVM — in CI, where the instrumented suite does not run, and where a
 * Standing Action cannot do real work at all because the shipped transport
 * policy refuses the only Relay a test can reach.
 *
 * **One slot, not a queue.** A press arriving before the window can act
 * supersedes the one before it, because two presses in that window are somebody
 * correcting themselves rather than asking for both. Once the window *can* act,
 * each press starts as it arrives and they run alongside each other.
 *
 * Touched only from the main thread — the lifecycle callbacks and a `MainScope`
 * coroutine — so it needs no synchronisation.
 */
internal class Presses {

    private var owed: String? = null

    private var focused = false

    private var working = 0

    /** Nothing is working, so the window has nothing to stay open for. */
    val idle: Boolean get() = working == 0

    /**
     * A press arrived, naming [action].
     *
     * Answers the verb to start now, or `null` if there is nothing to start —
     * either because the window has no focus yet, or because the `Intent`
     * carried no action at all.
     */
    fun arrived(action: String?): String? {
        owed = action
        return start()
    }

    /** The window gained or lost focus. Answers the verb to start now, if any. */
    fun focus(hasFocus: Boolean): String? {
        focused = hasFocus
        return start()
    }

    /** A verb finished. Answers whether the window may now close. */
    fun finished(): Boolean {
        working--
        return idle
    }

    /**
     * The verb to start, if one may start.
     *
     * Clearing [owed] is what makes a verb run **once** per press: focus can
     * arrive more than once for one press, and a repeat must not offer the
     * clipboard twice.
     */
    private fun start(): String? {
        if (!focused) return null
        val verb = owed ?: return null
        owed = null
        working++
        return verb
    }
}
