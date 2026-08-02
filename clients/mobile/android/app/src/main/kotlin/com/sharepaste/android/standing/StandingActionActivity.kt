package com.sharepaste.android.standing

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.util.Log
import com.sharepaste.android.OfferAttempt
import com.sharepaste.android.R
import com.sharepaste.android.RecallAttempt
import com.sharepaste.android.SharepasteApplication
import com.sharepaste.android.SharepasteRepository
import com.sharepaste.android.ui.Receipt
import com.sharepaste.android.ui.offerRefusalLabel
import com.sharepaste.android.ui.offerRefusalMessage
import com.sharepaste.android.ui.receiptLogged
import com.sharepaste.android.ui.showReceipt
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
 * uploaders in a race over one `last_seen_seq`.
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
     * `NonCancellable` for exactly that reason — and under a bound of its own,
     * because a teardown nothing can cancel is also a teardown nothing can end.
     * So a destroyed window can neither leave a session running nor leave a
     * coroutine waiting on one forever. Past that, cancelling can only cost the
     * sentence afterwards.
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
                        val (receipt, queuedOn) = offer()
                        // Reported first: the person pressed a control and is
                        // owed an answer now, not when the network has finished.
                        report(action, receipt)
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
                // arrived beside. A press that has not started yet — one that
                // arrived while the shade held the focus — is outstanding too;
                // see [Presses.finished].
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
     * Answers the Receipt to show, and — when there is one — the Pairing whose
     * queue now has something in it. See [SharepasteRepository.sendPending] for
     * why that second half exists.
     */
    private suspend fun offer(): Pair<Receipt, String?> = try {
        when (val attempt = repository.offerClipboard()) {
            OfferAttempt.Unpaired ->
                Receipt.Aloud(R.string.notice_not_paired, R.string.action_unpaired) to null

            is OfferAttempt.Settled -> when (val outcome = attempt.outcome) {
                is OfferOutcome.Queued -> Receipt.Offered(outcome.pending) to attempt.userId
                // Carries the Pairing exactly as a queued Offer does. Nothing
                // was captured, but a recognition still records a Use, and a
                // Use made with the Relay out of reach is queued and wants the
                // same flush.
                is OfferOutcome.Recognised ->
                    Receipt.Recognised(outcome.pending) to attempt.userId

                // The same two sentences the screen uses, under the same two
                // labels, from the same two functions. A refusal a person cannot
                // act on is a control that did nothing.
                is OfferOutcome.Rejected -> Receipt.Aloud(
                    offerRefusalLabel(outcome.reason),
                    offerRefusalMessage(outcome.reason),
                ) to null
            }
        }
    } catch (e: AppException) {
        Receipt.Aloud(R.string.notice_failed, R.string.offer_failed) to null
    }

    /**
     * The newest Entry onto this device's clipboard — what it was, and the cache
     * fallback said out loud.
     *
     * Recall Latest always attempts the round trip, and this is now the only
     * verb that does: `RECALL FIRST` on the open screen selects from the cache
     * and fetches nothing (ADR 0010). When the fetch fails, the Entry this
     * phone already had is still the best answer available, but it may be
     * yesterday's link and only the person can tell — so it reaches this
     * surface as a [Receipt.Aloud], a warning said out loud and never a
     * confirmation, which is what keeps ADR 0007's "may never be silent" true
     * of the switch below. There is no longer an in-app twin of it, because
     * there is no longer an in-app fetch to fall back from.
     *
     * The confirmation names the Entry's Preview, read back through the same
     * [SharepasteRepository.previewOf] the state holder uses, so a Recall from
     * here and a Recall from a row are one Receipt rather than two that resemble
     * each other. A missing Preview is still a [Receipt.Recalled]: the outcome
     * is the same and so is what silences it.
     */
    private suspend fun recallLatest(): Receipt = try {
        when (val attempt = repository.recallLatestOnActivePairing()) {
            RecallAttempt.Unpaired ->
                Receipt.Aloud(R.string.notice_not_paired, R.string.action_unpaired)

            is RecallAttempt.Done -> if (attempt.fromCache) {
                Receipt.Aloud(R.string.recall_from_cache_badge, R.string.recall_from_cache)
            } else {
                Receipt.Recalled(repository.previewOf(attempt.userId, attempt.entryId))
            }
        }
    } catch (e: AppException.NotFound) {
        Receipt.Aloud(R.string.notice_nothing_to_recall, R.string.recall_nothing_to_recall)
    } catch (e: AppException) {
        Receipt.Aloud(R.string.notice_failed, R.string.recall_failed)
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
     * **The log line is never the Toast.** It is [receiptLogged], which for a
     * Recall is the fixed sentence that names no Entry — see ADR 0009 for what
     * the Toast is allowed to say and why a durable log is not allowed to say
     * it. `StandingActionsNotificationTest` and the acceptance sequence both
     * read this line expecting one of the app's own fixed sentences.
     *
     * The verb is passed in rather than read back off [getIntent], because
     * [intent] answers with whatever was delivered *most recently* — a second
     * press arriving while this one is still working would otherwise relabel
     * this line as the other verb.
     */
    private suspend fun report(action: String?, receipt: Receipt) {
        // Suppressed whole, not merely stripped of its Preview: `SHOW WHAT WAS
        // RECALLED` off means Sharepaste says nothing about a Recall on either
        // path. The log line still goes, because it is a diagnostic rather than
        // something the person is being told, and because the acceptance
        // sequence reads it with the app force-stopped.
        val silenced = receipt is Receipt.Recalled &&
            !(application as SharepasteApplication).uiPreferences.showRecalledNow()
        if (!silenced) showReceipt(this, receipt)
        Log.i(StandingActions.TAG, "$action: ${getString(receiptLogged(receipt))}")
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
 * "the window may close" is not "the verb finished" but **"nothing is
 * outstanding"** — nothing working and nothing owed. An Offer spends up to ten
 * seconds draining its queue and finishing the window under it would cancel the
 * upload the person asked for; a press still waiting for focus has not started
 * yet and would die with the window just as quietly.
 *
 * A value rather than three fields on the activity, for the reason `sharedFrom`
 * is a function rather than three private methods on the share target: this is
 * the whole of the judgement and it is the part worth testing. Driving it needs
 * no window, no focus and no facade, so `StandingActionPressesTest` pins it on
 * the JVM — in CI, where the instrumented suite does not run, and where a
 * Standing Action cannot do real work at all because the shipped transport
 * policy refuses the only Relay a test can reach. The counting half is [Verbs],
 * which the share target holds directly: it has the same window-closing rule and
 * none of the focus one.
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

    private val verbs = Verbs()

    /** Nothing is working, so the window has nothing to stay open for. */
    val idle: Boolean get() = verbs.idle

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

    /**
     * A verb finished. Answers whether the window may now close.
     *
     * **Not while a press is still owed**, which is the hole a counter alone
     * left. [start] counts only the verbs it actually starts, so a press that
     * arrived without focus lives in [owed] and nowhere else — and **the
     * notification shade is exactly what takes window focus away**, so an
     * `onNewIntent` from a notification action arrives unfocused by
     * construction. If the verb already working finished in that gap — an
     * Offer's drain can end anywhere in its ten seconds — this answered "the
     * window may close", the window closed, and the recorded press died with it
     * in silence. That is the dropped second press this class exists to prevent,
     * arriving by the one route that always removes focus.
     *
     * `onStop` still closes on [idle] alone, and that is deliberate: a window
     * whose focus is gone for good — a press on a locked screen — has to be able
     * to go, and an owed press is worth less than a task that never empties.
     */
    fun finished(): Boolean {
        val nothingWorking = verbs.finished()
        return nothingWorking && owed == null
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
        verbs.began()
        return verb
    }
}
