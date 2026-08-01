package com.sharepaste.android

import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isToggleable
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.ui.Confirmation
import com.sharepaste.android.ui.PairingsScreen
import com.sharepaste.android.ui.SessionPhase
import com.sharepaste.android.ui.SharepasteTheme
import com.sharepaste.android.ui.TAG_DIVERGED
import com.sharepaste.android.ui.TAG_DIVERGED_USE
import com.sharepaste.android.ui.TAG_FAULT
import com.sharepaste.android.ui.TAG_PAIRINGS_LIST
import com.sharepaste.android.ui.TAG_SETTINGS_ABSENT_NOTE
import com.sharepaste.android.ui.TAG_SETTINGS_LABEL_NOTE
import com.sharepaste.android.ui.Tone
import com.sharepaste.android.ui.UiState
import com.sharepaste.android.ui.pairActiveTag
import com.sharepaste.android.ui.pairCardTag
import com.sharepaste.android.ui.pairCipherTag
import com.sharepaste.android.ui.pairClearTag
import com.sharepaste.android.ui.pairConfirmTag
import com.sharepaste.android.ui.pairConfirmYesTag
import com.sharepaste.android.ui.pairForgetTag
import com.sharepaste.android.ui.pairPendingTag
import com.sharepaste.android.ui.pairStatusTag
import com.sharepaste.android.ui.toneOf
import com.sharepaste.core.ConnectionState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * What the Pairings screen says, with no facade behind it.
 *
 * The wording and the tone are the whole subject here, so the state is handed in
 * rather than arrived at: a card is a function of one [com.sharepaste.core.PairingSummary],
 * and the interesting cases — a Pairing that is resting, a queue on a Pairing this
 * phone has switched away from, a Viewed Pairing that is not the Active one — are
 * states a live Relay would take several minutes to be talked into.
 * [TwoPairingsTest] and [PendingOnANonActivePairingTest] prove the same rules
 * against a real one; this proves the screen renders them.
 */
@RunWith(AndroidJUnit4::class)
class PairingsScreenTest {

    @get:Rule
    val compose = createComposeRule()

    private val resources = InstrumentationRegistry.getInstrumentation().targetContext.resources

    private val syncing = pairing(
        userId = "aaaa-1111",
        username = "the laptop's account",
        label = "Pixel in my pocket",
        status = ConnectionState.ONLINE,
        isActive = true,
    )

    /**
     * The one that matters most: not the Active Pairing and not connected.
     *
     * That combination is the ordinary state of a second Pairing on a phone, and
     * it must not look like anything is wrong with it.
     */
    private val resting = pairing(
        userId = "bbbb-2222",
        username = "work",
        label = "Pixel at work",
        serverUrl = "http://relay.invalid:9000",
        status = ConnectionState.DISCONNECTED,
    )

    private fun show(
        state: UiState,
        onView: (String) -> Unit = {},
        onActivate: (String) -> Unit = {},
        onConfirm: (Confirmation?) -> Unit = {},
        onClear: (String) -> Unit = {},
        onForget: (String) -> Unit = {},
    ) {
        compose.setContent {
            SharepasteTheme {
                PairingsScreen(
                    state = state,
                    actions = noActions(
                        viewPairing = onView,
                        activatePairing = onActivate,
                        confirm = onConfirm,
                        clearHistory = onClear,
                        forgetPairing = onForget,
                    ),
                )
            }
        }
    }

    private fun scrollTo(tag: String) {
        compose.onNodeWithTag(TAG_PAIRINGS_LIST).performScrollToNode(hasTestTag(tag))
    }

    private fun both(vararg extra: com.sharepaste.core.PairingSummary) = UiState(
        pairings = listOf(syncing, resting) + extra,
        activeUserId = syncing.userId,
        foreground = true,
    )

    /**
     * A Pairing that is merely not active and disconnected is **resting, not
     * faulty**.
     *
     * Asserted as the absence of [TAG_FAULT], which is the marker ticket 09
     * introduced and which exactly one branch of the readout attaches. Asserting
     * a word would pass for a card painted in the error container as long as the
     * sentence was right; asserting the tag is the rule itself.
     */
    @Test
    fun a_pairing_that_is_not_active_and_not_connected_is_resting_not_a_fault() {
        show(both())

        scrollTo(pairCardTag(resting.userId))
        val words = resources.getString(R.string.contact_not_active)
        compose.onNodeWithTag(pairStatusTag(resting.userId)).assertTextEquals(words)
        compose.onNodeWithTag(TAG_FAULT).assertDoesNotExist()

        assertEquals(
            "a Pairing nobody asked to connect must be nominal",
            Tone.Nominal,
            toneOf(SessionPhase.NotActive(resting.userId)),
        )
        Evidence.log("resting card  = $words (no $TAG_FAULT anywhere on the screen)")
    }

    /**
     * The contrast that makes the assertion above mean something: when a Pairing
     * genuinely is broken, it looks like it.
     */
    @Test
    fun a_revoked_pairing_is_the_one_card_that_does_read_as_a_fault() {
        val revoked = pairing(userId = "cccc-3333", status = ConnectionState.AUTH_FAILED)
        show(both(revoked))

        scrollTo(pairCardTag(revoked.userId))
        compose.onNodeWithTag(TAG_FAULT).assertIsDisplayed()
        compose.onNodeWithTag(pairStatusTag(revoked.userId))
            .assertTextEquals(resources.getString(R.string.contact_refused))
    }

    /**
     * The cipher is disclosed on **every** card, and no other cipher is named.
     *
     * ADR 0002 puts the disclosure beside pairing, where the choice to trust a
     * Relay is being made, rather than in permanent chrome. The desktop's
     * equivalent test asserts the same two halves — the string on every card, and
     * `AES` nowhere — because the mock this product was drawn from carried an
     * `AES-256-GCM` badge for a product that seals with XChaCha20-Poly1305.
     */
    @Test
    fun every_card_discloses_xchacha20_poly1305_and_nothing_names_another_cipher() {
        show(both())

        listOf(syncing, resting).forEach {
            scrollTo(pairCardTag(it.userId))
            compose.onNodeWithTag(pairCipherTag(it.userId))
                .assertTextEquals("XCHACHA20-POLY1305")
        }
        compose.onAllNodes(hasText("AES", substring = true, ignoreCase = true))
            .assertCountEquals(0)
        Evidence.log("cipher        = XCHACHA20-POLY1305 on 2 of 2 cards; AES named nowhere")
    }

    /**
     * A queue on a Pairing this phone has switched away from is on screen.
     *
     * The History's own count is the Active Pairing's, so without this the
     * Entries waiting on any other Pairing are invisible — kept, never sent, and
     * never mentioned.
     */
    @Test
    fun the_pending_count_of_a_pairing_this_phone_is_not_syncing_is_shown() {
        val queued = resting.copy(pending = 2)
        show(UiState(pairings = listOf(syncing, queued), activeUserId = syncing.userId))

        scrollTo(pairCardTag(queued.userId))
        val two = resources.getQuantityString(R.plurals.pairings_pending, 2, 2)
        compose.onNodeWithTag(pairPendingTag(queued.userId)).assertTextEquals(two)
        compose.onNodeWithTag(pairPendingTag(syncing.userId)).assertDoesNotExist()
        Evidence.log("queue on a resting Pairing = $two")
    }

    /**
     * When the Viewed and the Active Pairing diverge, a band says so and offers
     * the one action that resolves it.
     *
     * Without it the list shows one Pairing while the device syncs another, and a
     * frozen History is indistinguishable from a current one.
     */
    @Test
    fun a_band_states_the_divergence_and_offers_to_make_the_viewed_one_active() {
        var activated: String? = null
        show(
            state = both().copy(viewedUserId = resting.userId),
            onActivate = { activated = it },
        )

        compose.onNodeWithTag(TAG_DIVERGED).assertIsDisplayed()
        val sentence = resources.getString(
            R.string.pairing_diverged,
            resting.username,
            syncing.username,
        )
        compose.onNodeWithText(sentence, substring = true).assertIsDisplayed()
        // Nominal, not a fault: looking at another Pairing is a thing a person
        // chose to do.
        compose.onNodeWithTag(TAG_FAULT).assertDoesNotExist()

        compose.onNodeWithTag(TAG_DIVERGED_USE).performClick()
        assertEquals(
            "the band's action must offer the Pairing being *viewed*, not any other",
            resting.userId,
            activated,
        )
        Evidence.log("diverged band = $sentence")
    }

    /** No divergence, no band. The ordinary case must stay quiet. */
    @Test
    fun there_is_no_band_when_the_viewed_pairing_is_the_active_one() {
        show(both())
        compose.onNodeWithTag(TAG_DIVERGED).assertDoesNotExist()
        compose.onNodeWithTag(pairActiveTag(syncing.userId)).assertIsDisplayed()
        compose.onNodeWithTag(pairActiveTag(resting.userId)).assertDoesNotExist()
    }

    /**
     * Clearing a History **names the Pairing it will clear, before it clears it.**
     *
     * Two halves, and the second is the one worth the trouble: pressing Clear
     * must not erase anything. It asks, and the question names the User and the
     * Relay rather than the heading — two Pairings can share a username, and this
     * cannot be undone.
     */
    @Test
    fun clearing_a_history_names_the_pairing_before_anything_is_erased() {
        val asked = mutableStateOf<Confirmation?>(null)
        var cleared: String? = null
        val state = mutableStateOf(both())
        compose.setContent {
            SharepasteTheme {
                PairingsScreen(
                    state = state.value.copy(confirming = asked.value),
                    actions = noActions(
                        confirm = { asked.value = it },
                        clearHistory = { cleared = it },
                    ),
                )
            }
        }

        scrollTo(pairClearTag(resting.userId))
        compose.onNodeWithTag(pairClearTag(resting.userId)).performClick()
        compose.waitForIdle()

        assertNull("pressing Clear must ask, not erase", cleared)
        assertEquals(Confirmation.ClearHistory(resting.userId), asked.value)

        val question = resources.getString(
            R.string.pairings_clear_confirm,
            "${resting.username} @ relay.invalid:9000",
        )
        compose.onNodeWithTag(pairConfirmTag(resting.userId)).assertIsDisplayed()
        compose.onNodeWithText(question, substring = true).assertIsDisplayed()
        Evidence.log("clear asks    = $question")

        compose.onNodeWithTag(pairConfirmYesTag(resting.userId)).performClick()
        compose.waitForIdle()
        assertEquals(
            "and only then does it clear, and only the Pairing it named",
            resting.userId,
            cleared,
        )
    }

    /** Forgetting asks the same way, and says what leaves this phone. */
    @Test
    fun forgetting_a_pairing_names_it_and_says_what_it_erases() {
        var forgotten: String? = null
        show(
            state = both().copy(confirming = Confirmation.Forget(resting.userId)),
            onForget = { forgotten = it },
        )

        scrollTo(pairConfirmTag(resting.userId))
        val question = resources.getString(
            R.string.pairings_forget_confirm,
            "${resting.username} @ relay.invalid:9000",
        )
        compose.onNodeWithText(question, substring = true).assertIsDisplayed()
        assertTrue(
            "the question has to say the key and the token go, not merely the row",
            question.contains("key") && question.contains("token"),
        )

        compose.onNodeWithTag(pairConfirmYesTag(resting.userId)).performClick()
        compose.waitForIdle()
        assertEquals(resting.userId, forgotten)
        Evidence.log("forget asks   = $question")
    }

    /**
     * The settings a phone does not have are **stated**, not left as a gap.
     *
     * Someone who knows the desktop comes looking for the capture switch and the
     * deny-list. Finding nothing is indistinguishable from finding a half-built
     * screen, so the screen says why both are inert here — and says the same
     * about the Device Label, which the Relay owns and has no route to rename.
     */
    @Test
    fun the_settings_say_why_the_computers_two_switches_are_not_here() {
        show(both())

        scrollTo(TAG_SETTINGS_ABSENT_NOTE)
        val absent = resources.getString(R.string.settings_absent_note)
        val label = resources.getString(R.string.settings_label_note)
        compose.onNodeWithTag(TAG_SETTINGS_ABSENT_NOTE).assertTextEquals(absent)
        compose.onNodeWithTag(TAG_SETTINGS_LABEL_NOTE).assertTextEquals(label)
        Evidence.log("absent switches = $absent")
        Evidence.log("device label  = $label")
    }

    /**
     * There is no plaintext-at-rest toggle and no biometric gate, and this is
     * what stops one being helpfully added.
     *
     * Neither exists in this product: the cache stores plaintext unconditionally
     * on both clients and SQLite lives in app-private storage, which Android's
     * file-based encryption covers, so a switch would either lie or do nothing.
     * The spec's mention of one is mistaken.
     *
     * Two observable assertions rather than a reading of the source: **the
     * settings surface renders no switch at all**, which is the shape either
     * control would take, and the biometric API is not even on the classpath, so
     * a gate cannot be written without a dependency change somebody has to
     * justify. `SettingsThatDoNotExistTest` on the JVM covers the vocabulary and
     * the manifest.
     */
    @Test
    fun no_plaintext_toggle_and_no_biometric_gate_were_added() {
        show(both())
        scrollTo(TAG_SETTINGS_ABSENT_NOTE)
        compose.onAllNodes(isToggleable()).assertCountEquals(0)

        listOf(
            "androidx.biometric.BiometricPrompt",
            "androidx.biometric.BiometricManager",
        ).forEach { name ->
            try {
                Class.forName(name)
                throw AssertionError(
                    "$name is on the classpath. There is no biometric gate in this release — " +
                        "see the Android contract's leakage controls and spec risk 4.",
                )
            } catch (e: ClassNotFoundException) {
                // As it should be.
            }
        }
        Evidence.log("no toggles    = 0 switches on the settings surface; no biometric API present")
    }
}
