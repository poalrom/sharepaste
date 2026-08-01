package com.sharepaste.android

import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.StateRestorationTester
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.ui.ContactReadout
import com.sharepaste.android.ui.HistoryScreen
import com.sharepaste.android.ui.SessionPhase
import com.sharepaste.android.ui.SharepasteTheme
import com.sharepaste.android.ui.TAG_FAULT
import com.sharepaste.android.ui.TAG_FOREGROUND_NOTE
import com.sharepaste.android.ui.TAG_FOREGROUND_WHY
import com.sharepaste.android.ui.TAG_HISTORY_EMPTY
import com.sharepaste.android.ui.TAG_NOMINAL
import com.sharepaste.android.ui.Tone
import com.sharepaste.android.ui.UiState
import com.sharepaste.android.ui.toneOf
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Nothing shows an error, a warning or a degraded state merely for being
 * disconnected.
 *
 * The desktop surfaces relay health only when it is degraded (ADR 0002), which is
 * right for something that is always connected. A phone is out of contact almost
 * all of the time, because sync is foreground-only (ADR 0007) — so the same rule
 * would paint a perfectly healthy phone as permanently broken, and the rule
 * inverts: **not in contact is the nominal reading.**
 *
 * `TAG_FAULT` is the whole assertion. It is attached only by the fault branch of
 * [ContactReadout] — the error container, the error colour — so its absence *is*
 * "this does not render as an error". A future refactor that reaches for
 * `colorScheme.error` on a disconnected phase has to delete that tag to make
 * these pass, which is a thing somebody would notice.
 */
@RunWith(AndroidJUnit4::class)
class ContactReadoutTest {

    @get:Rule
    val compose = createComposeRule()

    private val resources = InstrumentationRegistry.getInstrumentation().targetContext.resources

    /**
     * The phases that must be nominal, named one by one rather than derived, so
     * that adding a phase does not quietly join the list.
     */
    private val nominalPhases = listOf(
        SessionPhase.Looking,
        SessionPhase.InContact("u"),
        SessionPhase.OutOfContact("u"),
        SessionPhase.Resting("u"),
    )

    @Test
    fun every_phase_but_a_revoked_pairing_is_nominal() {
        (nominalPhases + SessionPhase.Unpaired).forEach { phase ->
            assertEquals("$phase must be nominal on a phone", Tone.Nominal, toneOf(phase))
        }
        assertEquals(
            "a revoked Pairing is the one fault a phone can be in",
            Tone.Fault,
            toneOf(SessionPhase.Refused("u", "401")),
        )
        Evidence.log("tone table    = ${nominalPhases.map { "${it::class.java.simpleName}=${toneOf(it)}" }}")
    }

    @Test
    fun a_disconnected_phone_renders_no_fault() {
        // One `setContent` and a state that moves, because a rule hosts exactly
        // one composition: calling `setContent` again is an
        // `IllegalStateException`, not a second render.
        val phase = mutableStateOf<SessionPhase>(SessionPhase.Looking)
        compose.setContent { SharepasteTheme { ContactReadout(phase.value) } }
        nominalPhases.forEach { next ->
            compose.runOnIdle { phase.value = next }
            compose.onNodeWithTag(TAG_NOMINAL).assertIsDisplayed()
            compose.onNodeWithTag(TAG_FAULT).assertDoesNotExist()
        }
        Evidence.log("no fault      = ${nominalPhases.size} phases render in the ordinary voice")
    }

    @Test
    fun not_in_contact_reads_as_a_plain_statement_of_fact() {
        compose.setContent { SharepasteTheme { ContactReadout(SessionPhase.OutOfContact("u")) } }
        val text = resources.getString(R.string.contact_offline)
        compose.onNodeWithText(text, substring = true).assertIsDisplayed()
        compose.onNodeWithTag(TAG_FAULT).assertDoesNotExist()
        Evidence.log("offline says  = $text")
    }

    @Test
    fun a_revoked_pairing_does_render_as_a_fault() {
        compose.setContent { SharepasteTheme { ContactReadout(SessionPhase.Refused("u", "401")) } }
        // The contrast that makes the rest of this test mean something: when
        // something genuinely is wrong, it looks like it.
        compose.onNodeWithTag(TAG_FAULT).assertIsDisplayed()
        compose.onNodeWithText(resources.getString(R.string.contact_refused), substring = true)
            .assertIsDisplayed()
    }

    /**
     * An empty History says why it is empty, and the surprising part is one tap
     * away from anywhere in the list.
     *
     * A phone that has just paired shows nothing, which is correct and reads as a
     * bug. The note is the difference between "this is broken" and "this is how it
     * works" — and it is now pinned chrome rather than the list's first item, so
     * what this asserts is stronger than it was: the clipped line is on screen
     * whatever the list is doing, and `WHY ▸` opens it in full without scrolling
     * anywhere.
     */
    @Test
    fun an_empty_history_explains_itself_and_the_foreground_only_rule() {
        compose.setContent {
            SharepasteTheme {
                HistoryScreen(
                    state = UiState(session = SessionPhase.OutOfContact("u")),
                    actions = noActions(),
                )
            }
        }
        compose.onNodeWithTag(TAG_HISTORY_EMPTY).assertIsDisplayed()
        compose.onNodeWithTag(TAG_FOREGROUND_NOTE).assertIsDisplayed()
        compose.onNodeWithTag(TAG_FAULT).assertDoesNotExist()

        val pinned = resources.getString(R.string.foreground_only_pinned)
        compose.onNodeWithText(pinned, substring = true).assertIsDisplayed()

        // Clipped until asked, verbatim once opened.
        val note = resources.getString(R.string.foreground_only_note)
        compose.onNodeWithText(note, substring = true).assertDoesNotExist()
        compose.onNodeWithTag(TAG_FOREGROUND_WHY).performClick()
        compose.onNodeWithText(note, substring = true).assertIsDisplayed()
        Evidence.log("empty history = no fault; pinned \"$pinned\"; WHY opens: $note")
    }

    /**
     * `▴ CLOSE` takes the band away for good; opening it does not.
     *
     * The whole 30dp band is one tap target — redesign §6, and the reason the
     * pinned line is reachable from any screenful of Entries — which is exactly
     * what makes a dismissal dangerous here. If the first tap closed it, a thumb
     * that brushed the chrome would delete the app's most important disclosure
     * without ever having shown it. So the first tap can only open, and only the
     * second, on a control reading `▴ CLOSE` with the sentence already in front
     * of the person, is taken as acknowledgement.
     *
     * The dismissal is a fact about the phone rather than about this
     * composition, so it arrives on [UiState] and the band is not composed at
     * all. [StateRestorationTester] is what proves that is where it lives: the
     * band's own open/closed flag is `rememberSaveable` and *is* restored along
     * with everything else, and a recreated composition still has to draw
     * nothing.
     *
     * **The real preference store is deliberately not written here.**
     * `UiPreferences` can set this dismissal and has no way to unset it, and its
     * DataStore is one process-wide object that keeps what it read — so a test
     * that dismissed for real would hide this band from every test that ran
     * after it, with no `@After` able to put it back. The flag is held here the
     * way the state holder holds it, and the write it stands for is the one
     * `UiPreferences.dismissForegroundNote` call behind `AppActions`.
     */
    @Test
    fun closing_the_foreground_band_takes_it_away_for_good_and_opening_it_does_not() {
        val restoration = StateRestorationTester(compose)
        val dismissed = mutableStateOf(false)
        var dismissals = 0
        restoration.setContent {
            SharepasteTheme {
                HistoryScreen(
                    state = UiState(
                        session = SessionPhase.OutOfContact("u"),
                        foregroundNoteDismissed = dismissed.value,
                    ),
                    actions = noActions(
                        dismissForegroundNote = {
                            dismissals++
                            dismissed.value = true
                        },
                    ),
                )
            }
        }

        // Exploration: the sentence arrives and the band stays.
        val note = resources.getString(R.string.foreground_only_note)
        compose.onNodeWithTag(TAG_FOREGROUND_WHY).performClick()
        compose.onNodeWithText(note, substring = true).assertIsDisplayed()
        compose.onNodeWithTag(TAG_FOREGROUND_NOTE).assertIsDisplayed()
        assertEquals("expanding the band must not dismiss it", 0, dismissals)

        // Acknowledgement: the whole band goes, sentence and all.
        compose.onNodeWithTag(TAG_FOREGROUND_WHY).performClick()
        assertEquals("`▴ CLOSE` must dismiss, and once", 1, dismissals)
        compose.onNodeWithTag(TAG_FOREGROUND_NOTE).assertDoesNotExist()
        compose.onNodeWithText(note, substring = true).assertDoesNotExist()

        // And a recreate restores the band's saved open/closed flag into a
        // composition that has no band to restore it into.
        restoration.emulateSavedInstanceStateRestore()
        compose.onNodeWithTag(TAG_FOREGROUND_NOTE).assertDoesNotExist()
        compose.onNodeWithTag(TAG_HISTORY_EMPTY).assertIsDisplayed()
        Evidence.log("foreground    = WHY opens without dismissing; CLOSE removes it, a recreate keeps it gone")
    }
}
