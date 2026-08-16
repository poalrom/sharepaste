package com.sharepaste.android

import com.sharepaste.android.platform.UiPreferenceValues
import com.sharepaste.android.ui.Receipt
import com.sharepaste.android.ui.silences
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Which Receipts the two switches on the Settings Screen may silence, and which
 * they may not touch.
 *
 * The whole rule is one function, and this is the only place the rule is stated
 * twice. `silences` is reached from two places — `SharepasteViewModel.confirm` for
 * the open app and `reportReceipt` for the windowless Standing Action and share —
 * and neither may re-decide any of this locally; a JVM test is what keeps that
 * affordable, because the function needs no Context, no store and no device.
 *
 * **The two exemptions are the load-bearing half.** [Receipt.Recognised] and
 * [Receipt.Aloud] are asserted at all four switch combinations rather than at
 * the interesting one, because the failure being defended against is a switch
 * silencing something it was never given: an `ALREADY SAVED` that goes quiet
 * says *saved* by omission (ADR 0012), and an [Receipt.Aloud] that goes quiet is
 * a **Notice** — a refusal, an unpaired phone, a Recall off the cache — deleted
 * on its way to the one surface a closed phone has.
 *
 * **And the independence.** Each switch must silence its own verb and only its
 * own. The half-applied setting is the one a person cannot diagnose: they turn
 * one confirmation off, the other goes quiet too, and nothing anywhere says why.
 */
class ReceiptSilencingTest {

    private val offered = Receipt.Offered(pending = 0)
    private val recognised = Receipt.Recognised(pending = 0)
    private val recalled = Receipt.Recalled(preview = "https://example.invalid")
    private val aloud = Receipt.Aloud(R.string.notice_not_paired, R.string.action_unpaired)

    private val all = listOf(offered, recognised, recalled, aloud)

    /** Every combination of the two switches, labelled for the failure message. */
    private val positions = listOf(
        "both on" to UiPreferenceValues(showRecalled = true, confirmOffers = true),
        "recalls off" to UiPreferenceValues(showRecalled = false, confirmOffers = true),
        "offers off" to UiPreferenceValues(showRecalled = true, confirmOffers = false),
        "both off" to UiPreferenceValues(showRecalled = false, confirmOffers = false),
    )

    /**
     * A fresh install silences nothing.
     *
     * The defaults are the whole of what a person who never opened the Settings
     * Screen experiences, and both switches failing open to off would look like
     * nothing at all was wrong: the Entry still reaches the clipboard and the
     * Offer is still taken, so the only symptom is silence.
     */
    @Test
    fun the_defaults_silence_nothing() {
        val defaults = UiPreferenceValues()
        all.forEach { receipt ->
            assertFalse(
                "a fresh install silences $receipt. Both switches default on, and an app that " +
                    "says nothing about a verb it just performed reads as a verb that did not run.",
                defaults.silences(receipt),
            )
        }
    }

    /** `SHOW WHAT WAS RECALLED` silences a Recall, and only when it is off. */
    @Test
    fun the_recall_switch_silences_a_recall_in_the_off_position_only() {
        assertFalse(
            "the Recall Receipt is silenced with the switch on",
            UiPreferenceValues(showRecalled = true).silences(recalled),
        )
        assertTrue(
            "the Recall Receipt still speaks with the switch off. Off means the Entry reaches " +
                "the clipboard and Sharepaste says nothing (ADR 0009).",
            UiPreferenceValues(showRecalled = false).silences(recalled),
        )
    }

    /** `CONFIRM OFFERS` silences a taken Offer, and only when it is off. */
    @Test
    fun the_offer_switch_silences_a_taken_offer_in_the_off_position_only() {
        assertFalse(
            "the Offer Receipt is silenced with the switch on",
            UiPreferenceValues(confirmOffers = true).silences(offered),
        )
        assertTrue(
            "the Offer Receipt still speaks with the switch off",
            UiPreferenceValues(confirmOffers = false).silences(offered),
        )
    }

    /**
     * Neither switch reaches the other's verb.
     *
     * One switch per verb is the whole of the decision. A person silencing their
     * Offers has said nothing about what a Recall may say, and the reverse.
     */
    @Test
    fun each_switch_reaches_its_own_verb_and_no_other() {
        val offersOff = UiPreferenceValues(showRecalled = true, confirmOffers = false)
        assertFalse(
            "turning CONFIRM OFFERS off silenced a Recall. The switches are one per verb; a " +
                "person who silenced their Offers has said nothing about what a Recall may say.",
            offersOff.silences(recalled),
        )

        val recallsOff = UiPreferenceValues(showRecalled = false, confirmOffers = true)
        assertFalse(
            "turning SHOW WHAT WAS RECALLED off silenced an Offer",
            recallsOff.silences(offered),
        )
    }

    /**
     * A recognised Offer speaks at every switch position.
     *
     * It is not an Offer Receipt in different words. The ordinary one says the
     * content was saved; here nothing was saved, on a list the person can turn to
     * and check a second later. Going quiet leaves the claim that something was
     * saved standing unopposed, which is worse than the noise (ADR 0012).
     */
    @Test
    fun a_recognised_offer_is_never_silenced() {
        positions.forEach { (label, prefs) ->
            assertFalse(
                "ALREADY SAVED was silenced with $label. Nothing was saved, and silence says " +
                    "otherwise: it reads exactly like the ordinary Offer that did save.",
                prefs.silences(recognised),
            )
        }
    }

    /**
     * A Notice wearing a Toast speaks at every switch position.
     *
     * [Receipt.Aloud] shares the surface with a confirmation and nothing else:
     * every sentence it can carry is one that would take the band on an open
     * screen — a refusal, an unpaired phone, a Recall served off the cache. These
     * switches silence a confirmation. There is no control on the Settings Screen
     * that may take away something raised to be acted on.
     */
    @Test
    fun a_notice_said_out_loud_is_never_silenced() {
        positions.forEach { (label, prefs) ->
            assertFalse(
                "a Notice said out loud was silenced with $label. A refusal, an unpaired phone " +
                    "and a stale Recall have no band on a closed phone; this Toast is the only " +
                    "surface they have.",
                prefs.silences(aloud),
            )
        }
    }
}
