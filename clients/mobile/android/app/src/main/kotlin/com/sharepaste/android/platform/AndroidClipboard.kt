package com.sharepaste.android.platform

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.os.PersistableBundle
import com.sharepaste.core.AppException
import com.sharepaste.core.Clipboard

/**
 * The system clipboard.
 *
 * [readText] returning `null` is ordinary, not an error, and it means two
 * different ordinary things. Since Android 10 the clipboard is readable only by
 * the focused application or the default input method — the platform rule ADR
 * 0007 is built on, and the reason a Standing Action has to launch a transparent
 * activity rather than run in a broadcast receiver. It also means the clipboard
 * is holding something that is not text at all, which is what an Offered Capture
 * has to be able to refuse.
 *
 * [writeText] is the **raw** write. The self-write marker that stops a watcher
 * re-capturing our own clipboard write is the facade's job and is not
 * reimplemented here; a shell that tries gets the ordering wrong and a Recall
 * becomes a Capture of itself.
 */
class AndroidClipboard(context: Context) : Clipboard {

    private val appContext = context.applicationContext

    private val manager: ClipboardManager
        get() = appContext.getSystemService(ClipboardManager::class.java)
            ?: throw AppException.Storage("no ClipboardManager on this device")

    override fun readText(): String? {
        val clip = manager.primaryClip ?: return null
        // The declared MIME type decides whether there is text here, not
        // `coerceToText`. Handed a screenshot, `coerceToText` answers its
        // `content://` URI — a perfectly good String that an Offered Capture
        // would then encrypt and upload, so the person's other devices would
        // receive a URI only this phone can open in place of the image they
        // meant to send. `hasMimeType` is the platform's own answer to "is this
        // text", wildcard matching included, and it is what makes a non-text
        // clipboard reach the core's one filter as `SkipReason.NonText`.
        if (!clip.description.hasMimeType("text/*")) return null
        if (clip.itemCount == 0) return null
        return clip.getItemAt(0).coerceToText(appContext)?.toString()?.takeIf { it.isNotEmpty() }
    }

    override fun writeText(text: String) {
        val clip = ClipData.newPlainText(null, text)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            // Suppresses the system's clipboard preview toast, which would
            // otherwise put the first line of whatever was recalled on a locked
            // device's screen.
            clip.description.extras = PersistableBundle().apply {
                putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true)
            }
        }
        try {
            manager.setPrimaryClip(clip)
        } catch (e: Exception) {
            throw AppException.Storage("clipboard write: ${e.message ?: e::class.java.simpleName}")
        }
    }
}
