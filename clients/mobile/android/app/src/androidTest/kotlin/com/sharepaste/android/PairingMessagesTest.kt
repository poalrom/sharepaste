package com.sharepaste.android

import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.scan.CameraProblem
import com.sharepaste.android.scan.cameraProblem
import com.sharepaste.android.ui.PairAttempt
import com.sharepaste.android.ui.PairingState
import com.sharepaste.android.ui.PairingScreen
import com.sharepaste.android.ui.SharepasteTheme
import com.sharepaste.android.ui.TAG_CAMERA_ABSENT
import com.sharepaste.android.ui.TAG_CAMERA_RECHECK
import com.sharepaste.android.ui.TAG_CAMERA_REFUSED
import com.sharepaste.android.ui.TAG_CODE_FIELD
import com.sharepaste.android.ui.TAG_CODE_SCANNED
import com.sharepaste.android.ui.TAG_FAILURE
import com.sharepaste.android.ui.TAG_FAILURE_DETAIL
import com.sharepaste.android.ui.TAG_LABEL_FIELD
import com.sharepaste.android.ui.TAG_PAIR_BUTTON
import com.sharepaste.android.ui.TAG_VIEWFINDER
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The three failure modes each land somewhere, and each says something different.
 *
 * "Not one generic failure" is the criterion, so the assertion is on the *words*
 * the person reads, not on which branch a `when` took. Every message is checked
 * to be non-empty and distinct from the other two, which is the property that
 * quietly breaks the first time somebody consolidates them.
 *
 * The expired-code path is the third one, and it is proven end to end against the
 * live relay in [ExpiredCodeTest] — a real 120-second slot really expiring —
 * rather than by handing this screen a state and taking its word for it.
 *
 * **The slot those two camera failures share is also where a scan reports.** Which
 * of the three things occupies it — no camera, no permission, or a code already
 * read — is the whole of what this screen says about the camera, so the states that
 * hide the viewfinder are asserted here beside the words that replace it.
 */
@RunWith(AndroidJUnit4::class)
class PairingMessagesTest {

    @get:Rule
    val compose = createComposeRule()

    private val resources = InstrumentationRegistry.getInstrumentation().targetContext.resources

    @Test
    fun a_refused_camera_permission_says_the_permission_is_off_and_offers_the_other_way_in() {
        var rechecks = 0
        compose.setContent {
            SharepasteTheme {
                PairingScreen(
                    state = PairingState(camera = CameraProblem.PermissionRefused),
                    onLabelChange = {},
                    onCodeChange = {},
                    onPair = {},
                    onDismissFailure = {},
                    onRecheckCamera = { rechecks += 1 },
                )
            }
        }

        compose.onNodeWithTag(TAG_CAMERA_REFUSED).assertIsDisplayed()
        val message = resources.getString(R.string.camera_permission_refused)
        compose.onNodeWithText(message, substring = true).assertIsDisplayed()

        // The fallback has to be present *on the same screen*, not behind a
        // retry: with the camera off it is the only way in.
        compose.onNodeWithTag(TAG_CODE_FIELD).assertIsDisplayed()

        // And beside the refusal, the way back from it. The flow notices a grant
        // on its own, but the control is what somebody who has just come back from
        // Settings can press instead of trusting that.
        compose.onNodeWithTag(TAG_CAMERA_RECHECK).performScrollTo().assertIsDisplayed().performClick()
        assertEquals("the re-check control must reach the permission", 1, rechecks)
        Evidence.log("camera off    = $message")
    }

    @Test
    fun an_absent_camera_says_there_is_no_camera_rather_than_asking_for_a_permission() {
        showPairing(PairingState(camera = CameraProblem.NoCamera))

        compose.onNodeWithTag(TAG_CAMERA_ABSENT).assertIsDisplayed()
        val message = resources.getString(R.string.camera_absent)
        compose.onNodeWithText(message, substring = true).assertIsDisplayed()
        compose.onNodeWithTag(TAG_CODE_FIELD).assertIsDisplayed()
        Evidence.log("no camera     = $message")
    }

    @Test
    fun an_expired_code_says_the_code_expired_and_why_it_is_short_lived() {
        showPairing(PairingState(attempt = PairAttempt.Failed(R.string.pair_code_expired)))

        // Under the button that failed, at the foot of a flow taller than a
        // phone. That is where a person who just pressed Pair is looking.
        compose.onNodeWithTag(TAG_FAILURE).performScrollTo().assertIsDisplayed()
        val message = resources.getString(R.string.pair_code_expired)
        compose.onNodeWithText(message, substring = true).assertIsDisplayed()
        Evidence.log("expired code  = $message")
    }

    /**
     * The three are actually different sentences.
     *
     * This is the assertion the criterion is really about. Three separate string
     * resources that happen to hold the same words would satisfy every test above.
     */
    @Test
    fun the_three_failure_messages_are_distinct() {
        val refused = resources.getString(R.string.camera_permission_refused)
        val absent = resources.getString(R.string.camera_absent)
        val expired = resources.getString(R.string.pair_code_expired)
        listOf("permission refused" to refused, "no camera" to absent, "expired code" to expired)
            .forEach { (what, text) ->
                assertTrue("the $what message is empty", text.isNotBlank())
            }
        assertNotEquals("permission-refused and no-camera share a message", refused, absent)
        assertNotEquals("permission-refused and expired share a message", refused, expired)
        assertNotEquals("no-camera and expired share a message", absent, expired)
        assertEquals(3, setOf(refused, absent, expired).size)
        Evidence.log("distinct msgs = 3 of 3")
    }

    /**
     * A cleartext Relay shows the core's own sentence underneath the app's.
     *
     * The core names the address and the reason, which is the specific fact a
     * person needs, and the app's wording is what makes it actionable. Both, in
     * that order.
     */
    @Test
    fun a_cleartext_relay_shows_the_cores_explanation_as_well() {
        val fromTheCore = "that relay is plain HTTP and this app requires HTTPS"
        showPairing(
            PairingState(attempt = PairAttempt.Failed(R.string.pair_insecure_relay, fromTheCore)),
        )
        compose.onNodeWithTag(TAG_FAILURE).performScrollTo().assertIsDisplayed()
        compose.onNodeWithTag(TAG_FAILURE_DETAIL).assertIsDisplayed()
        compose.onNodeWithText(fromTheCore, substring = true).assertIsDisplayed()
        Evidence.log("insecure msg  = ${resources.getString(R.string.pair_insecure_relay)}")
    }

    /**
     * A scan fills the code field and takes the viewfinder away.
     *
     * **The fault this pins.** A scan used to pair, so on a fresh install — where
     * the square is the first thing anybody points the phone at, before reading a
     * word — it failed on the empty name and spent a code with a two-minute life on
     * a message. The camera now hands its code down to the field and stops there,
     * which leaves the name as the only thing still missing.
     *
     * The viewfinder going is the other half: a camera left running over a code it
     * has already read would keep reporting it, and there is nothing left to point
     * it at.
     */
    @Test
    fun a_scanned_code_lands_in_the_field_and_the_viewfinder_stands_down() {
        val scanned = "ABCD2345EFGH"
        showPairing(PairingState(code = scanned, scanned = true))

        compose.onNodeWithText(scanned).assertIsDisplayed()
        compose.onNodeWithTag(TAG_CODE_SCANNED).assertIsDisplayed()
        compose.onNodeWithTag(TAG_VIEWFINDER).assertDoesNotExist()
        Evidence.log("scanned code  = $scanned in the field, viewfinder stood down")
    }

    /** Before that, the viewfinder is what occupies the slot. */
    @Test
    fun an_unscanned_flow_shows_the_viewfinder() {
        showPairing(PairingState())

        compose.onNodeWithTag(TAG_VIEWFINDER).performScrollTo().assertIsDisplayed()
        compose.onNodeWithTag(TAG_CODE_SCANNED).assertDoesNotExist()
    }

    /**
     * The name is the person's, and pairing waits for it — and for a code.
     *
     * The desktop's flow hard-codes a default name. This one starts empty, and a
     * code on its own does not make the button pressable: the name is what the
     * computer lists beside every Entry that comes from this phone, and it is the
     * half a scan cannot supply.
     */
    @Test
    fun pairing_waits_for_both_the_name_and_the_code() {
        val state = mutableStateOf(PairingState(camera = CameraProblem.NoCamera))
        compose.setContent {
            SharepasteTheme {
                PairingScreen(
                    state = state.value,
                    onLabelChange = { state.value = state.value.copy(deviceLabel = it) },
                    onCodeChange = { state.value = state.value.copy(code = it) },
                    onPair = {},
                    onDismissFailure = {},
                )
            }
        }
        compose.onNodeWithTag(TAG_PAIR_BUTTON).assertIsNotEnabled()

        compose.onNodeWithTag(TAG_CODE_FIELD).performTextInput("SOMECODE")
        compose.onNodeWithTag(TAG_PAIR_BUTTON).assertIsNotEnabled()

        compose.onNodeWithTag(TAG_LABEL_FIELD).performTextInput("named at last")
        compose.onNodeWithTag(TAG_PAIR_BUTTON).assertIsEnabled()
        Evidence.log("pair gate     = a code alone is not enough; the name and the code both are")
    }

    private fun showPairing(state: PairingState) {
        compose.setContent {
            SharepasteTheme {
                PairingScreen(
                    state = state,
                    onLabelChange = {},
                    onCodeChange = {},
                    onPair = {},
                    onDismissFailure = {},
                )
            }
        }
    }

    /** The two camera problems are told apart before either message is chosen. */
    @Test
    fun the_camera_problems_are_told_apart_at_the_source() {
        assertEquals(null, cameraProblem(hasCamera = true, permissionGranted = true))
        assertEquals(
            CameraProblem.PermissionRefused,
            cameraProblem(hasCamera = true, permissionGranted = false),
        )
        assertEquals(
            CameraProblem.NoCamera,
            cameraProblem(hasCamera = false, permissionGranted = false),
        )
        // A device with no camera also has no granted permission, and the useful
        // thing to say is the one the person can act on.
        assertEquals(
            "a device with no camera must not be told to grant a permission",
            CameraProblem.NoCamera,
            cameraProblem(hasCamera = false, permissionGranted = true),
        )
    }
}
