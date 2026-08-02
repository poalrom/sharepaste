package com.sharepaste.android

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.ui.HistoryScreen
import com.sharepaste.android.ui.SessionPhase
import com.sharepaste.android.ui.SharepasteTheme
import com.sharepaste.android.ui.TAG_FAULT
import com.sharepaste.android.ui.TAG_OFFER
import com.sharepaste.android.ui.TAG_RECALL_FIRST
import com.sharepaste.android.ui.TAG_STANDING_ACTIONS_BLOCKED
import com.sharepaste.android.ui.TAG_STANDING_ACTIONS_ENABLE
import com.sharepaste.android.ui.UiState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * A denied notification permission leaves the app usable, and says why the
 * notification is missing.
 *
 * `POST_NOTIFICATIONS` is a runtime grant from API 33 and implicit below it, so
 * this path has two variants across the supported floor regardless — and on
 * either one a person may simply switch notifications off in Settings. Both
 * arrive here as the same fact and the same sentence.
 *
 * The screen half is what a compose test can hold, and it is the half that
 * decides whether a denial has broken the app: the two verbs are still on
 * screen, still pressable, and the app has said out loud what was lost. That the
 * *platform* really does report a denial, and that the in-app verbs really do
 * still reach the Relay with the permission revoked, is exercised on both AVDs
 * by the host-driven sequence in this ticket's issue file — `pm revoke` cannot
 * be done from inside the process it applies to.
 */
@RunWith(AndroidJUnit4::class)
class StandingActionsBlockedTest {

    @get:Rule
    val compose = createComposeRule()

    private val resources = InstrumentationRegistry.getInstrumentation().targetContext.resources

    private val blocked = UiState(
        session = SessionPhase.OutOfContact("u"),
        activeUserId = "u",
        standingActionsBlocked = true,
        // A row for `RECALL FIRST` to be about: the verb takes the first
        // displayed one, so with an empty History it is correctly disabled and
        // would prove nothing about a blocked notification.
        entries = listOf(entry(id = 1, preview = "ssh me@box")),
    )

    /**
     * The note is there, it explains, and it is not a fault.
     *
     * `TAG_FAULT` absent is the whole tone rule of this app, held by one tag
     * that exactly one branch of exactly two composables attaches. A refused
     * permission is a choice a person is entitled to make, not something broken,
     * and it must not render like the one thing on this screen they do have to
     * act on — a revoked Pairing.
     */
    @Test
    fun a_blocked_notification_is_explained_and_is_not_a_fault() {
        var offers = 0
        val recalled = mutableListOf<Long>()
        compose.setContent {
            SharepasteTheme {
                HistoryScreen(
                    blocked,
                    noActions(offerClipboard = { offers++ }, recall = { recalled += it.id }),
                )
            }
        }

        compose.onNodeWithTag(TAG_STANDING_ACTIONS_BLOCKED).assertIsDisplayed()
        val sentence = resources.getString(R.string.standing_actions_blocked)
        compose.onNodeWithText(sentence).assertIsDisplayed()
        compose.onNodeWithTag(TAG_FAULT).assertDoesNotExist()

        // The point of the sentence: nothing else is affected. Both verbs are on
        // screen and both still fire.
        compose.onNodeWithTag(TAG_OFFER).performClick()
        compose.onNodeWithTag(TAG_RECALL_FIRST).performClick()
        assertEquals("the in-app Offer must still work with the notification denied", 1, offers)
        assertEquals("the in-app Recall must still work with the notification denied", listOf(1L), recalled)
        Evidence.log("denied        = on screen: ${sentence.take(80)}…; both verbs still fire")
    }

    /** One control, and it says what pressing it is for. */
    @Test
    fun the_note_offers_a_way_to_turn_the_notification_back_on() {
        var asked = 0
        compose.setContent {
            SharepasteTheme { HistoryScreen(blocked, noActions(enableStandingActions = { asked++ })) }
        }
        compose.onNodeWithTag(TAG_STANDING_ACTIONS_ENABLE).performClick()
        assertEquals("the control must ask the platform for the notification back", 1, asked)
    }

    /**
     * A phone whose notifications work says nothing about them.
     *
     * The other half of a note that is attached by exactly one branch. Without
     * this the tag could be unconditional and the first test would still pass.
     */
    @Test
    fun a_phone_with_a_working_notification_says_nothing_about_it() {
        compose.setContent {
            SharepasteTheme {
                HistoryScreen(blocked.copy(standingActionsBlocked = false), noActions())
            }
        }
        compose.onNodeWithTag(TAG_STANDING_ACTIONS_BLOCKED).assertDoesNotExist()
    }
}
