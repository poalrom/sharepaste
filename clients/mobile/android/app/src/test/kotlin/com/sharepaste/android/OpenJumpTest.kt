package com.sharepaste.android

import com.sharepaste.android.ui.OpenJump
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The one jump an open is owed, and the two things that spend it.
 *
 * ADR 0019's whole rule is a sequence — armed at an open, spent by whichever of
 * two causes arrives first, and dead until the next open — so it is the
 * ordering that has to be pinned rather than any single answer. A JVM test can
 * do it because the gate needs no Relay, no `LazyListState` and no device: the
 * two facts it composes reach it as calls, from
 * `SharepasteViewModel.onEnterForeground` and from the History Screen's own
 * report that a hand moved the list.
 *
 * **What every case here defends is the second jump.** A gate that never closed
 * would turn the ADR's open-only rule back into the arrival rule it replaced,
 * and it would do it silently: the first jump — the one anybody would check by
 * hand — is right in all of them.
 */
class OpenJumpTest {

    /**
     * A process that has never been in front owes nothing.
     *
     * The state holder is constructed before the first `onStart` and the core
     * can raise `HistoryChanged` from a session that outlived the last one, so
     * "closed until opened" is the only safe starting position.
     */
    @Test
    fun a_gate_nobody_opened_does_not_jump() {
        assertFalse(
            "a History change before the first foreground must not move the list: " +
                "nobody has opened this phone, so there is no open to be at the head of",
            OpenJump().spend(),
        )
    }

    /** The case the record exists for: opened, then the Catch-Up finds something. */
    @Test
    fun the_first_change_after_an_open_jumps() {
        val gate = OpenJump()
        gate.opened()
        assertTrue("the open's first History change must put the list at the head", gate.spend())
    }

    /**
     * A second change in the same foreground leaves the Place alone.
     *
     * This is the mid-session rule, and it is the same rule: the gate is spent,
     * so an arrival, a remote Use or a flush that lands later moves nothing.
     */
    @Test
    fun a_second_change_in_the_same_foreground_does_not_jump() {
        val gate = OpenJump()
        gate.opened()
        gate.spend()
        assertFalse(
            "only the first change of a foreground jumps. Later ones land under somebody's " +
                "eyes, and chasing them costs the reader their Place",
            gate.spend(),
        )
    }

    /**
     * A hand on the list spends the gate without moving anything.
     *
     * The half that keeps the rule alive on this product's network. There is no
     * clock in the gate — ADR 0007 makes being out of contact nominal, so an
     * open with no signal followed by a late Catch-Up is common — and the edge
     * that replaces a timeout is somebody having touched the list.
     */
    @Test
    fun a_hand_on_the_list_spends_the_gate() {
        val gate = OpenJump()
        gate.opened()
        gate.close()
        assertFalse(
            "somebody who has scrolled has a Place to lose, however late the Catch-Up is",
            gate.spend(),
        )
    }

    /**
     * A late Catch-Up still jumps for somebody who has not touched the list.
     *
     * The case a time bound would have surrendered: twenty minutes of no signal
     * is a nominal open here, and the person is still looking at the list they
     * opened.
     */
    @Test
    fun an_untouched_list_still_jumps_however_late_the_change_is() {
        val gate = OpenJump()
        gate.opened()
        // Nothing at all happens in between, which is what a phone with no route
        // to the Relay looks like from here: no change to announce and no hand.
        assertTrue("a late Catch-Up must still reach somebody with no Place to lose", gate.spend())
    }

    /** Every open owes its own jump, whatever spent the last one. */
    @Test
    fun the_next_open_is_owed_its_own_jump() {
        val gate = OpenJump()
        gate.opened()
        gate.spend()
        gate.opened()
        assertTrue(
            "a phone put down and picked up again is a phone that was away, and the reason " +
                "for the jump is being away rather than being new",
            gate.spend(),
        )
    }

    /** A hand from the last foreground is not a hand in this one. */
    @Test
    fun a_hand_before_the_open_does_not_spend_the_open() {
        val gate = OpenJump()
        gate.close()
        gate.opened()
        assertTrue("the Place a hand cost belonged to the foreground it was in", gate.spend())
    }
}
