package com.sharepaste.android

import android.app.NotificationManager
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.standing.StandingActions
import com.sharepaste.android.ui.Receipt
import com.sharepaste.android.ui.SessionPhase
import com.sharepaste.android.ui.TAG_FAULT
import com.sharepaste.android.ui.TAG_OFFER
import com.sharepaste.android.ui.TAG_RECALL_LATEST
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * A denied notification permission leaves the app working.
 *
 * **This test only runs when the host has actually taken the permission away**,
 * and it skips itself otherwise — a revoke restarts the process doing the
 * revoking, so it cannot be done from in here. Two commands, one per variant
 * across the supported floor, and both are exercised:
 *
 * ```
 * adb shell pm revoke com.sharepaste.android android.permission.POST_NOTIFICATIONS  # API 33+
 * adb shell cmd appops set com.sharepaste.android POST_NOTIFICATION ignore          # below 33
 * ```
 *
 * The sub-33 form is why the app asks `areNotificationsEnabled()` rather than
 * checking the permission: below API 33 there is no permission to check, and a
 * person can still switch notifications off. One question, one answer, one
 * sentence on screen, on every version this app supports.
 *
 * What is proven is the whole of "usable rather than broken": the notification
 * really is refused rather than silently absent, the app knows, and both verbs
 * still make and retrieve real Entries through a real Relay. The *sentence* on
 * screen has its own test in [StandingActionsBlockedTest], which needs no
 * permission state at all because it renders the state directly.
 */
@RunWith(AndroidJUnit4::class)
class NotificationsDeniedTest {

    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    private val resources = context.resources

    private lateinit var phone: PhoneUnderTest

    /**
     * Skip unless the host says it has taken the permission away — and then
     * insist that it really has.
     *
     * An assumption on the *host's claim* rather than on the permission state,
     * because those are different failures and only one of them is a skip. A
     * run that was never meant to include this class should pass it by; a run
     * that meant to revoke and did not must fail loudly rather than quietly
     * report a green test that never executed. Deciding purely on
     * `StandingActions.enabled` would also make the class order-dependent: it
     * would run or not depending on whether a class that grants the permission
     * happened to sort before it.
     */
    @Before
    fun onlyWhenTheHostHasRevokedIt() {
        assumeTrue(
            "this class needs the notification permission actually revoked, which cannot be done " +
                "from inside the process it applies to. Pass -e notificationsDenied true; the " +
                "sequence is in .scratch/mobile-client/issues/12-android-standing-actions.md.",
            InstrumentationRegistry.getArguments().getString("notificationsDenied") == "true",
        )
        assertFalse(
            "the host said it had denied notifications and they are still enabled. On API 33+ " +
                "that is `pm revoke android.permission.POST_NOTIFICATIONS`; below it, " +
                "`cmd appops set com.sharepaste.android POST_NOTIFICATION ignore`.",
            StandingActions.enabled(context),
        )
    }

    @After
    fun close() {
        if (::phone.isInitialized) phone.close()
    }

    /**
     * Refused, known about, and not fatal.
     *
     * `NotificationManager.notify` reports nothing at all when the permission is
     * denied — it simply does not appear — which is exactly why the app has to
     * ask rather than assume, and why the answer has to reach the screen.
     */
    @Test
    fun a_denied_permission_posts_nothing_and_the_app_knows() {
        assertTrue("the app must not claim it posted", !StandingActions.post(context))
        val manager = context.getSystemService(NotificationManager::class.java)
        assertTrue(
            "something was posted despite the denial",
            manager.activeNotifications.none { it.id == StandingActions.NOTIFICATION_ID },
        )
        Evidence.log("denied        = post() answered false and nothing is on the shade")
    }

    /**
     * And both in-app verbs still work, against a real Relay.
     *
     * The part of "usable rather than broken" that cannot be faked by rendering
     * a state: a genuine Offered Capture and a genuine Recall Latest, on a phone
     * whose notification the platform is refusing to show. Costs the run one
     * single-use invite token, and only when the class actually runs.
     */
    @Test
    fun the_in_app_verbs_still_work_with_the_notification_denied() {
        phone = PhoneUnderTest.open(compose, DATABASE)
        phone.pairWithInvite(TestRelay.url, "a phone with notifications off")
        phone.enterForeground()
        phone.await("in contact") { it.session is SessionPhase.InContact }

        val text = "offered-with-notifications-denied-${System.currentTimeMillis()}"
        phone.clip.putText(text)
        compose.onNodeWithTag(TAG_OFFER).performClick()
        phone.awaitReceipt("the in-app Offer must still be taken") { it is Receipt.Offered }
        val entry = phone.awaitEntry("the Offer must round-trip through the Relay") {
            it.preview == text
        }
        Evidence.log("denied verbs  = Offer still works: Entry id=${entry.id}")

        phone.clip.putText("not the Entry")
        compose.onNodeWithTag(TAG_RECALL_LATEST).performClick()
        val recalled = phone.awaitReceipt("the in-app Recall must still report what it handed over") {
            it is Receipt.Recalled
        } as Receipt.Recalled
        assertEquals(
            "the Recall must say which Entry it handed over, and that is the one just offered",
            text,
            recalled.preview,
        )
        assertNull(
            "the fetch succeeded, so the outcome confirms and vanishes. A band left standing " +
                "here would be the cache fallback, which is what 'not fall back' rules out",
            phone.state.notice,
        )
        assertEquals(
            "the in-app Recall must still put the Entry on the clipboard",
            text,
            phone.clip.requireText("after an in-app Recall with notifications denied"),
        )

        // And the app says why the notification is missing, in the ordinary
        // voice. Not a fault: refusing a notification is a choice a person is
        // entitled to make.
        phone.model.onStandingActionsChecked(blocked = true)
        phone.await("the screen must admit the notification is gone") { it.standingActionsBlocked }
        compose.onNodeWithText(resources.getString(R.string.standing_actions_blocked))
            .assertIsDisplayed()
        compose.onNodeWithTag(TAG_FAULT).assertDoesNotExist()
        Evidence.log("denied verbs  = Recall still works, and the screen says why the shade is bare")
    }

    private companion object {
        const val DATABASE = "notifications-denied.db"
    }
}
