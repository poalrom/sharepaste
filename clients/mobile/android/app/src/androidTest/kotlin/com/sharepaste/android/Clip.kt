package com.sharepaste.android

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.net.Uri

/**
 * The system clipboard, as a test drives and reads it.
 *
 * Through `ClipboardManager` itself and not through the app's own `Clipboard`
 * implementation, deliberately: the criterion is that a Recall reaches *the
 * platform's* clipboard and that an Offer reads what is really on it. Asserting
 * against our own wrapper would prove only that the wrapper agrees with itself.
 *
 * A **read** needs window focus — since Android 10 the clipboard is readable only
 * by the focused application or the default input method — which is why every
 * test that offers or checks a Recall runs under a compose rule with a resumed
 * activity. [requireText] says so when the read comes back empty, because that is
 * the failure that otherwise looks like a broken Recall.
 */
class Clip(private val context: Context) {

    private val manager: ClipboardManager =
        context.getSystemService(ClipboardManager::class.java)
            ?: error("no ClipboardManager on this device")

    /** Put text on the clipboard, as any app copying would. */
    fun putText(text: String) {
        manager.setPrimaryClip(ClipData.newPlainText(null, text))
    }

    /**
     * Put something that is not text on the clipboard.
     *
     * An image MIME type over a URI item, which is the shape of clip a share
     * sheet or a browser leaves behind for a picture. It matters that the item is
     * a URI rather than an empty clip, because `ClipData.Item.coerceToText`
     * answers a URI item with the URI *string* — a perfectly good `String` that a
     * naive Offer would encrypt and upload, sending the person's other devices a
     * path instead of the picture they meant.
     *
     * The URI is `https:` and not `content:` for one uninteresting reason: putting
     * a `content://` URI on the clipboard requires a grant for it, and this app
     * owns no content provider to grant one from — `setPrimaryClip` refuses with a
     * `SecurityException` before the clip exists. The scheme is irrelevant to what
     * is being proven: `coerceToText` renders both the same way, and the MIME type
     * is what the app reads.
     */
    fun putImage() {
        manager.setPrimaryClip(
            ClipData(
                ClipDescription("a screenshot", arrayOf("image/png")),
                ClipData.Item(Uri.parse("https://pictures.invalid/screenshot-424242.png")),
            ),
        )
    }

    /** Whatever is on the clipboard now, coerced to text, or null if there is none. */
    fun text(): String? = manager.primaryClip
        ?.takeIf { it.itemCount > 0 }
        ?.getItemAt(0)
        ?.coerceToText(context)
        ?.toString()

    fun requireText(what: String): String = text() ?: throw AssertionError(
        "$what: the clipboard read back nothing. If this activity has lost window focus — a " +
            "permission dialog, a locked screen — Android denies the read and it looks exactly " +
            "like a Recall that did not happen.",
    )
}
