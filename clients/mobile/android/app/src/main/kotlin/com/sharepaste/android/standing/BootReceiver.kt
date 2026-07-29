package com.sharepaste.android.standing

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * Puts the Standing Actions back after a reboot, without the app being opened.
 *
 * A notification does not survive a reboot; nothing does. So the one thing this
 * client offers a person who never opens it would be gone until they did, which
 * is precisely the opposite of the point.
 *
 * **This is the only broadcast this app receives, and it schedules nothing.** It
 * is not a back door into background work: it posts a notification and returns.
 * Sync stays foreground-only (ADR 0007) — the process the boot broadcast starts
 * opens no session, contacts no Relay and reads no clipboard, because none of
 * that happens until an action is actually pressed. If you are here to add
 * `startForegroundService` or a `WorkManager` enqueue, read
 * [StandingActions]'s comment first.
 *
 * `ACTION_BOOT_COMPLETED` rather than `LOCKED_BOOT_COMPLETED` deliberately: the
 * facade lives in credential-encrypted storage and this app is not direct-boot
 * aware, so before the first unlock there is no database to act on anyway.
 */
class BootReceiver : BroadcastReceiver() {

    override fun onReceive(context: Context, intent: Intent) {
        // A receiver declared for one action can still be woken by a directed
        // intent; posting the notification is harmless either way, but checking
        // keeps the reason this class exists legible.
        if (intent.action != Intent.ACTION_BOOT_COMPLETED) return
        StandingActions.post(context)
    }
}
