package com.sharepaste.android.ui

/**
 * How the list gets to the head of the History, when something has put a row
 * there and the list is to be there too.
 *
 * The signal used to be a `Flow<Long>` of Entry ids and nothing ever read the
 * id: index 0 is the destination whichever Entry took it, so
 * `animateScrollToItem(0)` ignored what it was told. What the screen needs
 * instead is *which motion*, and that is one bit the emitter has and the
 * collector cannot derive — the same discipline [SharepasteViewModel.headMoves]
 * already demands of the causes (ADR 0011: a rule derived from the list rots the
 * moment a Use can change the head).
 *
 * Two motions, one rule with a phase (ADR 0019). Nothing else moves the
 * **Place**: not an arrival over the live stream, not a remote Use, not a
 * delete, not the hundred-row prune, and not a flush.
 */
enum class HeadMove {
    /**
     * Straight there, with no animation.
     *
     * The open's own motion. Nothing changed under anybody's eyes — nobody was
     * looking at the list at all — so there is no change to show, and an
     * animation would perform one for an empty room. It is also what a Viewed
     * Pairing switch does, where the rows are a different list and the row
     * somebody was reading is not in it.
     */
    Jump,

    /**
     * Follow the row, animated.
     *
     * A **Use this phone made**, which is the one thing that moves the Place once
     * the open's jump is spent: a Recall, and an Offer whose Capture is a Use of
     * its own (CONTEXT.md). Here the motion is the point — the person did
     * something, a row moved because of it, and the animation is what says those
     * two facts are the same fact.
     */
    Follow,
}

/**
 * The one jump an open is owed.
 *
 * A phone that was away opens at the head (ADR 0019): the first **Catch-Up** of
 * a foreground that found anything puts the list at index 0, and nothing after
 * it does. The rule is a sequence rather than a predicate, so it is a small
 * object with a memory rather than a `when` — armed at an open, spent by
 * whichever of two causes lands first, and closed until the next open.
 *
 * **Why the arming is the shell's.** The core cannot say which Catch-Up is the
 * open's: every reconnect re-enters the same `Connecting` → catch up → `Online`
 * sequence, and every Catch-Up announces a plain `HistoryChanged` either way. So
 * the fact that distinguishes them — a foreground just began — is one only
 * `SharepasteViewModel.onEnterForeground` holds, and no core change is made for
 * it.
 *
 * **Why there is no clock in it.** ADR 0007 makes being out of contact the
 * nominal case, so an open with no signal followed by a Catch-Up minutes later
 * is common rather than an edge. A time bound would have surrendered exactly
 * that case; the edge that replaces it is a hand on the list, which is what
 * somebody actually means by "I already moved somewhere". A Catch-Up twenty
 * minutes into a session can therefore still move the list — but only for
 * somebody who has no **Place** to lose.
 *
 * Main-thread only and deliberately unsynchronised: every caller is the state
 * holder on `Dispatchers.Main.immediate` — the lifecycle edges, the core event
 * collector, and the screen's report that a hand moved the list.
 */
class OpenJump {

    private var armed = false

    /** A foreground began. Whatever the last one spent, this one is owed a jump. */
    fun opened() {
        armed = true
    }

    /**
     * Something changed the Viewed Pairing's History. `true` at most once per
     * open, and only for the first such change.
     */
    fun spend(): Boolean {
        val jump = armed
        armed = false
        return jump
    }

    /**
     * Give the jump up without taking it: somebody has a Place worth keeping,
     * or is about to be moved for a better reason.
     */
    fun close() {
        armed = false
    }
}
