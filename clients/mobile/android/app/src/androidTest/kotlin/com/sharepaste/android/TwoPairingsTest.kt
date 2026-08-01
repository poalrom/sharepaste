package com.sharepaste.android

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.platform.AndroidKeychain
import com.sharepaste.android.ui.Confirmation
import com.sharepaste.android.ui.Notice
import com.sharepaste.android.ui.Receipt
import com.sharepaste.android.ui.Screen
import com.sharepaste.android.ui.SessionPhase
import com.sharepaste.android.ui.TAG_BACK_TO_HISTORY
import com.sharepaste.android.ui.TAG_DIVERGED
import com.sharepaste.android.ui.TAG_FAULT
import com.sharepaste.android.ui.TAG_OFFER
import com.sharepaste.android.ui.TAG_OPEN_PAIRINGS
import com.sharepaste.android.ui.pairActiveTag
import com.sharepaste.android.ui.pairClearTag
import com.sharepaste.android.ui.pairConfirmYesTag
import com.sharepaste.android.ui.pairForgetTag
import com.sharepaste.android.ui.pairViewTag
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * A phone holding two Pairings at once, against the live Relay.
 *
 * Two Pairings means two **Users** — a Pairing is "the local record binding this
 * machine to one user on one relay" (CONTEXT.md), so pairing twice against the
 * same inviting device would give one Pairing two Devices and prove nothing.
 * [Inviter.second] is the second User, claimed once for the whole run.
 *
 * The distinctions this exercises are the desktop's, and they are not the same
 * distinction twice:
 *
 *  * the **Active** Pairing is what the phone syncs and captures to, and the
 *    choice is persistent;
 *  * the **Viewed** Pairing is what is on screen, changes nothing, and is
 *    forgotten when the app is put down.
 */
@RunWith(AndroidJUnit4::class)
class TwoPairingsTest {

    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    private lateinit var phone: PhoneUnderTest
    private lateinit var synced: String
    private lateinit var held: String

    @Before
    fun open() {
        phone = PhoneUnderTest.open(compose, DATABASE)
        synced = phone.pairWithCode(Inviter.shared(), "the Pairing this phone syncs")
        held = phone.pairWithCode(Inviter.second(), "the Pairing it merely holds")
        // Pairing makes the newest one Active, so the one this suite calls the
        // Active Pairing has to be chosen back.
        phone.makeActive(synced)
        Evidence.log("two pairings  = active=$synced held=$held")
    }

    @After
    fun close() = phone.close()

    /**
     * The Viewed Pairing switches on its own, and **nothing about syncing or
     * capture moves with it.**
     *
     * Three separate claims, each asserted against something a mistake could not
     * fake: the core's own `activePairing()` still names the Active Pairing; the
     * live session is still the Active Pairing's; and an Offer made while looking
     * at the other Pairing lands in the Active Pairing's History and nowhere else.
     */
    @Test
    fun the_viewed_pairing_switches_without_changing_what_is_synced_or_captured() {
        // Both Entries go onto the Relay **before** this phone is opened, so the
        // backfill is what has to find them. An Entry offered after the session
        // reports `InContact` would depend on the SSE stream being subscribed
        // already, and the session reports Online a beat before it subscribes —
        // the same window `Uploader::cache_own_entry` was written for, which only
        // covers a device's own content. A phone that is closed has no window.
        val delivered = "reaches-the-active-pairing-${System.currentTimeMillis()}"
        Inviter.shared().offerAndWaitForUpload(delivered)
        val unheard = "never-reaches-the-pairing-nobody-syncs-${System.currentTimeMillis()}"
        Inviter.second().offerAndWaitForUpload(unheard)

        phone.enterForeground()
        phone.await("in contact on the Active Pairing") {
            it.session == SessionPhase.InContact(synced)
        }
        // The positive control: the Active Pairing does receive.
        phone.awaitEntry("the Active Pairing's Entry must arrive") { it.preview == delivered }

        compose.onNodeWithTag(TAG_OPEN_PAIRINGS).performClick()
        phone.await("the Pairings screen, with both Pairings on it") {
            it.screen == Screen.Pairings && it.pairings.size == 2
        }
        compose.onNodeWithTag(pairActiveTag(synced)).assertIsDisplayed()
        compose.onNodeWithTag(pairActiveTag(held)).assertDoesNotExist()
        // The Pairing this phone is not syncing is resting, not faulty.
        compose.onNodeWithTag(TAG_FAULT).assertDoesNotExist()

        phone.scrollToPairing(pairViewTag(held))
        compose.onNodeWithTag(pairViewTag(held)).performClick()
        phone.await("the Viewed Pairing must change") { it.viewedUserId == held }

        assertEquals(
            "switching what is on screen must not switch what the core syncs",
            synced,
            runBlocking { phone.repo.activePairing() },
        )
        assertEquals(synced, phone.state.activeUserId)
        assertEquals(
            "and it must not disturb the live session either",
            SessionPhase.InContact(synced),
            phone.state.session,
        )
        assertTrue(
            "the Pairing nothing syncs must hold none of the Entries put on the Relay for it",
            phone.state.entries.none { it.preview == unheard },
        )
        compose.onNodeWithTag(TAG_DIVERGED).assertIsDisplayed()
        Evidence.log("viewed switch = viewing $held, still syncing $synced; the band is on screen")

        // Capture, too, follows the Active Pairing and not the Viewed one.
        compose.onNodeWithTag(TAG_BACK_TO_HISTORY).performClick()
        phone.await("back on the History of the Viewed Pairing") { it.screen == Screen.History }
        val offered = "offered-while-looking-elsewhere-${System.currentTimeMillis()}"
        phone.clip.putText(offered)
        compose.onNodeWithTag(TAG_OFFER).performClick()
        phone.awaitReceipt("the Offer must be taken") { it is Receipt.Offered }

        // Polled, not read once. An Offer is *queued* the moment its Receipt is
        // shown; it reaches this cache only after the uploader has sent it and
        // the session's own stream has brought it back. The list on screen cannot
        // be the assertion either, because it belongs to the Pairing being viewed.
        awaitCached(synced, offered)
        val onTheViewedOne = runBlocking { phone.repo.listHistory(held) }
        assertTrue(
            "and must not go to the Pairing that merely happened to be on screen",
            onTheViewedOne.none { it.preview == offered },
        )
        Evidence.log("capture       = the Offer landed on $synced while $held was on screen")
    }

    /**
     * The Viewed Pairing is **not remembered.**
     *
     * It is a transient view choice — CONTEXT.md: "forgotten when the window
     * closes" — and a phone's equivalent of closing the window is being put down.
     * Nothing writes it anywhere, so the assertion is that picking the phone back
     * up shows the Active Pairing again without anyone asking.
     */
    @Test
    fun the_viewed_pairing_is_forgotten_when_the_app_is_put_down() {
        phone.enterForeground()
        phone.await("in contact on the Active Pairing") {
            it.session == SessionPhase.InContact(synced)
        }
        compose.onNodeWithTag(TAG_OPEN_PAIRINGS).performClick()
        phone.await("the Pairings screen") { it.screen == Screen.Pairings }
        phone.scrollToPairing(pairViewTag(held))
        compose.onNodeWithTag(pairViewTag(held)).performClick()
        phone.await("the Viewed Pairing must change") { it.viewedUserId == held }

        phone.leaveForeground()
        phone.await("resting after onStop") { it.session is SessionPhase.Resting }
        assertNull("nothing may survive the app being put down", phone.state.viewedUserId)

        phone.enterForeground()
        phone.await("in contact again") { it.session == SessionPhase.InContact(synced) }
        assertEquals(
            "the Viewed Pairing must come back as the Active one, not as the last choice",
            synced,
            phone.state.viewedPairing,
        )
        assertEquals(
            "and with nothing to diverge, there is nothing for the band to say",
            false,
            phone.state.diverged,
        )
        Evidence.log("not remembered = viewed $held, put the phone down, back on $synced")
    }

    /**
     * Forgetting a Pairing takes its Entries, its **key material** and its
     * **token**, and promotes another to Active.
     *
     * The keychain is asserted directly, before and after: a row disappearing
     * from a list is not the claim. `AndroidKeychain` is the app's real
     * keystore-backed store and `<user>:key` / `<user>:token` are the accounts
     * the core writes, so this reads exactly what the facade wrote.
     */
    @Test
    fun forgetting_a_pairing_takes_its_entries_key_and_token_and_promotes_another() {
        phone.enterForeground()
        phone.await("in contact on the Active Pairing") {
            it.session == SessionPhase.InContact(synced)
        }
        val doomed = "erased-with-its-pairing-${System.currentTimeMillis()}"
        phone.clip.putText(doomed)
        compose.onNodeWithTag(TAG_OFFER).performClick()
        phone.awaitEntry("something for the Pairing to lose") { it.preview == doomed }

        val keychain = AndroidKeychain(context)
        assertNotNull("no key to erase means this test proves nothing", keychain.get("$synced:key"))
        assertNotNull("no token to erase either", keychain.get("$synced:token"))

        compose.onNodeWithTag(TAG_OPEN_PAIRINGS).performClick()
        phone.await("the Pairings screen") { it.screen == Screen.Pairings && it.pairings.size == 2 }
        // How the app names the Pairing about to go, read while it is still on
        // the list: a notice about one that has just been erased cannot be
        // checked against a list it is no longer in. The survivor's name is read
        // *after* the forget instead — a Pairing's `username` is filled in by the
        // Relay's `GET /me` at some point during its session, so the only stable
        // comparison is against the same refreshed list the notice was built from.
        val goneName = phone.state.nameOf(synced)
        phone.scrollToPairing(pairForgetTag(synced))
        compose.onNodeWithTag(pairForgetTag(synced)).performClick()
        phone.await("Forget must ask before it erases") {
            it.confirming == Confirmation.Forget(synced)
        }
        compose.onNodeWithTag(pairConfirmYesTag(synced)).performClick()

        // The notice is part of the wait, not an assertion after it. The core's
        // own `ActivePairingChanged` re-reads the Pairings, so the list and the
        // Active id can both be settled a beat before the sentence that explains
        // them — asserting the sentence separately is a race that fails one run
        // in several.
        phone.await("the Pairing must go, the other be promoted, and it must say so") {
            it.activeUserId == held &&
                it.pairings.singleOrNull()?.userId == held &&
                it.notice is Notice.PairingForgotten
        }
        val said = phone.state.notice as Notice.PairingForgotten
        assertEquals("the notice must name the Pairing that went", goneName, said.pairing)
        assertEquals("and the one that took its place", phone.state.nameOf(held), said.promoted)

        assertNull("the key material must be gone from the keychain", keychain.get("$synced:key"))
        assertNull("and so must the token", keychain.get("$synced:token"))
        assertEquals(
            "the core must have promoted the remaining Pairing to Active",
            held,
            runBlocking { phone.repo.activePairing() },
        )
        // `list_history` is a query over Entries, not a lookup of the Pairing, so
        // a forgotten one answers with an empty History rather than `NotFound` —
        // measured, not assumed. Empty is the claim that matters: the cached
        // Entries went with the key that could read them.
        val leftBehind = runBlocking { phone.repo.listHistory(synced) }
        assertTrue(
            "the forgotten Pairing still holds Entries: $leftBehind",
            leftBehind.isEmpty(),
        )
        Evidence.log("forgotten     = $synced: key gone, token gone, 0 Entries left")
        Evidence.log("promoted      = $held, said as: ${said.pairing} -> ${said.promoted}")
    }

    /**
     * Clearing a History erases that Pairing's Entries and **leaves the other
     * Pairing's alone.**
     *
     * The naming in the confirmation is what [PairingsScreenTest] proves; this is
     * the other half of the same claim, which is that the name is not decorative
     * — the erase really is scoped to the Pairing that was named.
     */
    @Test
    fun clearing_one_pairings_history_leaves_the_other_pairings_entries_alone() {
        phone.enterForeground()
        phone.await("in contact on the Active Pairing") {
            it.session == SessionPhase.InContact(synced)
        }
        val cleared = "cleared-from-the-active-pairing-${System.currentTimeMillis()}"
        phone.clip.putText(cleared)
        compose.onNodeWithTag(TAG_OFFER).performClick()
        phone.awaitEntry("something to clear") { it.preview == cleared }

        // And an Entry on the other Pairing, brought in by making it Active for
        // as long as it takes to sync one.
        val kept = "kept-on-the-other-pairing-${System.currentTimeMillis()}"
        Inviter.second().offerAndWaitForUpload(kept)
        // Put down between switches, so the sessions do not overlap: `makeActive`
        // stops them all, and the next `onStart` brings up exactly the one the
        // core now calls Active.
        phone.leaveForeground()
        phone.makeActive(held)
        phone.enterForeground()
        phone.await("the other Pairing is the Active one now") { it.activeUserId == held }
        phone.awaitEntry("the other Pairing's Entry must be cached first") { it.preview == kept }
        phone.leaveForeground()
        phone.makeActive(synced)
        phone.enterForeground()
        phone.await("back on the Active Pairing") { it.activeUserId == synced }

        compose.onNodeWithTag(TAG_OPEN_PAIRINGS).performClick()
        phone.await("the Pairings screen") { it.screen == Screen.Pairings }
        phone.scrollToPairing(pairClearTag(synced))
        compose.onNodeWithTag(pairClearTag(synced)).performClick()
        phone.await("Clear must ask before it erases") {
            it.confirming == Confirmation.ClearHistory(synced)
        }
        compose.onNodeWithTag(pairConfirmYesTag(synced)).performClick()
        phone.await("and then say which Pairing it cleared") {
            it.notice is Notice.HistoryCleared
        }

        assertTrue(
            "the named Pairing's Entries must be gone",
            runBlocking { phone.repo.listHistory(synced) }.none { it.preview == cleared },
        )
        assertTrue(
            "and the other Pairing's must not be touched",
            runBlocking { phone.repo.listHistory(held) }.any { it.preview == kept },
        )
        Evidence.log("cleared       = $synced emptied; $held still holds its own Entry")
    }

    /**
     * Wait until one Pairing's **cache** holds an Entry with this Preview.
     *
     * The state holder cannot answer this: `UiState.entries` belongs to the
     * Viewed Pairing, and the whole point of these tests is that the Pairing
     * being written to is not the one on screen. So the cache is asked directly,
     * and asked repeatedly — an Offer is queued the moment its Receipt is shown and
     * lands here only after the uploader and the session's own stream have been
     * round the loop.
     */
    private fun awaitCached(userId: String, preview: String) {
        val deadline = System.nanoTime() + CACHE_TIMEOUT_SECONDS * 1_000_000_000L
        while (System.nanoTime() < deadline) {
            if (runBlocking { phone.repo.listHistory(userId) }.any { it.preview == preview }) return
            Thread.sleep(200)
        }
        throw AssertionError(
            "no Entry previewing \"$preview\" reached $userId's cache in ${CACHE_TIMEOUT_SECONDS}s",
        )
    }

    private companion object {
        const val DATABASE = "two-pairings-proof.db"

        /** Generous: an encrypt, a POST, and the stream bringing it back. */
        const val CACHE_TIMEOUT_SECONDS = 60L
    }
}
