package com.sharepaste.android

import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
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
import com.sharepaste.android.ui.TAG_CONFIRM_OFFERS
import com.sharepaste.android.ui.TAG_CONFIRM_OFFERS_NOTE
import com.sharepaste.android.ui.TAG_DIVERGED
import com.sharepaste.android.ui.TAG_DIVERGED_USE
import com.sharepaste.android.ui.TAG_FAULT
import com.sharepaste.android.ui.TAG_PAIRINGS_LIST
import com.sharepaste.android.ui.TAG_SETTINGS_ABSENT_NOTE
import com.sharepaste.android.ui.TAG_SETTINGS_FOREGROUND_NOTE
import com.sharepaste.android.ui.TAG_SHOW_RECALLED
import com.sharepaste.android.ui.TAG_SHOW_RECALLED_NOTE
import com.sharepaste.android.ui.TAG_THIS_PHONE
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
 * What the Settings screen says, with no facade behind it.
 *
 * The wording and the tone are the whole subject here, so the state is handed in
 * rather than arrived at: a card is a function of one [com.sharepaste.core.PairingSummary],
 * and the interesting cases — a Pairing that is resting, a queue on a Pairing this
 * phone has switched away from, a Viewed Pairing that is not the Active one — are
 * states a live Relay would take several minutes to be talked into.
 * [TwoPairingsTest] and [PendingOnANonActivePairingTest] prove the same rules
 * against a real one; this proves the screen renders them.
 *
 * The two controls here that are not verbs — the `SHOW WHAT WAS RECALLED` and
 * `CONFIRM OFFERS` switches — are asserted the same way, as a snapshot in and a
 * call out. What they *persist* across process death belongs to a test with a
 * real DataStore behind it, not to one that hands the screen two `Boolean`s.
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
        onShowRecalled: (Boolean) -> Unit = {},
        onConfirmOffers: (Boolean) -> Unit = {},
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
                        setShowRecalled = onShowRecalled,
                        setConfirmOffers = onConfirmOffers,
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
     * The screen is called **`SETTINGS`**, and so is the way in.
     *
     * Pairings were the whole of this screen once and the title said so. It now
     * holds four sections and a Pairing is one of them, so a title naming that
     * one sends anybody looking for a switch to a screen that does not exist.
     * `Screen.Pairings` and both string keys keep their names on purpose: what
     * changed is what the screen is *called*, not where it lives, and renaming
     * the key would edit every call site to say nothing new.
     *
     * The `◎` glyph itself is on the History and is asserted there. What belongs
     * here is that the door and the room agree — the half a retitle breaks, and
     * the half that decides what a screen reader says before the screen opens.
     */
    @Test
    fun the_screen_and_the_way_into_it_are_both_called_settings() {
        show(both())

        val title = resources.getString(R.string.pairings_title)
        assertEquals("SETTINGS", title)
        assertEquals(
            "the glyph that opens this screen has to name the screen it opens",
            "Settings",
            resources.getString(R.string.pairings_open),
        )
        compose.onNodeWithText(title).assertIsDisplayed()
        Evidence.log("title band    = $title")
    }

    /**
     * A card is subtitled by **the relay address alone**.
     *
     * The `user_id` led that subtitle and was redundant twice over: the card is
     * headed by the User and states `This phone here: …` inside it, so the uuid
     * told nobody anything — and at its length it pushed the one fact the
     * subtitle exists for, which Relay this Pairing talks to, off the end of a
     * single ellipsised line.
     *
     * Asserted as the exact subtitle *and* the absence of the uuid anywhere on
     * the screen. A subtitle that merely contains the host would pass with the
     * uuid still sitting in front of it, which is the state this is fixing.
     */
    @Test
    fun a_card_is_subtitled_by_its_relay_address_alone() {
        show(both())

        listOf(syncing, resting).forEach {
            scrollTo(pairCardTag(it.userId))
            compose.onNodeWithText(it.relayHost).assertIsDisplayed()
            compose.onAllNodes(hasText(it.userId, substring = true)).assertCountEquals(0)
        }
        Evidence.log("card subtitle = ${resting.relayHost} (no user_id anywhere on screen)")
    }

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
     * `THIS PHONE` carries both of the phone's real preferences and shows which
     * way each is set.
     *
     * Both directions, because a switch wedged on is indistinguishable from a
     * working one for everybody whose preference is the default — and the default
     * here is on. The assertion is the toggleable semantics rather than a colour:
     * the row is built out of this screen's own borders, there being no Material
     * `Switch` anywhere in this app, and a hand-built control that looks right
     * while announcing nothing to a screen reader is the exact failure worth
     * pinning. `Role.Switch` is what makes it say "on" instead of describing a
     * rectangle.
     */
    @Test
    fun the_two_this_phone_switches_show_which_way_they_are_set() {
        val recalls = mutableStateOf(true)
        val offers = mutableStateOf(true)
        compose.setContent {
            SharepasteTheme {
                PairingsScreen(
                    state = both().copy(
                        showRecalled = recalls.value,
                        confirmOffers = offers.value,
                    ),
                    actions = noActions(),
                )
            }
        }

        scrollTo(TAG_THIS_PHONE)
        listOf(TAG_SHOW_RECALLED, TAG_CONFIRM_OFFERS).forEach { tag ->
            compose.onNodeWithTag(tag).assertIsOn()
            compose.onNodeWithTag(tag)
                .assert(SemanticsMatcher.expectValue(SemanticsProperties.Role, Role.Switch))
        }
        val recallNote = resources.getString(R.string.settings_show_recalled_note)
        val offerNote = resources.getString(R.string.settings_confirm_offers_note)
        compose.onNodeWithTag(TAG_SHOW_RECALLED_NOTE).assertTextEquals(recallNote)
        compose.onNodeWithTag(TAG_CONFIRM_OFFERS_NOTE).assertTextEquals(offerNote)

        // One at a time, so a screen wired to draw both rows from one field would
        // fail here rather than pass a both-off assertion by accident.
        recalls.value = false
        compose.waitForIdle()
        compose.onNodeWithTag(TAG_SHOW_RECALLED).assertIsOff()
        compose.onNodeWithTag(TAG_CONFIRM_OFFERS).assertIsOn()

        recalls.value = true
        offers.value = false
        compose.waitForIdle()
        compose.onNodeWithTag(TAG_SHOW_RECALLED).assertIsOn()
        compose.onNodeWithTag(TAG_CONFIRM_OFFERS).assertIsOff()
        Evidence.log("switches      = each row follows its own field, on and off")
        Evidence.log("recall note   = $recallNote")
        Evidence.log("offer note    = $offerNote")
    }

    /**
     * Pressing the switch **asks for the change; it does not make it.**
     *
     * The preference lives in DataStore and arrives back through `UiState`, so
     * the row is told what to draw and never holds an opinion of its own.
     * Asserted as the value handed to `setShowRecalled` while the row is still
     * showing the old one: a switch that moved itself would look right here and
     * then disagree with the phone for as long as the write took, or for good if
     * the write failed.
     */
    @Test
    fun pressing_either_switch_asks_for_the_change_rather_than_making_it() {
        var askedRecalls: Boolean? = null
        var askedOffers: Boolean? = null
        show(
            both(),
            onShowRecalled = { askedRecalls = it },
            onConfirmOffers = { askedOffers = it },
        )

        scrollTo(TAG_THIS_PHONE)
        compose.onNodeWithTag(TAG_SHOW_RECALLED).performClick()
        compose.onNodeWithTag(TAG_CONFIRM_OFFERS).performClick()
        compose.waitForIdle()

        assertEquals(
            "the Recall switch has to ask for the opposite of what it is showing",
            false,
            askedRecalls,
        )
        assertEquals(
            "the Offer switch has to ask for the opposite of what it is showing, and it must not " +
                "be wired to the Recall switch's own action",
            false,
            askedOffers,
        )
        compose.onNodeWithTag(TAG_SHOW_RECALLED).assertIsOn()
        compose.onNodeWithTag(TAG_CONFIRM_OFFERS).assertIsOn()
        Evidence.log("switch press  = each row asked for false, and neither moved itself")
    }

    /**
     * `ABOUT THIS PHONE` states two facts about the phone, and no longer states a
     * third somewhere it cannot be acted on.
     *
     * The two settings a computer has and this does not are stated rather than
     * left as a gap: finding nothing where the capture switch should be is
     * indistinguishable from finding a half-built screen. The foreground-only
     * rule joins them at full length, because the History's band says the same
     * thing and `▴ CLOSE` retires that one for good — a fact a person can dismiss
     * permanently needs one surface where they cannot.
     *
     * The Device Label note is gone from here, and the assertion for it is not:
     * it moved to `SettingsThatDoNotExistTest`, which reads `strings.xml` as
     * text. Keeping it here would have meant keeping the string itself alive
     * purely so that a test could look it up and find it undrawn. Its one
     * load-bearing fact is `pair_label_explainer` now, beside the field where a
     * name is still being chosen.
     */
    @Test
    fun about_this_phone_states_the_absent_switches_and_the_foreground_rule() {
        show(both())

        scrollTo(TAG_SETTINGS_ABSENT_NOTE)
        val absent = resources.getString(R.string.settings_absent_note)
        val foreground = resources.getString(R.string.foreground_only_note)
        compose.onNodeWithTag(TAG_SETTINGS_ABSENT_NOTE).assertTextEquals(absent)
        compose.onNodeWithTag(TAG_SETTINGS_FOREGROUND_NOTE).assertTextEquals(foreground)
        Evidence.log("absent switches = $absent")
        Evidence.log("foreground    = $foreground")
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
     * **A census, not an absence.** This screen has exactly two switches — `SHOW
     * WHAT WAS RECALLED` and `CONFIRM OFFERS`, which decide only whether
     * Sharepaste speaks after a verb it performed either way — so an assertion of
     * *no* switches anywhere would have had to be deleted the day the first one
     * arrived, and deleting the guard is how the guarded thing gets in. Naming
     * the permitted switches instead fails a third one whatever it ends up
     * called; that is the whole reason this survived ADR 0018 as an edit rather
     * than as a deletion. The count is taken at every scroll position in the list
     * rather than once, because a `LazyColumn` composes what is on screen and a
     * control parked off it would otherwise never be counted.
     *
     * The other half is the classpath: the biometric API is not on it, so a gate
     * cannot be written without a dependency change somebody has to justify.
     * `SettingsThatDoNotExistTest` on the JVM covers the vocabulary and the
     * manifest.
     */
    @Test
    fun no_plaintext_toggle_and_no_biometric_gate_were_added() {
        show(both())
        listOf(
            pairCardTag(syncing.userId),
            pairCardTag(resting.userId),
            TAG_THIS_PHONE,
            TAG_SETTINGS_ABSENT_NOTE,
        ).forEach { tag ->
            scrollTo(tag)
            compose.onAllNodes(
                isToggleable() and
                    !hasTestTag(TAG_SHOW_RECALLED) and
                    !hasTestTag(TAG_CONFIRM_OFFERS),
            ).assertCountEquals(0)
        }
        scrollTo(TAG_THIS_PHONE)
        compose.onAllNodes(isToggleable()).assertCountEquals(2)

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
        Evidence.log("switch census = 2 on this screen, both confirmation ones; no biometric API")
    }
}
