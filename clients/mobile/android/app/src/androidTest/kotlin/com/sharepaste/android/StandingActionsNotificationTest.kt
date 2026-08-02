package com.sharepaste.android

import android.Manifest
import android.app.Notification
import android.app.NotificationManager
import android.app.PendingIntent
import android.os.Build
import android.service.notification.StatusBarNotification
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.standing.StandingActions
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * What the Standing Actions notification is, and what it is careful not to say.
 *
 * Everything here is about the notification as an object rather than about
 * either verb: whether it is secret, whether it previews anything, and whether
 * the two intents it carries are the ones the acceptance sequence fires from the
 * host. What the verbs *do* with the app not running is
 * [StandingActionsOnAClosedPhoneTest], which needs a Relay and a force-stop and
 * cannot run from inside one instrumentation.
 */
@RunWith(AndroidJUnit4::class)
class StandingActionsNotificationTest {

    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    private val manager = context.getSystemService(NotificationManager::class.java)

    /**
     * Grant the runtime permission, then post.
     *
     * Granted here rather than with a `GrantPermissionRule`, because that rule
     * would have to name a permission that **does not exist** below API 33 —
     * and the whole point of this ticket's second AVD is that this suite runs on
     * an API level where it does not. `UiAutomation` lets the grant be
     * conditional; below 33 the permission is implicit and there is nothing to
     * ask for.
     *
     * A grant does not restart the process the way a revoke does, so this is
     * safe in the middle of a suite. The **denied** variant cannot be done from
     * in here at all — revoking restarts the very process doing the revoking —
     * and is exercised from the host on both AVDs; see the issue file.
     */
    @Before
    fun post() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            InstrumentationRegistry.getInstrumentation().uiAutomation.grantRuntimePermission(
                context.packageName,
                Manifest.permission.POST_NOTIFICATIONS,
            )
        }
        assertTrue(
            "notifications are disabled for this app, so nothing here can be asserted",
            StandingActions.post(context),
        )
    }

    /**
     * Secret, ongoing, and previewing nothing.
     *
     * The secrecy is not decoration. With plaintext at rest and no gate, one tap
     * on Recall hands the last copied secret to whoever is holding an unlocked
     * phone — that much is the person's responsibility. Keeping the exposure off
     * the **lock** screen is the part the app can own, and
     * `VISIBILITY_SECRET` is how it owns it.
     *
     * The "previews nothing" half is asserted as a whitelist rather than as
     * "does not contain today's Entry". A blacklist proves only that the one
     * string a test happened to think of is absent; this walks every
     * `CharSequence` the notification carries, extras and actions included, and
     * fails on anything that is not one of the four fixed strings this app is
     * allowed to say. A big-text style, a ticker or a sub-text added later
     * carrying a Preview is a failure here even though nobody wrote a test for
     * it. [StandingActionsOnAClosedPhoneTest] does the blacklist half too, with
     * a real Entry on a real phone.
     */
    @Test
    fun the_notification_is_secret_ongoing_and_previews_nothing() {
        val posted = ours()
        val notification = posted.notification

        assertEquals(
            "the Standing Actions notification must be VISIBILITY_SECRET so it does not appear " +
                "on a locked screen",
            Notification.VISIBILITY_SECRET,
            notification.visibility,
        )
        assertTrue(
            "it must be ongoing: it is a pair of controls that sit there, not an event",
            notification.flags and Notification.FLAG_ONGOING_EVENT != 0,
        )
        assertEquals(
            "the channel must be the low-importance one, so this never makes a sound or arrives " +
                "as a heads-up",
            NotificationManager.IMPORTANCE_LOW,
            manager.getNotificationChannel(StandingActions.CHANNEL_ID).importance,
        )
        // Not SECRET: a channel's lock-screen setting belongs to the person, so
        // the framework discards whatever an app passes and reads back
        // NO_OVERRIDE — "defer to the notification's own", which is the one
        // asserted above. Pinned because the failure mode if it ever *did* carry
        // a value would be a channel quietly overriding the notification's
        // secrecy, and nothing on screen would look wrong.
        assertEquals(
            "the channel must defer to the notification's own visibility",
            // `NotificationManager.VISIBILITY_NO_OVERRIDE` is `@hide`, so the
            // value rather than the name. -1000 is not one of the three real
            // visibilities (-1, 0, 1); it is the sentinel for "the person has
            // expressed no preference, so use the notification's".
            NO_OVERRIDE,
            manager.getNotificationChannel(StandingActions.CHANNEL_ID).lockscreenVisibility,
        )

        val allowed = listOf(
            R.string.standing_actions_title,
            R.string.standing_actions_text,
            R.string.offer_button,
            R.string.recall_latest_button,
        ).map { context.getString(it) }.toSet()
        val said = words(notification)
        assertTrue(
            "the notification says something it was not written to say: ${said - allowed}. It " +
                "must preview no Entry text anywhere — not in the title, the text, a big-text " +
                "style or a ticker.",
            (said - allowed).isEmpty(),
        )
        Evidence.log("notification  = VISIBILITY_SECRET, ongoing, IMPORTANCE_LOW, says only $said")
    }

    /**
     * The two actions are the two intents, and this is what makes `am start`
     * equivalent to a tap.
     *
     * The acceptance sequence cannot tap anything: with the app force-stopped
     * there is no instrumentation to press with, so it fires
     * `StandingActions.intentFor(...)` from the host. That is only a fair
     * substitute if the notification's own `PendingIntent` *is* the one built
     * from that intent — and `PendingIntent` equality is exactly that question,
     * because two requests with the same package, request code and `Intent`
     * (action, data, type, component, categories) return one and the same
     * object. `FLAG_NO_CREATE` asks the question without minting anything: a
     * non-null answer means the notification is already holding it.
     */
    @Test
    fun the_two_actions_are_exactly_the_intents_the_host_fires() {
        val actions = ours().notification.actions
        assertEquals("the notification carries an Offer and a Recall and nothing else", 2, actions.size)
        assertEquals(context.getString(R.string.offer_button), actions[0].title)
        assertEquals(context.getString(R.string.recall_latest_button), actions[1].title)

        listOf(
            StandingActions.ACTION_OFFER to StandingActions.REQUEST_OFFER,
            StandingActions.ACTION_RECALL_LATEST to StandingActions.REQUEST_RECALL,
        ).forEachIndexed { index, (action, requestCode) ->
            val fromTheHostsIntent = PendingIntent.getActivity(
                context,
                requestCode,
                StandingActions.intentFor(context, action),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_NO_CREATE,
            )
            assertNotNull(
                "no PendingIntent exists for $action, so what the host fires with `am start` is " +
                    "not what the notification carries",
                fromTheHostsIntent,
            )
            assertEquals(
                "action $index does not carry the intent the host fires for $action",
                fromTheHostsIntent,
                actions[index].actionIntent,
            )
        }
        Evidence.log(
            "actions       = ${actions.joinToString { it.title.toString() }}; both PendingIntents " +
                "are the ones StandingActions.intentFor builds",
        )
    }

    /**
     * A tap reaches the transparent activity, and the activity says something
     * before it disappears.
     *
     * Sent through the notification's own `PendingIntent`, which is a tap in
     * every respect that matters — the same object the system would send.
     *
     * The assertion is that a **message was produced**, not which one: this
     * class runs against whatever the app's own database happens to hold, so the
     * Offer may be `Unpaired` or may be a genuine refusal, and both are correct
     * outcomes of a Standing Action that ran. What is being proven is that the
     * verb reached a window with focus and reported, rather than dying silently
     * in a receiver with an empty clipboard.
     */
    @Test
    fun a_tap_reaches_the_transparent_activity_and_it_reports() {
        Logcat.clear()
        ours().notification.actions[0].actionIntent.send()

        val line = Logcat.await(
            STANDING_ACTION_TAG,
            "the Offer action must reach the transparent activity and report",
        ) { it.contains(StandingActions.ACTION_OFFER) }

        val sayable = listOf(
            R.string.offer_queued,
            R.string.offer_recognised,
            R.string.offer_failed,
            R.string.action_unpaired,
            R.string.offer_refused_non_text,
            R.string.offer_refused_too_large,
            R.string.offer_refused_unreachable,
        ).map { context.getString(it) }
        assertTrue(
            "the Standing Action reported something that is not one of this app's sentences: $line",
            sayable.any { line.contains(it.take(REPORT_PREFIX)) },
        )
        Evidence.log("tap           = ${line.substringAfter(": ").take(120)}")
    }

    /** This app's one Standing Actions notification, as the system holds it. */
    private fun ours(): StatusBarNotification = manager.activeNotifications
        .firstOrNull { it.id == StandingActions.NOTIFICATION_ID }
        ?: throw AssertionError(
            "the Standing Actions notification is not posted. Active: " +
                manager.activeNotifications.joinToString { "${it.id}" },
        )

    private fun words(notification: Notification) = PostedNotifications.words(notification)

    private companion object {
        /**
         * How much of a sentence has to match.
         *
         * The log line is one line and the app's sentences wrap over several in
         * `strings.xml`, where the indentation between them becomes a run of
         * spaces the resource loader keeps. Matching the opening clause is
         * enough to tell seven distinct messages apart and does not turn this
         * into a test of whitespace.
         */
        const val REPORT_PREFIX = 30

        /**
         * The tag both Standing Action activities log under.
         *
         * Repeated here rather than reached for. `StandingActions.TAG` is
         * `internal` and this test could import it, but
         * `adb logcat -s SharepasteStandingAction` is the documented diagnostic,
         * so the string itself is the contract — a test that pinned it by
         * importing the constant could not notice it changing.
         */
        const val STANDING_ACTION_TAG = "SharepasteStandingAction"

        /** `NotificationManager.VISIBILITY_NO_OVERRIDE`, which is `@hide`. */
        const val NO_OVERRIDE = -1000
    }
}
