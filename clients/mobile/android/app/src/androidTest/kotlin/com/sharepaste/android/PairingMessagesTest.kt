package com.sharepaste.android

import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
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
import com.sharepaste.android.ui.TAG_CAMERA_REFUSED
import com.sharepaste.android.ui.TAG_CODE_FIELD
import com.sharepaste.android.ui.TAG_FAILURE
import com.sharepaste.android.ui.TAG_FAILURE_DETAIL
import com.sharepaste.android.ui.TAG_LABEL_FIELD
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
 */
@RunWith(AndroidJUnit4::class)
class PairingMessagesTest {

    @get:Rule
    val compose = createComposeRule()

    private val resources = InstrumentationRegistry.getInstrumentation().targetContext.resources

    @Test
    fun a_refused_camera_permission_says_the_permission_is_off_and_offers_the_other_way_in() {
        showPairing(PairingState(camera = CameraProblem.PermissionRefused))

        compose.onNodeWithTag(TAG_CAMERA_REFUSED).assertIsDisplayed()
        val message = resources.getString(R.string.camera_permission_refused)
        compose.onNodeWithText(message, substring = true).assertIsDisplayed()

        // The fallback has to be present *on the same screen*, not behind a
        // retry: with the camera off it is the only way in.
        compose.onNodeWithTag(TAG_CODE_FIELD).assertIsDisplayed()
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
     * The name is the person's, and pairing waits for it.
     *
     * The desktop's flow hard-codes a default. This one starts empty, and typing
     * a code before a name gets a message asking for the name rather than a
     * Pairing labelled with somebody else's guess.
     */
    @Test
    fun pairing_is_blocked_until_the_person_names_the_phone() {
        var codes = mutableListOf<String>()
        compose.setContent {
            SharepasteTheme {
                PairingScreen(
                    state = PairingState(camera = CameraProblem.NoCamera),
                    onLabelChange = {},
                    onCode = { codes += it },
                    onDismissFailure = {},
                )
            }
        }
        // No name yet: the field is empty by default and `canPair` is false, so
        // the button cannot be pressed at all.
        compose.onNodeWithTag(TAG_CODE_FIELD).performTextInput("SOMECODE")
        compose.onNodeWithTag(com.sharepaste.android.ui.TAG_PAIR_BUTTON)
            .assertIsDisplayed()
            .assertIsNotEnabled()
        Evidence.log("label gate    = the pair button is disabled while the name is empty")
    }

    private fun showPairing(state: PairingState) {
        compose.setContent {
            SharepasteTheme {
                PairingScreen(
                    state = state,
                    onLabelChange = {},
                    onCode = {},
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
