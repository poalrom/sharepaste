package com.sharepaste.android.standing

import android.app.Activity
import android.content.ClipDescription
import android.content.Intent
import android.os.Bundle
import com.sharepaste.android.OfferAttempt
import com.sharepaste.android.R
import com.sharepaste.android.SharepasteApplication
import com.sharepaste.android.SharepasteRepository
import com.sharepaste.android.ui.Receipt
import com.sharepaste.android.ui.offerRefusalLabel
import com.sharepaste.android.ui.offerRefusalMessage
import com.sharepaste.core.AppException
import com.sharepaste.core.OfferOutcome
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

/**
 * The secondary Offer path: text shared from any other app becomes an Offered
 * Capture.
 *
 * Invisible and unanimated for the same reason [StandingActionActivity] is —
 * there is nothing to show — but it does **not** wait for window focus, because
 * it never touches the clipboard. The content arrives inside the `Intent`, which
 * the system has already handed over by the time `onCreate` runs. Waiting for
 * focus here would only add a frame of latency and one more way to hang.
 *
 * What arrives is judged by [sharedFrom] before anything is sent anywhere, and
 * that is where the reasoning lives: content the sender marked sensitive is
 * refused outright, and text has to actually be text. This class is the shell
 * around it — an `Intent` in, a Toast out.
 */
class ShareTargetActivity : Activity() {

    /**
     * Scoped to this activity, and cancelled in [onDestroy].
     *
     * Safe for the reasons `StandingActionActivity` states, and it depends on the
     * same one: a share that queued goes on to `sendPending`, and the session that
     * raises comes back down under `NonCancellable` and under a bound — so a
     * window destroyed inside the drain can neither leave a session running nor
     * leave this coroutine waiting on a teardown that never returns.
     */
    private val scope = MainScope()

    /**
     * How many shares are still working. See [Verbs], which is the rule rather
     * than a copy of it — this window closes when the last share is done, and so
     * does the other surface's, for the same reason and out of the same value.
     */
    private val working = Verbs()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // No window animation: `Theme.Sharepaste.Invisible` sets
        // `windowAnimationStyle` to null, which is the declarative form of the
        // deprecated `overridePendingTransition` and applies to being finished
        // as well as to being started.
        act(intent)
    }

    /**
     * A second share, handed to the window the first one is still using.
     *
     * `singleTask` means a share arriving while one is still working does not
     * create a second instance: the platform delivers the `Intent` here and
     * [onCreate] does not run again. Without this override that share is dropped
     * in silence — the share sheet closes, the person believes they sent
     * something, and nothing was ever offered.
     *
     * [setIntent] keeps [getIntent] honest about what this window is doing. The
     * content itself is handed straight to [act], because an `ACTION_SEND`
     * carries it and nothing here waits for focus.
     */
    override fun onNewIntent(intent: Intent?) {
        super.onNewIntent(intent)
        setIntent(intent)
        act(intent)
    }

    /** Judge what arrived, say what became of it, and then send it. */
    private fun act(shared: Intent?) {
        working.began()
        scope.launch {
            try {
                val (receipt, queuedOn) = offerFrom(shared)
                // Reported first, sent second — see `StandingActionActivity`. A
                // share is an Offer made by somebody who is standing in another
                // app waiting to get on with something, so the answer comes now
                // and the upload happens behind it.
                reportReceipt("share", receipt)
                queuedOn?.let { repository.sendPending(it) }
            } finally {
                if (working.finished() && !isFinishing) finish()
            }
        }
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    private val repository: SharepasteRepository
        get() = (application as SharepasteApplication).repository

    /**
     * The Receipt to show, and — when there is one — the Pairing whose queue now
     * has something in it.
     */
    private suspend fun offerFrom(intent: Intent?): Pair<Receipt, String?> =
        when (val shared = sharedFrom(intent)) {
            // Refused before the text is read out of the intent at all, so
            // nothing the sender called sensitive is ever in a local of ours.
            Shared.Sensitive -> Receipt.Aloud(
                R.string.notice_refused_sensitive,
                R.string.share_refused_sensitive,
            ) to null

            Shared.NotText -> Receipt.Aloud(
                R.string.notice_nothing_to_send,
                R.string.share_refused_not_text,
            ) to null

            is Shared.Text -> offer(shared.text)
        }

    private suspend fun offer(text: String): Pair<Receipt, String?> = try {
        when (val attempt = repository.offerText(text)) {
            OfferAttempt.Unpaired ->
                Receipt.Aloud(R.string.notice_not_paired, R.string.action_unpaired) to null

            is OfferAttempt.Settled -> when (val outcome = attempt.outcome) {
                is OfferOutcome.Queued -> Receipt.Offered(outcome.pending) to attempt.userId
                is OfferOutcome.Recognised ->
                    Receipt.Recognised(outcome.pending) to attempt.userId

                is OfferOutcome.Rejected -> Receipt.Aloud(
                    offerRefusalLabel(outcome.reason),
                    offerRefusalMessage(outcome.reason),
                ) to null
            }
        }
    } catch (e: AppException) {
        Receipt.Aloud(R.string.notice_failed, R.string.offer_failed) to null
    }
}

/** What a share turned out to be. */
internal sealed interface Shared {

    /** Offerable text, exactly as the sending app wrote it. */
    data class Text(val text: String) : Shared

    /** The sender marked its content sensitive, so this is not ours to keep. */
    data object Sensitive : Shared

    /** Nothing text-like arrived, whatever the share sheet claimed. */
    data object NotText : Shared
}

/**
 * What an `ACTION_SEND` is offering, decided before anything is sent anywhere.
 *
 * A plain function over an `Intent` rather than three private methods on the
 * activity, because this is the whole of the judgement and it is the part worth
 * testing: an exported activity can be handed any intent at all, and every one
 * of the guards below exists because of a specific way of getting this wrong.
 *
 * **Sensitive wins over everything and is checked first**, so a marked payload
 * is refused before its text is read out of the intent at all.
 * [ClipDescription.EXTRA_IS_SENSITIVE] travels in two places in the wild — on the
 * `ClipData` an `ACTION_SEND` carries, and as a bare boolean extra on the intent
 * — and honouring it means *not sending*: an Offered Capture ends up encrypted
 * on the Relay and decrypted into every paired device's History, which is not
 * what a password manager means by Share. The constant is a compile-time
 * `String`, so naming it costs nothing below the API level that introduced it;
 * there the flag simply never appears, which is the right answer for a platform
 * whose share sheet cannot set it.
 *
 * **The text has to be text twice over.** The MIME type must match the text
 * wildcard, and `EXTRA_TEXT` is read with `getCharSequenceExtra`, which answers
 * `null` for an extra that is not a `CharSequence` rather than handing back
 * something a `toString()` would flatten. Ticket 10 had to stop
 * `ClipData.Item.coerceToText` answering a screenshot's `content://` URI — a
 * perfectly good `String` that an Offer would then encrypt and upload, so the
 * person's other devices received a path nothing could open in place of the
 * picture they meant to send. A loose `toString()` here is the same mistake in a
 * different hat.
 *
 * What text is *worth* sending is not decided here. Size, duplication and
 * emptiness are the core's one capture filter's business, and it is the filter
 * with the tests.
 */
internal fun sharedFrom(intent: Intent?): Shared {
    if (intent == null || intent.action != Intent.ACTION_SEND) return Shared.NotText
    val sensitive = intent.clipData?.description?.extras
        ?.getBoolean(ClipDescription.EXTRA_IS_SENSITIVE, false) == true ||
        intent.extras?.getBoolean(ClipDescription.EXTRA_IS_SENSITIVE, false) == true
    if (sensitive) return Shared.Sensitive
    val mime = intent.type ?: return Shared.NotText
    if (!ClipDescription.compareMimeTypes(mime, "text/*")) return Shared.NotText
    val text = intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()
    return if (text.isNullOrEmpty()) Shared.NotText else Shared.Text(text)
}
