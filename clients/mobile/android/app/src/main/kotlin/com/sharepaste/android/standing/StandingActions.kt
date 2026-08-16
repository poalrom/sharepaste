package com.sharepaste.android.standing

import android.app.Activity
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.util.Log
import com.sharepaste.android.MainActivity
import com.sharepaste.android.R
import com.sharepaste.android.SharepasteApplication
import com.sharepaste.android.ui.Receipt
import com.sharepaste.android.ui.receiptLogged
import com.sharepaste.android.ui.showReceipt
import com.sharepaste.android.ui.silences

/**
 * The two verbs, reachable without opening the app.
 *
 * An ongoing notification carrying an Offer and a Recall. Everything below is a
 * consequence of two platform facts and one product rule, and each of them is
 * the kind of thing a later author will try to "fix".
 *
 * ## It is deliberately not backed by a foreground service
 *
 * Three independent reasons, any one of which is sufficient:
 *
 *  1. Recent Android caps a `dataSync` foreground service at six hours in any
 *     twenty-four, after which the system stops it. A Standing Action that
 *     evaporates after six hours is worse than no Standing Action, because the
 *     person has already learned to reach for it.
 *  2. An ongoing notification is user-dismissible regardless since Android 14,
 *     so the service would not even buy the one thing it is usually reached for.
 *  3. **A foreground service does not confer clipboard access.** Only window
 *     focus does — see below. So the service would carry all of the cost of
 *     running unattended and none of the capability the feature needs.
 *
 * And the reason underneath those three: sync here is foreground-only by design
 * (ADR 0007). A clipboard tool that runs unattended is a clipboard tool that
 * reads your clipboard unattended. If you are here to add
 * `<service android:foregroundServiceType="dataSync">`, `MergedManifestTest`
 * will fail and this comment is the argument you have to answer first.
 *
 * ## Each action launches a transparent activity, not a broadcast receiver
 *
 * Since Android 10 the clipboard is readable only by the application that
 * currently has **window focus**, or the default input method. A
 * `BroadcastReceiver` fired from a notification has no window, so it reads the
 * clipboard as empty — every time, silently, and identically to "you copied
 * nothing". That is not a bug to work around; it is the platform rule ADR 0007
 * is built on. [StandingActionActivity] is invisible, waits for real window
 * focus, does the one thing and finishes.
 *
 * ## It previews no Entry text, ever
 *
 * Not in the title, the text, a big-text style or a ticker, and the notification
 * is [Notification.VISIBILITY_SECRET] so it does not appear on a locked screen at
 * all. With plaintext at rest and no biometric gate, one tap on Recall hands the
 * last copied secret to whoever is holding an unlocked phone — that much is
 * accepted as the person's responsibility rather than the app's, and no amount
 * of at-rest encryption would change it. Keeping the exposure off the lock
 * screen is the part the app *can* own, so it is not optional. `setLocalOnly`
 * is the same rule pointed sideways: nothing here is worth bridging to a watch.
 */
object StandingActions {

    /**
     * One channel, at low importance.
     *
     * Low rather than default because this notification is a pair of controls
     * that sit there, not news: it must never make a sound, vibrate, or arrive
     * as a heads-up over what someone is reading.
     */
    const val CHANNEL_ID = "standing-actions"

    /** One notification, replaced in place rather than stacked. */
    const val NOTIFICATION_ID = 1

    /** Offer whatever is on the clipboard now. */
    const val ACTION_OFFER = "com.sharepaste.android.standing.OFFER"

    /** Put the newest Entry on the clipboard. */
    const val ACTION_RECALL_LATEST = "com.sharepaste.android.standing.RECALL_LATEST"

    /**
     * The one diagnostic both Standing Action surfaces log under.
     *
     * `adb logcat -s SharepasteStandingAction` is the whole of it, and that
     * contract is only worth anything if there is exactly one tag: this used to
     * be declared privately in [StandingActionActivity] and again in
     * [ShareTargetActivity], where a rename on either side would have quietly
     * halved what that command shows. It names what happened and never what was
     * in the Entry.
     */
    internal const val TAG = "SharepasteStandingAction"

    /**
     * Whether the platform will actually show it.
     *
     * One question rather than two, and it is the right one at every API level.
     * `POST_NOTIFICATIONS` is a runtime grant from API 33 and implicit below it,
     * but a person can switch this app's notifications off in Settings on *any*
     * version — and both cases have the same consequence and deserve the same
     * sentence. Asking "are notifications enabled" covers the runtime grant, the
     * settings switch, and a channel someone blocked individually.
     */
    fun enabled(context: Context): Boolean =
        context.getSystemService(NotificationManager::class.java)?.areNotificationsEnabled() == true

    /**
     * Put the notification up, or report that the platform will not have it.
     *
     * Idempotent: the same id and the same channel, so calling it from the
     * process's own start-up, from a reboot and from `MainActivity.onStart`
     * replaces one notification rather than stacking three.
     *
     * Returns whether it went up, so a caller that has somewhere to say so can.
     * Posting with notifications disabled is not an error the platform reports —
     * `notify` simply does nothing — which is exactly the silent failure the
     * in-app note exists to turn into a sentence.
     */
    fun post(context: Context): Boolean {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return false
        if (!manager.areNotificationsEnabled()) return false
        manager.createNotificationChannel(channel(context))
        manager.notify(NOTIFICATION_ID, build(context))
        return true
    }

    private fun channel(context: Context) = NotificationChannel(
        CHANNEL_ID,
        context.getString(R.string.standing_actions_channel),
        NotificationManager.IMPORTANCE_LOW,
    ).apply {
        description = context.getString(R.string.standing_actions_channel_description)
        // **No `lockscreenVisibility` here, and that is not an omission.** The
        // framework treats a channel's lock-screen setting as the *person's*,
        // not the app's: whatever is passed at creation is discarded and the
        // channel reads back `VISIBILITY_NO_OVERRIDE`, which means "defer to the
        // notification's own". Measured on `spike35`, where setting it to
        // SECRET (-1) read back as NO_OVERRIDE (-1000). Deferring is exactly
        // what is wanted — `Notification.setVisibility(VISIBILITY_SECRET)` in
        // `build` is the enforcement point — but leaving the call in would have
        // read like a second lock that does not exist.
        setShowBadge(false)
        enableVibration(false)
        setSound(null, null)
    }

    private fun build(context: Context): Notification = Notification.Builder(context, CHANNEL_ID)
        .setSmallIcon(R.drawable.ic_standing_actions)
        // Two fixed sentences and nothing derived from an Entry. No big-text
        // style, no ticker, no sub-text: every one of those is a place a preview
        // would be easy to add and impossible to notice.
        .setContentTitle(context.getString(R.string.standing_actions_title))
        .setContentText(context.getString(R.string.standing_actions_text))
        .setVisibility(Notification.VISIBILITY_SECRET)
        .setOngoing(true)
        // There is no moment to timestamp: this is a pair of controls, not an
        // event, and "3 hours ago" beside them means nothing.
        .setShowWhen(false)
        .setOnlyAlertOnce(true)
        // Never bridged to a watch or any other companion surface.
        .setLocalOnly(true)
        .setContentIntent(openTheApp(context))
        .addAction(action(context, ACTION_OFFER, R.string.offer_button, REQUEST_OFFER))
        .addAction(action(context, ACTION_RECALL_LATEST, R.string.recall_latest_button, REQUEST_RECALL))
        .build()

    /**
     * The intent one of the two actions carries.
     *
     * `FLAG_IMMUTABLE` because nothing outside this process has any business
     * filling anything in, and `FLAG_UPDATE_CURRENT` so a re-post reuses the
     * same slot. The request codes differ because two `PendingIntent`s that
     * compare equal are one `PendingIntent`, and `Intent` equality ignores
     * extras — the action is what tells them apart, and a distinct request code
     * says so out loud rather than relying on it.
     */
    private fun action(context: Context, action: String, label: Int, requestCode: Int) =
        Notification.Action.Builder(
            null as android.graphics.drawable.Icon?,
            context.getString(label),
            PendingIntent.getActivity(
                context,
                requestCode,
                intentFor(context, action),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            ),
        ).build()

    /**
     * The intent behind an action, as a value.
     *
     * Public because it is the thing the acceptance sequence fires from the host
     * with `am start` when the app is not running, and the equivalence between
     * that and a tap is only as good as both sides using this one definition.
     * `StandingActionsNotificationTest` sends the notification's own
     * `PendingIntent` and asserts it arrives as this.
     */
    fun intentFor(context: Context, action: String): Intent =
        Intent(context, StandingActionActivity::class.java)
            .setAction(action)
            // Its own task, so a Standing Action never rearranges the stack of a
            // Sharepaste window somebody left open.
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION)

    /** Tapping the body opens the app, which is the only other thing to do with it. */
    private fun openTheApp(context: Context) = PendingIntent.getActivity(
        context,
        REQUEST_OPEN,
        Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    private const val REQUEST_OPEN = 0

    /**
     * The request codes the two actions' `PendingIntent`s were minted with.
     *
     * `internal` rather than private for one reason: two `PendingIntent`s are
     * the same object when their request code, package and `Intent` all match,
     * so a test can rebuild each action's `PendingIntent` with `FLAG_NO_CREATE`
     * and prove that the one hanging off the posted notification *is* the one
     * built from [intentFor]. That is what makes "`am start` with this intent is
     * a tap" a fact rather than a claim.
     */
    internal const val REQUEST_OFFER = 1

    /** See [REQUEST_OFFER]. */
    internal const val REQUEST_RECALL = 2
}

/**
 * Say what a verb did, from a window that has no screen to say it on.
 *
 * The one surface a Standing Action or a share has, and the one place either
 * consults the switches. It is shared for the same reason [StandingActions.TAG]
 * is: this was written out twice, once in each activity, and two copies of a rule
 * about a person's settings are two ways for those settings to end up
 * half-applied. `ShareTargetActivity` is the proof that matters — it went ungated
 * for as long as one verb had one switch, so it is exactly the site a third copy
 * would be forgotten at next time.
 *
 * The Toast goes to the **application** context, from [showReceipt], and is shown
 * *before* either caller finishes: a Toast is queued by the system rather than
 * drawn by the window that asked for it, but one asked for after `finish` has run
 * is one the system may drop.
 *
 * **The log line is never the Toast, and never silent.** It is [receiptLogged],
 * which for a Recall is the fixed sentence that names no Entry — see ADR 0009 for
 * what the Toast is allowed to say and why a durable log is not allowed to say it.
 * It is outside the branch because it is a diagnostic rather than something the
 * person is being told, and because the acceptance sequence reads it with the app
 * force-stopped. `StandingActionsNotificationTest` and that sequence both expect
 * one of this app's own fixed sentences here.
 *
 * [action] is passed in rather than read back off `getIntent`, because that
 * answers with whatever was delivered *most recently* — a second press arriving
 * while this one is still working would otherwise relabel this line as the other
 * verb.
 */
internal suspend fun Activity.reportReceipt(action: String?, receipt: Receipt) {
    val prefs = (application as SharepasteApplication).uiPreferences.snapshot()
    if (!prefs.silences(receipt)) showReceipt(this, receipt)
    Log.i(StandingActions.TAG, "$action: ${getString(receiptLogged(receipt))}")
}
