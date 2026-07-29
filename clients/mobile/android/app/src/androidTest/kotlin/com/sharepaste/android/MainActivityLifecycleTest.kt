package com.sharepaste.android

import androidx.lifecycle.Lifecycle
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.rule.GrantPermissionRule
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The real activity drives the two edges.
 *
 * [SessionLifecycleTest] proves what each edge *does* to a live session.
 * This proves the activity is wired to them at all — that `onStart` reaches
 * `onEnterForeground` and `onStop` reaches `onLeaveForeground`, on the shipped
 * `MainActivity`, over a real `ActivityScenario`.
 *
 * Both halves are needed. A correct lifecycle object nobody calls is a phone that
 * never syncs; a wired activity whose edges do the wrong thing is a phone that
 * syncs wrongly. Neither test catches the other's failure.
 *
 * It runs on the app's own facade, with the app's own transport policy, and needs
 * no relay: `foreground` is a fact about the app, not about the network.
 */
@RunWith(AndroidJUnit4::class)
class MainActivityLifecycleTest {

    /**
     * Granted up front, and it is load-bearing for the test rather than for the
     * feature.
     *
     * `MainActivity` opens on the pairing flow, which asks for the camera. A
     * system permission dialog takes window focus, which leaves the activity
     * `PAUSED` and `moveToState(RESUMED)` waiting for a state it will never reach
     * — the first run of this test failed exactly that way. Granting it in advance
     * removes the dialog; the permission-refused path has its own test, in
     * [PairingMessagesTest], where no dialog is involved.
     */
    @get:Rule
    val cameraPermission: GrantPermissionRule =
        GrantPermissionRule.grant(android.Manifest.permission.CAMERA)

    @Test
    fun onStart_and_onStop_move_the_app_in_and_out_of_the_foreground() {
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.moveToState(Lifecycle.State.RESUMED)
            assertTrue(
                "onStart did not reach the state holder: the app does not think it is in front",
                awaitForeground(scenario, expected = true),
            )
            Evidence.log("activity      = RESUMED -> foreground=true")

            // CREATED is below STARTED, so this runs `onStop`. Not DESTROYED,
            // which would take the whole state holder with it and prove nothing
            // about the edge.
            scenario.moveToState(Lifecycle.State.CREATED)
            assertTrue(
                "onStop did not reach the state holder: the app still thinks it is in front",
                awaitForeground(scenario, expected = false),
            )
            Evidence.log("activity      = CREATED -> foreground=false")

            scenario.moveToState(Lifecycle.State.RESUMED)
            assertTrue(
                "a second onStart did not bring the app back to the foreground",
                awaitForeground(scenario, expected = true),
            )
            Evidence.log("activity      = RESUMED again -> foreground=true")
        }
    }

    /**
     * Polls the activity's own state holder until it reads [expected].
     *
     * Returns whether it got there, so both directions read the same way at the
     * call site: `assertTrue(awaitForeground(expected = false))` asserts that the
     * flag *reached* false. (Writing that as `assertFalse` is the obvious mistake
     * and it is how the first run of this test failed — against an activity that
     * logcat showed reaching `STOPPED` perfectly well.)
     *
     * A poll rather than a single read only to absorb the hop onto the main thread
     * that `onActivity` needs; both edges are recorded synchronously.
     */
    private fun awaitForeground(
        scenario: ActivityScenario<MainActivity>,
        expected: Boolean,
    ): Boolean {
        val deadline = System.nanoTime() + 10_000_000_000L
        while (System.nanoTime() < deadline) {
            var actual = !expected
            scenario.onActivity { actual = it.uiStateForTests.foreground }
            if (actual == expected) return true
            Thread.sleep(50)
        }
        return false
    }
}
