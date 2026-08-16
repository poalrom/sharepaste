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
import kotlinx.coroutines.runBlocking
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
     * With `CONFIRM OFFERS` off, an Offer is taken and says nothing — and a
     * repeat still says `ALREADY SAVED`.
     *
     * Both halves in one test because the second is what makes the first
     * defensible. Silencing the ordinary Offer is the switch; leaving the
     * recognised one speaking is the exception the switch's own note has to state,
     * because silence there would read exactly like the Offer that did save (ADR
     * 0012, ADR 0018).
     *
     * The proof that the Offer was *taken* is the Entry, not the Receipt: the
     * switch decides whether Sharepaste speaks and never whether the verb runs,
     * and this is the only place that distinction is observable end to end.
     *
     * `receipts` keeps every Receipt rather than the latest, which is what lets
     * "the Offer said nothing" be told apart from "the Offer was overtaken".
     */
    @Test
    fun an_offer_is_silent_with_the_switch_off_and_a_repeat_still_speaks() {
        runBlocking { phone.preferences.setConfirmOffers(false) }

        val text = "offered in silence ${System.currentTimeMillis()}"
        phone.clip.putText(text)
        compose.onNodeWithTag(TAG_OFFER).performClick()

        // The verb ran: the Entry reached the Relay and came back.
        val entry = phone.awaitEntry("a silenced Offer must still be taken") { it.preview == text }
        Evidence.log("silent offer  = Entry id=${entry.id} exists with no Receipt for it")
        assertEquals(
            "a silenced Offer still drew its Receipt. Off means the Offer is taken and Sharepaste " +
                "says nothing; the Entry above proves the verb ran either way.",
            emptyList<Receipt>(),
            phone.receipts.filterIsInstance<Receipt.Offered>(),
        )

        // The exception, on the same clipboard and through the same button.
        compose.onNodeWithTag(TAG_OFFER).performClick()
        phone.awaitReceipt("a recognised Offer must speak whatever the switch says") {
            it is Receipt.Recognised
        }
        assertEquals(
            "silencing Offers silenced ALREADY SAVED as well. Nothing was saved, and going quiet " +
                "there says otherwise — it reads exactly like the Offer that did save.",
            emptyList<Receipt>(),
            phone.receipts.filterIsInstance<Receipt.Offered>(),
        )
        Evidence.log("exception     = ${resources.getString(R.string.offer_recognised)} still drawn")
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
