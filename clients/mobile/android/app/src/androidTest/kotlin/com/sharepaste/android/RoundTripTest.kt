package com.sharepaste.android

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.sharepaste.android.ui.Receipt
import com.sharepaste.android.ui.SessionPhase
import com.sharepaste.android.ui.TAG_OFFER
import com.sharepaste.android.ui.entryRecallTag
import com.sharepaste.core.AppException
import com.sharepaste.core.SettingsPatch
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The round trip that justifies the client at all.
 *
 * Copy on the laptop, open the phone, see it and Recall it onto the phone's
 * clipboard; Offer from the phone, see it on the laptop. Against the live Relay
 * with a second real device on the other end of the Pairing — a stub on either
 * side would prove nothing about the bytes.
 *
 * Everything is driven through the shipped objects: the app's `SharepasteRepository`,
 * its `SharepasteViewModel`, its `HistoryScreen`, and the same `appActions(model)`
 * bag `MainActivity` builds. The clipboard assertions read `ClipboardManager`
 * itself, because reaching the platform's clipboard is the whole criterion.
 *
 * Each test pairs its own phone into the run's one inviting User, so nothing here
 * depends on the order JUnit picks.
 *
 * **The in-app Recall does not fetch, and that is the point of ADR 0010.** The
 * test that used to sit here proved the opposite of `RECALL LATEST`: the phone
 * was put down, the other device put an Entry on the Relay that this phone's
 * cache had never held, and the verb handed it over. That verb is now the
 * notification's alone. `RECALL FIRST` hands over the first row of the
 * displayed list, which is a row on screen by construction, so the fetch it
 * lost cannot swap in an Entry the person never saw — and
 * `StandingActionsOnAClosedPhoneTest` still holds the round trip and the
 * fallback that may never be silent, on the surface that kept them.
 */
@RunWith(AndroidJUnit4::class)
class RoundTripTest {

    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private lateinit var phone: PhoneUnderTest
    private lateinit var other: Inviter

    @Before
    fun open() {
        other = Inviter.shared()
        phone = PhoneUnderTest.open(compose, DATABASE)
        // Paired but not yet in the foreground: several of these tests turn on
        // what a phone has and has not seen before its first `onStart`.
        phone.pairWithCode(other, "round-trip test phone")
    }

    @After
    fun close() = phone.close()

    /**
     * Something copied on the other device reaches the phone on a resume, and
     * Recalling it puts it on this phone's clipboard.
     *
     * The Entry is put on the Relay while the phone is closed, which is the shape
     * of the real thing: sync is foreground-only, so the first `onStart` is what
     * brings it in.
     */
    @Test
    fun an_entry_from_another_device_arrives_on_resume_and_recalls_onto_this_clipboard() {
        val copied = "https://example.invalid/from-the-laptop-${System.currentTimeMillis()}"
        other.offerAndWaitForUpload(copied)
        Evidence.log("other device  = offered and uploaded an Entry while this phone was closed")

        phone.enterForeground()
        phone.await("in contact after onStart") { it.session is SessionPhase.InContact }
        val arrived = phone.awaitEntry("the other device's Entry must be backfilled") {
            it.preview == copied
        }
        Evidence.log("backfilled    = id=${arrived.id} preview=${arrived.preview}")
        assertFalse("a decryptable Entry must not be marked", arrived.undecryptable)
        assertEquals(
            "this phone's own Device id has to be known, or every row claims an Origin",
            runBlocking { phone.repo.listPairings() }.single { it.userId == phone.userId }.deviceId,
            phone.state.ownDeviceId,
        )
        assertTrue(
            "this Entry came from the other device, so it has an Origin to show",
            arrived.deviceId != phone.state.ownDeviceId,
        )

        // Overwritten first, so a pass cannot be an accident of whatever the
        // clipboard already happened to hold.
        phone.clip.putText("something else entirely")
        phone.scrollTo(entryRecallTag(arrived.id))
        compose.onNodeWithTag(entryRecallTag(arrived.id)).performClick()
        val recalled = phone.awaitReceipt("the Recall must report itself") {
            it is Receipt.Recalled
        } as Receipt.Recalled
        assertEquals(
            "the Recall must say which Entry it handed over, and the Preview is how it says it",
            copied,
            recalled.preview,
        )

        val onClipboard = phone.clip.requireText("after Recalling the backfilled Entry")
        Evidence.log("recalled      = clipboard now holds ${onClipboard.length} chars")
        assertEquals("the Recall must put that Entry's plaintext on the clipboard", copied, onClipboard)
    }

    /**
     * Something Offered on the phone reaches the Relay and the other device can
     * read it.
     *
     * Read back with the other device's own `recall_latest`, which always performs
     * the round trip — so the assertion is about what the Relay holds now, not
     * about anything either device had cached.
     */
    @Test
    fun an_entry_offered_here_is_readable_by_the_other_device() {
        phone.enterForeground()
        phone.await("in contact before offering") { it.session is SessionPhase.InContact }

        val offered = "offered-from-the-phone-${System.currentTimeMillis()}"
        phone.clip.putText(offered)
        compose.onNodeWithTag(TAG_OFFER).performClick()
        val taken = phone.awaitReceipt("the Offer must be taken") { it is Receipt.Offered }
        Evidence.log("offered       = $taken")

        awaitRelayNewest("the phone's Offer must reach the Relay", offered)
        Evidence.log("other device  = read it back off the Relay: ${offered.take(24)}…")
    }

    /**
     * An Offer is honoured with capture disabled.
     *
     * `capture_enabled` governs Watched Capture, which a phone never performs.
     * Every Entry a phone produces is an Offered Capture — the person handed the
     * content over — and refusing that because of a desktop's watcher setting
     * would be indefensible.
     */
    @Test
    fun an_offer_is_honoured_with_capture_disabled() {
        // Only `captureEnabled` is patched. `autostart`, `hotkey` and
        // `updateCheckEnabled` are desktop concerns carried on the same row, and
        // a phone that named them would clear them.
        val settings = runBlocking { phone.repo.updateSettings(SettingsPatch(captureEnabled = false)) }
        assertFalse("capture must really be off for this to prove anything", settings.captureEnabled)

        phone.enterForeground()
        phone.await("in contact") { it.session is SessionPhase.InContact }

        val offered = "offered-with-capture-disabled-${System.currentTimeMillis()}"
        phone.clip.putText(offered)
        compose.onNodeWithTag(TAG_OFFER).performClick()
        val taken = phone.awaitReceipt("the Offer must be taken with capture disabled") {
            it is Receipt.Offered
        }
        Evidence.log("capture off   = $taken (capture_enabled=false)")

        awaitRelayNewest("an Offer made with capture disabled must still reach the Relay", offered)
    }

    /**
     * ADR 0011, across the two devices that make it a decision at all: a Recall
     * of a buried Entry puts it at the head of the History here **and** there.
     *
     * The other device is asked with `recall_latest`, which always performs the
     * round trip, so what it hands back is what the Relay considers the head
     * *now* rather than anything it had cached. Ordering that agrees across
     * devices is the whole point; a phone that reordered only its own list
     * would pass a weaker version of this test and ship the wrong feature.
     *
     * Nothing about the Entry itself moves — same id, same plaintext — which is
     * what "nothing is created and nothing is duplicated" means in practice.
     */
    @Test
    fun recalling_a_buried_entry_puts_it_at_the_head_on_both_devices() {
        val stamp = System.currentTimeMillis()
        val buried = "buried-entry-$stamp"
        val newer = "captured-since-$stamp"
        val buriedId = other.offerAndWaitForUpload(buried)
        other.offerAndWaitForUpload(newer)

        phone.enterForeground()
        phone.await("in contact") { it.session is SessionPhase.InContact }
        val onScreen = phone.awaitEntry("the buried Entry must be cached") { it.preview == buried }
        phone.awaitEntry("and the one captured after it") { it.preview == newer }
        assertEquals("the phone holds it under the Relay's own id", buriedId, onScreen.id)
        assertEquals(
            "precondition: the Entry captured last leads the History until something is used",
            newer,
            runBlocking { phone.repo.listHistory(phone.userId!!) }.first().preview,
        )
        Evidence.log("before recall = head is $newer, $buried is buried below it")

        phone.scrollTo(entryRecallTag(onScreen.id))
        compose.onNodeWithTag(entryRecallTag(onScreen.id)).performClick()
        phone.awaitReceipt("the Recall must report itself") { it is Receipt.Recalled }

        awaitHistoryHead("the recalled Entry must lead this phone's History", buried)
        Evidence.log("after recall  = this phone's head is $buried")
        awaitRelayNewest("and the Relay's, so every other device reorders with it", buried)
        Evidence.log("other device  = the Relay hands back $buried as its head too")

        // Counted rather than sized: the run's one inviting User is shared, so
        // another test's Entries may sit in this History too. What a use must
        // never do is *add* one.
        val after = runBlocking { phone.repo.listHistory(phone.userId!!) }
        assertEquals(
            "a use creates nothing, so the recalled text appears once and not twice",
            1,
            after.count { it.preview == buried },
        )
        assertEquals("and the Entry at the head is the one that was always there", buriedId, after.first().id)
    }

    /** Poll this phone's own History until [expected] is its head. */
    private fun awaitHistoryHead(what: String, expected: String) {
        val deadline = System.nanoTime() + PhoneUnderTest.TIMEOUT_SECONDS * 1_000_000_000L
        while (System.nanoTime() < deadline) {
            val head = runBlocking { phone.repo.listHistory(phone.userId!!) }.firstOrNull()?.preview
            if (head == expected) return
            Thread.sleep(250)
        }
        throw AssertionError("$what: it never reached the head")
    }

    /** Poll the other device until the Relay's newest Entry is [expected]. */
    private fun awaitRelayNewest(what: String, expected: String) {
        val deadline = System.nanoTime() + PhoneUnderTest.TIMEOUT_SECONDS * 1_000_000_000L
        while (System.nanoTime() < deadline) {
            val newest = try {
                other.newestOnRelay()
            } catch (e: AppException) {
                null
            }
            if (newest == expected) return
            Thread.sleep(250)
        }
        throw AssertionError("$what: the other device never saw it")
    }

    private companion object {
        const val DATABASE = "round-trip-proof.db"
    }
}
