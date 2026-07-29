package com.sharepaste.android

import android.app.Notification
import android.app.NotificationManager
import android.content.Context
import android.os.Bundle

/**
 * Everything this app's posted notifications actually say.
 *
 * Walks rather than reads. A notification's text lives in `extras`, styles nest
 * their own `Bundle` inside it, and actions carry titles of their own — so
 * reading the four fields somebody thought of proves nothing about the field
 * they add next year. Two criteria are asserted against this and they pull in
 * opposite directions: nothing here may be an Entry's text, and everything here
 * must be one of the fixed strings this app is allowed to say.
 *
 * `Bundle.toString()` is not a substitute: it prints
 * `Bundle[mParcelledData.dataSize=…]` for a bundle that has not been unparcelled,
 * so an assertion over it would pass on a notification that says anything at all.
 */
object PostedNotifications {

    /** Every `CharSequence` in every notification this app currently has up. */
    fun words(context: Context): Set<String> {
        val manager = context.getSystemService(NotificationManager::class.java)
        return manager.activeNotifications
            .flatMap { words(it.notification) }
            .filter { it.isNotBlank() }
            .toSet()
    }

    fun words(notification: Notification): Set<String> = buildSet {
        addAll(notification.extras.strings())
        notification.actions?.forEach { add(it.title.toString()) }
        notification.tickerText?.let { add(it.toString()) }
    }.filter { it.isNotBlank() }.toSet()

    private fun Bundle.strings(): List<String> = keySet().flatMap { key ->
        when (val value = @Suppress("DEPRECATION") get(key)) {
            is CharSequence -> listOf(value.toString())
            is Array<*> -> value.filterIsInstance<CharSequence>().map { it.toString() }
            is Bundle -> value.strings()
            else -> emptyList()
        }
    }
}
