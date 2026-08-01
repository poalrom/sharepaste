package com.sharepaste.android.ui

import android.content.Context
import android.widget.Toast
import androidx.annotation.StringRes
import com.sharepaste.android.R

/**
 * Confirmation that a verb did what was asked, needing nothing back.
 *
 * Transient, and **the same whether the app was open or closed when the verb
 * ran** (CONTEXT.md). That second half is the whole reason this type replaced
 * `standing/Said.kt`: a Standing Action and a press on the History are one
 * operation reported one way, and two types for it were two idioms waiting to
 * drift. A Toast is the surface, because it is the only one a closed phone has
 * and because a confirmation that persists is a [Notice] instead.
 *
 * The split against [Notice] is by **outcome kind**, not by which path invoked
 * it. A Receipt says a thing happened and vanishes; a Notice says something
 * needs doing and waits to be dismissed. [Notice.RecalledFromCache] is the one
 * that proves the line is real: it looks like a confirmation and is not, because
 * ADR 0007 says it may never be silent.
 *
 * **[Recalled] is the only variant that may ever carry an Entry's text**, and
 * that is enforced by the shape rather than by a comment — everything else here
 * is a pair of resource ids with nowhere for a plaintext to hide. See ADR 0009
 * for why a Recall now says what it handed over at all, and [receiptLogged] for
 * why the log line still does not.
 */
sealed interface Receipt {

    /** An Offered Capture was taken. [pending] is the queue depth after it. */
    data class Offered(val pending: Long) : Receipt

    /**
     * An Entry is on this device's clipboard, and — when it can be said — which
     * one.
     *
     * [preview] is the Entry's Preview, the facade's own one-line rendering and
     * the same string a History row shows. Never the plaintext. The Offer
     * Receipt has no equivalent on purpose: the person supplied that content a
     * second ago, and only a Recall hands back something they did not choose.
     *
     * **Nullable, and one variant rather than two.** A Preview can genuinely be
     * missing — an Undecryptable Entry has none, and the read that fetches it can
     * fail — but "the Recall was confirmed" is the same outcome either way, and
     * `SHOW WHAT WAS RECALLED` has to silence both. Splitting the two cases across
     * variants is exactly how one of them escaped that switch: a guard written
     * against a type has to be able to name the whole of what it guards.
     */
    data class Recalled(val preview: String?) : Receipt

    /**
     * A [Notice] said out loud, because whatever raised it has no band.
     *
     * A Standing Action and a share both run without a screen, so a refusal, an
     * unpaired phone or a Recall that found nothing has nowhere to persist and
     * gets the transient surface instead. It is the same words under the same
     * label; only the container is missing.
     *
     * **Not a confirmation, and named so it cannot be mistaken for one.** These
     * are Notices by CONTEXT.md's definition and would be Notices on an open
     * screen. What they share with a Receipt is the Toast and nothing else —
     * which is why the `SHOW WHAT WAS RECALLED` switch, which silences a
     * confirmation, must never be written to match this.
     *
     * Two resource ids and nothing else. Every sentence it can carry takes no
     * format argument, so there is no slot for an Entry to reach.
     */
    data class Aloud(@param:StringRes val label: Int, @param:StringRes val sentence: Int) : Receipt
}

/** The outcome in a word or so, over the sentence. */
@StringRes
fun receiptLabel(receipt: Receipt): Int = when (receipt) {
    is Receipt.Offered -> R.string.notice_offered
    is Receipt.Recalled -> R.string.notice_recalled
    is Receipt.Aloud -> receipt.label
}

/**
 * The sentence, resolved. The one place a Preview is read into words.
 *
 * A Recall with no Preview to name falls back to the sentence that says only
 * that something is on the clipboard, rather than promising a name and leaving
 * the slot empty.
 */
fun receiptSentence(context: Context, receipt: Receipt): String = when (receipt) {
    is Receipt.Offered -> context.getString(R.string.offer_queued)
    is Receipt.Recalled -> receipt.preview
        ?.takeIf { it.isNotBlank() }
        ?.let { context.getString(R.string.receipt_recalled, it) }
        ?: context.getString(R.string.recall_done)

    is Receipt.Aloud -> context.getString(receipt.sentence)
}

/**
 * The same outcome for the log, and it is **not** [receiptSentence].
 *
 * The Toast is drawn over whatever app is in front, for a few seconds, for the
 * person who just pressed the control — an exposure ADR 0009 weighs and accepts.
 * A log line is none of those things: it is durable, it is readable by anything
 * holding `READ_LOGS` or a cable, and nobody asked for it. So the Recall logs
 * the sentence it used to log, with no Preview in it, and the acceptance
 * sequence's `adb logcat -s SharepasteStandingAction` still reads as one of this
 * app's own fixed sentences and nothing derived from an Entry.
 */
@StringRes
fun receiptLogged(receipt: Receipt): Int = when (receipt) {
    is Receipt.Offered -> R.string.offer_queued
    is Receipt.Recalled -> R.string.recall_done
    is Receipt.Aloud -> receipt.sentence
}

/**
 * Show a Receipt, from whichever of the two paths ran the verb.
 *
 * The label and then the sentence, which is the shape the History's band draws
 * too. `LENGTH_LONG` because a Preview is a line of text somebody has to read,
 * not a tick.
 *
 * The **application** context, and never an activity's: a Toast is queued by the
 * system rather than drawn by the window that asked for it, and the window a
 * Standing Action opens is finishing as this runs.
 */
fun showReceipt(context: Context, receipt: Receipt) {
    Toast.makeText(
        context.applicationContext,
        "${context.getString(receiptLabel(receipt))}\n${receiptSentence(context, receipt)}",
        Toast.LENGTH_LONG,
    ).show()
}
