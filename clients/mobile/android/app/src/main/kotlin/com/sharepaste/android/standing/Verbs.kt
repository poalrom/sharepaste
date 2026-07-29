package com.sharepaste.android.standing

/**
 * How many of an invisible window's verbs are still working.
 *
 * **The window closes when the last verb is done, not the first.** An Offer
 * spends up to ten seconds draining its queue; a Recall arriving beside it
 * finishes in a moment, and finishing the window on that would cancel the
 * Offer's upload — the person told it was sent, the Entry still on the phone,
 * nothing saying otherwise.
 *
 * Both Standing Action surfaces need exactly this, and neither may keep its own
 * copy of it. Both are `launchMode="singleTask"` with an empty `taskAffinity`,
 * so both are handed a second press rather than given a second window: the rule
 * is one rule and the share target had it written out a second time. [Presses]
 * holds this alongside the focus rule, which is the half only the clipboard
 * verbs have — a share carries its content in the `Intent` and waits for
 * nothing, so [ShareTargetActivity] holds one of these on its own.
 *
 * Touched only from the main thread — the lifecycle callbacks and a `MainScope`
 * coroutine — so it needs no synchronisation.
 */
internal class Verbs {

    private var working = 0

    /** Nothing is working, so the window has nothing to stay open for. */
    val idle: Boolean get() = working == 0

    /** A verb started, and the window owes it an ending. */
    fun began() {
        working++
    }

    /** A verb finished. Answers whether nothing is working any more. */
    fun finished(): Boolean {
        working--
        return idle
    }
}
