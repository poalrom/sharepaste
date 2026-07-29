package com.sharepaste.android

import android.content.Intent
import com.sharepaste.android.standing.Presses
import com.sharepaste.android.standing.ShareTargetActivity
import com.sharepaste.android.standing.StandingActionActivity
import com.sharepaste.android.standing.StandingActions
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * A second press of a Standing Action does the second verb.
 *
 * **What was wrong.** Both Standing Action activities are
 * `launchMode="singleTask"` with an empty `taskAffinity`, so a second press does
 * not open a second window — the platform hands the new `Intent` to the instance
 * already running. Neither activity overrode `onNewIntent`, so that `Intent` went
 * nowhere: the window merely re-focused, the dispatch found it had already acted,
 * and `getIntent` still answered with the **first** press's. Pressing Recall while
 * an Offer was inside its ten-second drain therefore did nothing at all and said
 * nothing about it, and the manifest comment claimed it worked.
 *
 * **Why this is a JVM test and not an instrumented one.** Making a real second
 * press land on a real live window needs the first verb to still be working, and
 * an Offer only takes any time at all if it has something to upload. Inside
 * `connectedDebugAndroidTest` it cannot have: a Standing Action necessarily runs
 * on `SharepasteApplication.repository`, whose policy is the shipped
 * `requireHttps = true`, and the only Relay reachable from an emulator is
 * cleartext — so the Offer fails at `load_active_membership` with `InsecureRelay`
 * in a millisecond and the window is gone before a second press can be fired.
 * (Measured: the report reads *"Sharepaste could not offer that."*) That is the
 * same wall ticket 12 hit, and its answer was a host-driven sequence with
 * force-stops, which runs neither in CI nor in an ordinary run. Splitting the
 * judgement out into [Presses] puts the part that was actually wrong somewhere
 * both can reach it.
 *
 * [the_two_standing_action_activities_adopt_a_new_intent] closes the other half:
 * [Presses] cannot be reached at all unless `onNewIntent` exists, and its absence
 * was the bug.
 */
class StandingActionPressesTest {

    /**
     * The ordinary press: recorded when it arrives, run when focus allows.
     *
     * Focus is not ceremony. Since Android 10 the clipboard is readable only by
     * the application holding window focus, so an Offer that ran on `onCreate`
     * would read an empty clipboard and report that nothing had been copied.
     */
    @Test
    fun a_press_waits_for_focus_and_then_runs() {
        val presses = Presses()

        assertNull("a press cannot run before the window can read the clipboard", presses.arrived(OFFER))
        assertTrue("nothing is working yet, so the window may still close", presses.idle)

        assertEquals(OFFER, presses.focus(true))
        assertFalse("a verb is working, so the window must stay", presses.idle)
    }

    /**
     * Focus arriving twice runs the verb once.
     *
     * A window can gain focus more than once for a single press — a transition
     * completing, a dialog dismissing — and an Offer that ran twice would put the
     * same clipboard on the Relay twice.
     */
    @Test
    fun focus_arriving_again_does_not_run_the_verb_twice() {
        val presses = Presses()
        presses.arrived(OFFER)

        assertEquals(OFFER, presses.focus(true))
        assertNull("the same press ran a second time", presses.focus(true))
        assertNull("and a third", presses.focus(true))
    }

    /**
     * **The regression.** A second press, while the first is still working, runs
     * the second verb.
     *
     * Recall pressed during an Offer's drain used to do nothing whatsoever.
     */
    @Test
    fun a_second_press_while_the_first_is_still_working_runs_the_second_verb() {
        val presses = Presses()
        presses.arrived(OFFER)
        assertEquals(OFFER, presses.focus(true))

        assertEquals(
            "a second press while the first verb was still working was dropped",
            RECALL,
            presses.arrived(RECALL),
        )
    }

    /**
     * A press that arrives before the window can act supersedes the one before
     * it.
     *
     * One slot rather than a queue: two presses landing before focus has even
     * arrived are somebody correcting themselves in the moment it takes an
     * invisible window to come up, not somebody asking for both. Running the
     * superseded verb would be the worse answer of the two — it would offer a
     * clipboard the person had just decided not to offer.
     */
    @Test
    fun a_press_before_the_window_can_act_supersedes_the_one_before_it() {
        val presses = Presses()
        presses.arrived(OFFER)
        presses.arrived(RECALL)

        assertEquals(RECALL, presses.focus(true))
        assertNull("the superseded press ran as well", presses.focus(true))
    }

    /**
     * The window stays open until the **last** verb is done.
     *
     * This is what stops a Recall pressed during an Offer's drain from cancelling
     * that drain when it finishes first. The Entry would have stayed on the phone,
     * the person would have been told it was sent, and nothing would have said
     * otherwise.
     */
    @Test
    fun the_window_stays_open_until_the_last_verb_is_done() {
        val presses = Presses()
        presses.arrived(OFFER)
        presses.focus(true)
        presses.arrived(RECALL)

        assertFalse(
            "the Recall finishing closed the window while the Offer was still draining",
            presses.finished(),
        )
        assertFalse(presses.idle)

        assertTrue("the last verb finished and the window did not close", presses.finished())
        assertTrue(presses.idle)
    }

    /**
     * A window that lost focus without ever acting has nothing to stay open for.
     *
     * `onStop` finishes on this, so an invisible activity cannot sit in a task
     * forever after a press on a locked screen, where focus never arrives at all.
     */
    @Test
    fun a_press_that_never_got_focus_leaves_nothing_working() {
        val presses = Presses()
        presses.arrived(OFFER)

        assertNull(presses.focus(false))
        assertTrue("the window would sit in its task forever", presses.idle)
    }

    /** An `Intent` with no action asks for nothing, and does not hold the window. */
    @Test
    fun an_intent_with_no_action_asks_for_nothing() {
        val presses = Presses()

        assertNull(presses.arrived(null))
        assertNull(presses.focus(true))
        assertTrue(presses.idle)
    }

    /**
     * Both Standing Action activities override `onNewIntent`.
     *
     * Structural, and deliberately so: the bug was **zero occurrences of
     * `onNewIntent` under `app/src/main`**, and no assertion about [Presses] can
     * notice that, because a `Presses` nothing calls is a `Presses` that passes
     * every test above. Both activities are `singleTask`, so both are handed a
     * second press rather than given a second window, and both need the override —
     * `ShareTargetActivity` does not even run `onCreate` again, so a second share
     * arriving during the first one's drain would be swallowed whole: the share
     * sheet closes, the person believes they sent something, and nothing was ever
     * offered.
     *
     * `MergedManifestTest.the_standing_action_windows_are_reused_rather_than_stacked`
     * is the other half — it pins the `singleTask` these overrides exist for, so
     * that neither half can quietly stop mattering.
     */
    @Test
    fun the_two_standing_action_activities_adopt_a_new_intent() {
        listOf(StandingActionActivity::class.java, ShareTargetActivity::class.java).forEach { activity ->
            val declared = activity.declaredMethods.filter { it.name == "onNewIntent" }
            assertTrue(
                "${activity.simpleName} does not override onNewIntent. It is `singleTask`, so a " +
                    "second press is delivered to the window already open — an activity that " +
                    "does not adopt the new Intent drops the verb on the floor, with no feedback.",
                declared.any { it.parameterTypes.contentEquals(arrayOf(Intent::class.java)) },
            )
        }
    }

    private companion object {
        const val OFFER = StandingActions.ACTION_OFFER
        const val RECALL = StandingActions.ACTION_RECALL_LATEST
    }
}
