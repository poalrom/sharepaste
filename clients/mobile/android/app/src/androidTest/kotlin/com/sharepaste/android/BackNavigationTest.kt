package com.sharepaste.android

import android.view.KeyEvent
import androidx.activity.ComponentActivity
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.GrantPermissionRule
import com.sharepaste.android.scan.CameraProblem
import com.sharepaste.android.ui.PairingState
import com.sharepaste.android.ui.Screen
import com.sharepaste.android.ui.SharepasteApp
import com.sharepaste.android.ui.TAG_BACK_TO_HISTORY
import com.sharepaste.android.ui.TAG_HISTORY_LIST
import com.sharepaste.android.ui.TAG_PAIRINGS_SCREEN
import com.sharepaste.android.ui.TAG_PAIRING_SCREEN
import com.sharepaste.android.ui.UiState
import com.sharepaste.core.PairingSummary
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The system back gesture, answered per destination by the real `when`.
 *
 * **The fault this exists for.** There was no `BackHandler` anywhere in `app/src`,
 * so back did the platform's default on every screen: it finished the Activity.
 * Opening Settings and swiping back closed the app. That is the one bug here, and
 * it is invisible to every other test in this suite, because none of them press a
 * key the app never claimed.
 *
 * **Against [SharepasteApp], not against a screen.** The rule under test is not
 * "the Settings screen goes back"; it is that each branch of the app's one `when`
 * answers the gesture with the action that branch's own `◂` fires. A test that
 * rendered `PairingsScreen` directly would compose no `BackHandler` at all — they
 * live one level out, beside the branch they steer — and would pass with back
 * still closing the app.
 *
 * **A hand-made [UiState], hoisted.** Three destinations, and the difference
 * between a phone that holds a Pairing and one that never has, are facts about a
 * snapshot, so they are handed in rather than arrived at. The state is a
 * `mutableStateOf` the test also writes, and the two navigating members of
 * [noActions] write to it: that is what makes "back fired `openHistory`" and "the
 * History is on screen" one assertion instead of two hopeful ones.
 *
 * **The camera hazard, and how it is dodged.** `Screen.Pairing` composes
 * `rememberCameraAccess`, which asks for `CAMERA` once per visit. A system
 * permission dialog takes window focus, and an injected key would then land on
 * the dialog rather than on this app — the same hazard [PhoneUnderTest] bends its
 * `when` to avoid and [MainActivityLifecycleTest] grants its way past. This grants
 * its way past too, and additionally hands the flow a `camera` of
 * [CameraProblem.NoCamera], which is the one branch of the viewfinder that
 * composes no `CameraPreview`: no CameraX binding, no sensor and no frames, on a
 * screen whose subject here is a key press.
 */
@RunWith(AndroidJUnit4::class)
class BackNavigationTest {

    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    /**
     * Granted for the test rather than for the feature.
     *
     * See the class note: `Screen.Pairing` asks for the camera as it composes, and
     * the dialog that asks would take the window focus this test injects into.
     * Rule order does not matter — the composition happens inside the test body,
     * long after both rules have been applied — and the refused-permission path
     * keeps its own test in [PairingMessagesTest], where no dialog is involved.
     */
    @get:Rule
    val cameraPermission: GrantPermissionRule =
        GrantPermissionRule.grant(android.Manifest.permission.CAMERA)

    private val instrumentation = InstrumentationRegistry.getInstrumentation()

    /**
     * What the app is showing, and the only thing back is allowed to change.
     *
     * Hoisted out of the composition because the two navigating actions write to
     * it: the gesture's whole observable effect is the next snapshot, and reading
     * it here is how a fired lambda is told apart from a lambda never reached.
     */
    private val shown = mutableStateOf(UiState())

    /**
     * Held from before the press, on purpose.
     *
     * A finished Activity is on its way to `DESTROYED`, and `compose.activity`
     * goes through `ActivityScenario.onActivity`, which refuses once it gets
     * there. The reference taken while it was alive answers `isFinishing`
     * afterwards regardless.
     */
    private lateinit var activity: ComponentActivity

    /** A Pairing this phone holds, exactly as `listPairings` hands one over. */
    private val holder = pairing(userId = "aaaa-1111", username = "the laptop's account")

    private fun open(initial: UiState) {
        shown.value = initial
        compose.setContent {
            SharepasteApp(
                state = shown.value,
                actions = noActions(
                    openPairings = { shown.value = shown.value.copy(screen = Screen.Pairings) },
                    openHistory = { shown.value = shown.value.copy(screen = Screen.History) },
                ),
            )
        }
        activity = compose.activity
    }

    /** Settings, on a phone that holds one Pairing. */
    private fun settings() = UiState(
        screen = Screen.Pairings,
        pairings = listOf(holder),
        activeUserId = holder.userId,
        foreground = true,
    )

    /**
     * The pairing flow, with the viewfinder stood down.
     *
     * [CameraProblem.NoCamera] rather than `null`: it is the branch that renders
     * the absent-camera note in place of `Viewfinder`, which is the only way to
     * compose this screen without binding CameraX. Which problem the *device*
     * reports never reaches this snapshot — `noActions` swallows
     * `setCameraProblem` — so the note stays put for the length of the test.
     */
    private fun pairingFlow(vararg pairings: PairingSummary) = UiState(
        screen = Screen.Pairing,
        pairing = PairingState(camera = CameraProblem.NoCamera),
        pairings = pairings.toList(),
        foreground = true,
    )

    /**
     * System back, as the platform delivers one.
     *
     * **Why this and not `Espresso.pressBack()`.** `androidx.test.espresso` is not
     * on this module's androidTest *compile* classpath. It arrives only through
     * `androidx.compose.ui:ui-test`, which lists `espresso-core` in its runtime
     * elements and not its api elements, so an `import androidx.test.espresso.…`
     * costs a new `androidTestImplementation` in `app/build.gradle.kts` plus a
     * version-catalog entry — a dependency this branch would be paying for one
     * key press. Do not add it to "fix" this call.
     *
     * Nothing is lost. `sendKeyDownUpSync` is the same injection Espresso performs
     * underneath; what `pressBack()` adds on top is throwing
     * `NoActivityResumedException` when the press closed the app, and the two
     * unhandled cases below assert that outcome directly as `isFinishing` — which
     * is the ticket's own wording ("the app stays open") rather than the name of
     * an exception, and which distinguishes *this* press from an Activity that was
     * already on its way out.
     *
     * `KEYCODE_BACK` reaches `OnBackPressedDispatcher` on every supported level:
     * the manifest does not opt into `enableOnBackInvokedCallback`, so this is the
     * legacy path through `Activity.onBackPressed` on API 33+ as well as below it.
     * `sendKeyDownUpSync` returns once the app has finished handling the event,
     * and `waitForIdleSync` then drains what the handler posted — but neither
     * waits for a frame, which is why every assertion about the screen goes
     * through [awaitLeaving] rather than reading the snapshot straight after.
     */
    private fun pressBack() {
        instrumentation.sendKeyDownUpSync(KeyEvent.KEYCODE_BACK)
        instrumentation.waitForIdleSync()
    }

    /**
     * Waits for the app to leave [from], and says where it went.
     *
     * Deliberately does not take the expected destination: the caller asserts
     * that, and the drift test below compares two answers it must not have
     * presupposed. Failure is reported as an [AssertionError] naming the screen
     * the app is stuck on, because a bare `waitUntil` timeout says only that five
     * seconds passed.
     */
    private fun awaitLeaving(from: Screen): Screen {
        try {
            compose.waitUntil(WAIT_MS) { shown.value.screen != from }
        } catch (e: Throwable) {
            throw AssertionError("the app never left $from: nothing answered the gesture", e)
        }
        compose.waitForIdle()
        return shown.value.screen
    }

    /**
     * Whether the Activity started finishing, within the same window.
     *
     * `finish()` is synchronous, but the press that causes it crosses a process
     * boundary and lands on the main thread, so a single read straight afterwards
     * is a race. `waitForIdleSync` leads each attempt for two reasons: it yields
     * to the main thread, and it is the barrier that makes `mFinished` — a plain
     * boolean behind [android.app.Activity.isFinishing], written over there and
     * read from here — visible to this thread at all.
     *
     * Returns the flag rather than asserting it, so the call site states which
     * direction it expected. Both call sites also assert the flag was *false*
     * before pressing: read only afterwards, it cannot tell an Activity this
     * press finished from one that was already on its way out.
     */
    private fun awaitFinishing(): Boolean {
        val deadline = System.nanoTime() + WAIT_MS * 1_000_000L
        while (System.nanoTime() < deadline) {
            instrumentation.waitForIdleSync()
            if (activity.isFinishing) return true
            Thread.sleep(50)
        }
        return false
    }

    /**
     * Back from Settings shows the History, and the app is still open.
     *
     * The bug this ticket exists for, stated as the two halves that both have to
     * hold. Arriving on the History is not enough on its own — a back that did
     * nothing at all would leave the History nowhere in sight — and an Activity
     * that is not finishing is not enough either, since that is equally true of a
     * gesture nobody answered on a screen that never moves. Together they are the
     * sentence in the acceptance: the History is in front, and the app did not
     * close to get there.
     */
    @Test
    fun back_from_settings_shows_the_history_and_the_app_stays_open() {
        open(settings())
        compose.onNodeWithTag(TAG_PAIRINGS_SCREEN).assertIsDisplayed()
        assertFalse("the Activity was already finishing before the press", activity.isFinishing)

        pressBack()

        assertFalse(
            "the app closed on a back press from Settings, which is the whole bug",
            activity.isFinishing,
        )
        assertEquals(
            "back on Settings has to fire openHistory — with no handler it finishes the app",
            Screen.History,
            awaitLeaving(Screen.Pairings),
        )
        compose.onNodeWithTag(TAG_HISTORY_LIST).assertIsDisplayed()
        Evidence.log("back/Settings = Screen.History on screen, activity still alive")
    }

    /**
     * The gesture and the `◂` are one action, and cannot drift apart.
     *
     * Two ways out of a screen is two things to keep in step, and the second one
     * is the one nobody re-reads. Asserted as *the same resulting screen* from the
     * same starting snapshot rather than as "both call `openHistory`", because the
     * failure worth catching is the day one of them is pointed somewhere else — at
     * `openAddPairing`, at a screen that no longer exists — and a test that only
     * knew each one fired *something* would still pass.
     *
     * Both halves run in one composition: a compose rule permits exactly one
     * `setContent`, so the snapshot is put back to Settings between them. Pinning
     * the shared answer to `Screen.History` as well is what stops two identical
     * no-ops from agreeing with each other.
     */
    @Test
    fun back_from_settings_is_the_same_action_as_the_back_glyph() {
        open(settings())

        pressBack()
        val byGesture = awaitLeaving(Screen.Pairings)

        shown.value = settings()
        compose.waitForIdle()
        compose.onNodeWithTag(TAG_BACK_TO_HISTORY).performClick()
        val byGlyph = awaitLeaving(Screen.Pairings)

        assertEquals(
            "the gesture and $TAG_BACK_TO_HISTORY left the app on different screens",
            byGlyph,
            byGesture,
        )
        assertEquals(
            "both ways out of Settings have to reach the History",
            Screen.History,
            byGesture,
        )
        Evidence.log("back == glyph = $byGesture from the gesture and from $TAG_BACK_TO_HISTORY")
    }

    /**
     * Back out of the pairing flow reaches Settings when there is a Settings to
     * reach.
     *
     * The flow is two different screens wearing one name. Reached through `ADD A
     * PAIRING` it is a step with somewhere behind it; on a fresh install it is the
     * whole app. The app tells them apart by `pairings.isNotEmpty()` and nothing
     * else — no remembered origin, no back stack — so this is the half that proves
     * the derivation points somewhere, and
     * [back_from_the_pairing_flow_with_no_pairings_is_unhandled] is the half that
     * proves it is a derivation at all.
     */
    @Test
    fun back_from_the_pairing_flow_shows_settings_when_the_phone_holds_a_pairing() {
        open(pairingFlow(holder))
        compose.onNodeWithTag(TAG_PAIRING_SCREEN).assertIsDisplayed()
        assertFalse("the Activity was already finishing before the press", activity.isFinishing)

        pressBack()

        assertFalse(
            "back out of the pairing flow closed the app instead of returning to Settings",
            activity.isFinishing,
        )
        assertEquals(
            "a pairing flow reached from Settings has to go back to Settings",
            Screen.Pairings,
            awaitLeaving(Screen.Pairing),
        )
        compose.onNodeWithTag(TAG_PAIRINGS_SCREEN).assertIsDisplayed()
        Evidence.log("back/Pairing  = Screen.Pairings on screen, 1 Pairing held")
    }

    /**
     * On a phone that has never paired, the pairing flow is the root, and back
     * belongs to the platform.
     *
     * `enabled = false` is a claim about who answers, and the only way to read it
     * back is to press and watch the app not answer. The assertion is deliberately
     * *not* a screen change: there is no screen to change to, and the History
     * behind this one is empty — being sent there is worse than leaving, which is
     * why the on-screen `◂` is absent here too. So what is asserted is that the
     * snapshot did not move and that the Activity **flipped** to finishing: read
     * only afterwards, the flag could not tell this press from an Activity that
     * was already on its way out.
     *
     * Its own method rather than a second half of
     * [back_from_the_history_is_unhandled], because an Activity can only be
     * finished once and a compose rule hosts exactly one.
     */
    @Test
    fun back_from_the_pairing_flow_with_no_pairings_is_unhandled() {
        open(pairingFlow())
        compose.onNodeWithTag(TAG_PAIRING_SCREEN).assertIsDisplayed()
        assertFalse("the Activity was already finishing before the press", activity.isFinishing)

        pressBack()

        assertTrue(
            "back was claimed by the app on a screen with nothing behind it",
            awaitFinishing(),
        )
        assertEquals(
            "nothing in the app may answer back on the launch screen of a phone with no Pairing",
            Screen.Pairing,
            shown.value.screen,
        )
        Evidence.log("back/no-pair  = unhandled, activity finishing, screen unchanged")
    }

    /**
     * The History is the root, and back exits.
     *
     * The standard Android answer, and it has to be asserted rather than assumed:
     * a `BackHandler` added here to keep the composition uniform — or one from
     * Settings left composed across a screen change — would swallow the gesture
     * and strand the person in an app they cannot leave, which is a worse bug than
     * the one this ticket fixes.
     */
    @Test
    fun back_from_the_history_is_unhandled() {
        open(
            UiState(
                screen = Screen.History,
                pairings = listOf(holder),
                activeUserId = holder.userId,
                foreground = true,
            ),
        )
        compose.onNodeWithTag(TAG_HISTORY_LIST).assertIsDisplayed()
        assertFalse("the Activity was already finishing before the press", activity.isFinishing)

        pressBack()

        assertTrue(
            "back was claimed on the root screen, so the app cannot be left",
            awaitFinishing(),
        )
        assertEquals(
            "the History is the root: no destination may claim back here",
            Screen.History,
            shown.value.screen,
        )
        Evidence.log("back/History  = unhandled, activity finishing, screen unchanged")
    }

    private companion object {
        /** The same five seconds `HistoryListTest` and `PhoneUnderTest` allow. */
        const val WAIT_MS = 5_000L
    }
}
