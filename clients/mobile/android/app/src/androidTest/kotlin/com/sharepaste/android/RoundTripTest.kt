package com.sharepaste.android

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.sharepaste.android.ui.Notice
import com.sharepaste.android.ui.SessionPhase
import com.sharepaste.android.ui.TAG_NOTICE_STALE
import com.sharepaste.android.ui.TAG_OFFER
import com.sharepaste.android.ui.TAG_RECALL_LATEST
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
        phone.await("the Recall must report itself") { it.notice == Notice.Recalled }

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
        phone.await("the Offer must be taken") { it.notice is Notice.Offered }
        Evidence.log("offered       = ${phone.state.notice}")

        awaitRelayNewest("the phone's Offer must reach the Relay", offered)
        Evidence.log("other device  = read it back off the Relay: ${offered.take(24)}…")
    }

    /**
     * **Recall Latest fetches. It does not read the cache.**
     *
     * Proven the only way it can be: the phone is put down, the other device puts
     * an Entry on the Relay that this phone's cache has *never* held, and Recall
     * Latest is asked with no session running. If it read the cache it would hand
     * over the older Entry; it hands over the new one, and reports
     * [Notice.Recalled] rather than [Notice.RecalledFromCache] because the round
     * trip succeeded.
     */
    @Test
    fun recall_latest_fetches_an_entry_this_phone_has_never_cached() {
        val older = "older-entry-${System.currentTimeMillis()}"
        other.offerAndWaitForUpload(older)
        phone.enterForeground()
        phone.await("in contact") { it.session is SessionPhase.InContact }
        phone.awaitEntry("the older Entry must be cached first") { it.preview == older }

        // Put the phone down. Nothing syncs now, which is the entire sync model.
        phone.leaveForeground()
        phone.await("resting after onStop") { it.session is SessionPhase.Resting }

        val newer = "newer-entry-the-cache-has-never-seen-${System.currentTimeMillis()}"
        other.offerAndWaitForUpload(newer)
        Thread.sleep(SETTLE_MS)
        assertTrue(
            "the newer Entry must not have reached this phone's cache; with a live session " +
                "this test would prove nothing about fetching",
            phone.state.entries.none { it.preview == newer },
        )
        assertEquals(
            "a cache read would hand over this one",
            older,
            runBlocking { phone.repo.listHistory(phone.userId!!) }.first().preview,
        )
        Evidence.log("cache head    = $older (the newer Entry is on the Relay only)")

        compose.onNodeWithTag(TAG_RECALL_LATEST).performClick()
        phone.await("Recall Latest must settle") { it.notice != null }

        assertEquals(
            "the round trip succeeded, so the answer is authoritative and must not be " +
                "reported as stale",
            Notice.Recalled,
            phone.state.notice,
        )
        compose.onNodeWithTag(TAG_NOTICE_STALE).assertDoesNotExist()
        val onClipboard = phone.clip.requireText("after Recall Latest with the Relay reachable")
        Evidence.log("recall latest = fetched; clipboard holds the newer Entry")
        assertEquals(
            "Recall Latest read the cache instead of fetching",
            newer,
            onClipboard,
        )
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
        phone.await("the Offer must be taken with capture disabled") { it.notice is Notice.Offered }
        Evidence.log("capture off   = ${phone.state.notice} (capture_enabled=false)")

        awaitRelayNewest("an Offer made with capture disabled must still reach the Relay", offered)
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

        /**
         * How long the phone is left "closed" while the other device offers.
         *
         * Long enough that "nothing arrived" means the teardown worked rather than
         * that nothing had been sent yet — the same reasoning as ticket 09's
         * background window.
         */
        const val SETTLE_MS = 3_000L
    }
}
