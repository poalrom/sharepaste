package com.sharepaste.android

import android.content.ClipData
import android.content.ClipDescription
import android.content.Intent
import android.net.Uri
import android.os.PersistableBundle
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.sharepaste.android.standing.Shared
import com.sharepaste.android.standing.sharedFrom
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

/**
 * What the share target will and will not accept.
 *
 * Against `sharedFrom` and real `Intent`s rather than through the activity,
 * because the judgement *is* the feature and the activity around it is four
 * lines. An exported activity can be handed any intent at all, and every guard
 * below exists because of a specific way of getting this wrong.
 *
 * Instrumented rather than a JVM test: `Intent`, `ClipData`, `ClipDescription`
 * and `PersistableBundle` are all real platform classes with real marshalling
 * behind them, and a fake one would prove the fake agrees with itself. The half
 * that needs a Relay — that an accepted share really becomes an Entry — is in
 * [StandingActionsOnAClosedPhoneTest], where the app is not running.
 */
@RunWith(AndroidJUnit4::class)
class ShareTargetTest {

    @Test
    fun plain_shared_text_is_offerable() {
        val shared = sharedFrom(send("text/plain").putExtra(Intent.EXTRA_TEXT, "https://a.link/x"))
        assertEquals(Shared.Text("https://a.link/x"), shared)
        Evidence.log("share text    = accepted: ${(shared as Shared.Text).text}")
    }

    /**
     * Content the sender marked sensitive on the clip is refused.
     *
     * The path a password manager actually uses. Refusing is the honouring: an
     * Offered Capture is encrypted onto the Relay and decrypted into every
     * paired device's History, which is not what Share was being asked for.
     */
    @Test
    fun content_marked_sensitive_on_the_clip_is_refused() {
        val intent = send("text/plain").putExtra(Intent.EXTRA_TEXT, "hunter2")
        intent.clipData = ClipData("a credential", arrayOf("text/plain"), ClipData.Item("hunter2"))
            .apply {
                description.extras = PersistableBundle().apply {
                    putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true)
                }
            }
        assertEquals(Shared.Sensitive, sharedFrom(intent))
        Evidence.log("share clip    = refused: the sender marked its ClipDescription sensitive")
    }

    /**
     * And on the intent's own extras, which is the other place it travels.
     *
     * Both are checked because both are used. Honouring one and not the other
     * would be a rule that holds for some senders, which is the same as no rule.
     */
    @Test
    fun content_marked_sensitive_on_the_intent_is_refused() {
        val intent = send("text/plain")
            .putExtra(Intent.EXTRA_TEXT, "hunter2")
            .putExtra(ClipDescription.EXTRA_IS_SENSITIVE, true)
        assertEquals(Shared.Sensitive, sharedFrom(intent))
        Evidence.log("share extra   = refused: the sender set EXTRA_IS_SENSITIVE on the Intent")
    }

    /**
     * Sensitive is decided **before** the text is looked at.
     *
     * Ordering, not politeness: a marked payload never becomes a `String` in
     * this process at all. Proven by marking a perfectly good piece of text and
     * getting the refusal rather than the text.
     */
    @Test
    fun sensitive_wins_over_text_that_would_otherwise_be_fine() {
        val intent = send("text/plain")
            .putExtra(Intent.EXTRA_TEXT, "a link that would sail through every other check")
            .putExtra(ClipDescription.EXTRA_IS_SENSITIVE, true)
        assertEquals(Shared.Sensitive, sharedFrom(intent))
    }

    /**
     * An `EXTRA_TEXT` that is not a `CharSequence` is not text.
     *
     * The mistake ticket 10 had to undo, arriving by a different route.
     * `coerceToText` answered a screenshot's URI with a perfectly good `String`,
     * which an Offer then encrypted and uploaded — so the person's other devices
     * received a path nothing could open in place of the picture they meant to
     * send. `getCharSequenceExtra` answers `null` here instead of handing back
     * something a `toString()` would flatten into exactly that.
     */
    @Test
    fun a_non_text_extra_is_not_flattened_with_toString() {
        val intent = send("text/plain")
            .putExtra(Intent.EXTRA_TEXT, Uri.parse("content://media/external/images/media/42"))
        assertEquals(Shared.NotText, sharedFrom(intent))
        Evidence.log("share uri     = refused: EXTRA_TEXT held a Uri, not a CharSequence")
    }

    /** A share of something that is not a text type, whatever it put in EXTRA_TEXT. */
    @Test
    fun a_non_text_mime_type_is_refused() {
        val intent = send("image/png").putExtra(Intent.EXTRA_TEXT, "the caption on a picture")
        assertEquals(Shared.NotText, sharedFrom(intent))
    }

    /** A text type carries the wildcard match, so richer text still counts. */
    @Test
    fun any_text_subtype_counts_as_text() {
        val intent = send("text/html").putExtra(Intent.EXTRA_TEXT, "<p>a note</p>")
        assertEquals(Shared.Text("<p>a note</p>"), sharedFrom(intent))
    }

    /** Nothing to send is not an Offer, and an empty Entry is not worth making. */
    @Test
    fun an_empty_or_absent_share_is_refused() {
        assertEquals(Shared.NotText, sharedFrom(send("text/plain")))
        assertEquals(Shared.NotText, sharedFrom(send("text/plain").putExtra(Intent.EXTRA_TEXT, "")))
        assertEquals(Shared.NotText, sharedFrom(null))
    }

    /**
     * The activity is exported, so anything on the device can start it with
     * anything at all. An intent that is not a share is not a share.
     */
    @Test
    fun an_intent_that_is_not_a_send_is_refused() {
        val intent = Intent(Intent.ACTION_VIEW)
            .setType("text/plain")
            .putExtra(Intent.EXTRA_TEXT, "started by something that is not a share sheet")
        assertEquals(Shared.NotText, sharedFrom(intent))
    }

    private fun send(mime: String) = Intent(Intent.ACTION_SEND).setType(mime)
}
