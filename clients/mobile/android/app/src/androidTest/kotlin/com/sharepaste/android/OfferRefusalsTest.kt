package com.sharepaste.android

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.ui.Notice
import com.sharepaste.android.ui.Receipt
import com.sharepaste.android.ui.SessionPhase
import com.sharepaste.android.ui.TAG_NOTICE
import com.sharepaste.android.ui.TAG_OFFER
import com.sharepaste.android.ui.offerRefusalMessage
import com.sharepaste.core.SkipReason
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The two refusals an Offer can really receive and the repeat copy that is no
 * longer one, each on a real facade and each with the words a person reads.
 *
 * `HistoryListTest` pins the wording against a hand-made state; this pins that
 * the *core* produces those reasons for those inputs. Both halves are needed:
 * wording asserted against a reason nothing can produce proves nothing, and a
 * reason with no words is a button that appears to do nothing.
 *
 * Every offer here goes through the clipboard, because that is the operation —
 * `offerClipboard` reads what is on the clipboard and hands it to the core's one
 * capture filter. Which means a clipboard *read*, which means window focus, which
 * is why this runs under a compose rule with a resumed activity.
 */
@RunWith(AndroidJUnit4::class)
class OfferRefusalsTest {

    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val resources = InstrumentationRegistry.getInstrumentation().targetContext.resources

    private lateinit var phone: PhoneUnderTest

    @Before
    fun open() {
        phone = PhoneUnderTest.open(compose, DATABASE)
        phone.pairWithCode(Inviter.shared(), "offer refusals test phone")
        phone.enterForeground()
        phone.await("in contact") { it.session is SessionPhase.InContact }
    }

    @After
    fun close() = phone.close()

    /**
     * The same text twice.
     *
     * Not a refusal since ADR 0012: the phone already holds that text, so the
     * second Offer is a Use of the Entry it holds and nothing is queued. It
     * belongs in this class because it is the refusal that used to be here: the
     * proof that the core answers this input at all is the same proof, moved to
     * the outcome that replaced it.
     *
     * An Offered Capture is the easiest way to send one text twice: the button
     * is right there and nothing about the clipboard has changed.
     */
    @Test
    fun offering_the_same_text_twice_is_recognised_rather_than_refused() {
        val text = "the same link twice ${System.currentTimeMillis()}"
        phone.clip.putText(text)
        compose.onNodeWithTag(TAG_OFFER).performClick()
        phone.awaitReceipt("the first Offer must be taken") { it is Receipt.Offered }

        compose.onNodeWithTag(TAG_OFFER).performClick()
        phone.awaitReceipt("the repeat Offer must be recognised") { it is Receipt.Recognised }
        assertNull(
            "a recognised Offer confirms and needs nothing back, so it raises no Notice",
            phone.state.notice,
        )
        Evidence.log("recognised    = ${resources.getString(R.string.offer_recognised)}")
    }

    /**
     * A payload over the 64 KiB cap.
     *
     * One byte over, not comfortably over: the cap is `MAX_BYTES` in the core's
     * one filter and a test that offered a megabyte would pass under any cap at
     * all.
     */
    @Test
    fun offering_an_over_size_payload_is_refused_for_its_size() {
        phone.clip.putText("a".repeat(64 * 1024 + 1))
        compose.onNodeWithTag(TAG_OFFER).performClick()
        assertRefused(SkipReason.TOO_LARGE, "over-size")
    }

    /**
     * A clipboard holding something that is not text.
     *
     * A `content://` URI under an image MIME type, which is what copying a
     * screenshot leaves behind. It matters that this is a URI and not an empty
     * clip: `coerceToText` answers a URI item with the URI *string*, so a phone
     * that trusted it would encrypt and upload a path only this phone can open.
     * The clipboard is read through the app's own `AndroidClipboard`, which asks
     * the MIME type first and therefore hands the core nothing — and the core's
     * one filter is what calls that `NonText`.
     */
    @Test
    fun offering_a_non_text_payload_is_refused_as_not_text() {
        phone.clip.putImage()
        compose.onNodeWithTag(TAG_OFFER).performClick()
        assertRefused(SkipReason.NON_TEXT, "non-text")
    }

    private fun assertRefused(expected: SkipReason, what: String) {
        phone.await("the $what Offer must be refused") { it.notice is Notice.OfferRefused }
        assertEquals(
            "the $what Offer was refused for the wrong reason",
            Notice.OfferRefused(expected),
            phone.state.notice,
        )
        val sentence = resources.getString(offerRefusalMessage(expected))
        compose.onNodeWithTag(TAG_NOTICE).assertIsDisplayed()
        compose.onNodeWithText(sentence).assertIsDisplayed()
        Evidence.log("refused $what = $sentence")
    }

    private companion object {
        const val DATABASE = "offer-refusals-proof.db"
    }
}
