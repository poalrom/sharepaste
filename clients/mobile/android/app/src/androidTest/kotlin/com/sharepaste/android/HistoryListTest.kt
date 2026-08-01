package com.sharepaste.android

import androidx.compose.runtime.collectAsState
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.ui.HistoryScreen
import com.sharepaste.android.ui.Notice
import com.sharepaste.android.ui.SessionPhase
import com.sharepaste.android.ui.SharepasteTheme
import com.sharepaste.android.ui.TAG_NOTICE
import com.sharepaste.android.ui.TAG_NOTICE_STALE
import com.sharepaste.android.ui.TAG_PENDING
import com.sharepaste.android.ui.TAG_PENDING_COUNT
import com.sharepaste.android.ui.UiState
import com.sharepaste.android.ui.entryDeleteTag
import com.sharepaste.android.ui.entryOriginTag
import com.sharepaste.android.ui.entryPreviewTag
import com.sharepaste.android.ui.entryRecallTag
import com.sharepaste.android.ui.entryRowTag
import com.sharepaste.android.ui.entryUndecryptableTag
import com.sharepaste.android.ui.offerRefusalMessage
import com.sharepaste.core.SkipReason
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The three things this list has to get right, and the words it says.
 *
 * Every one of the three is a mistake the desktop made first, which is why they
 * are pinned here rather than left to a reading of the screen:
 *
 *  * A Preview is rendered as the facade built it — one line, control characters
 *    already spaces. Without that normalisation an indented Entry renders as a
 *    visually empty row, and re-deriving it here would be a second rule to keep
 *    in step with the first.
 *  * Undecryptable comes from the explicit flag and never from an empty Preview.
 *    Two tests make the whole argument: an empty Preview with the flag *off* must
 *    not be marked, and a row with the flag *on* must be — which no amount of
 *    guessing from the Preview can do.
 *  * Origin is shown only for an Entry that came from another Device.
 *
 * No facade behind any of it. A screen takes a [UiState] and an `AppActions`, so
 * every word is assertable without a Relay, a Pairing, or a device in a
 * particular mood.
 */
@RunWith(AndroidJUnit4::class)
class HistoryListTest {

    @get:Rule
    val compose = createComposeRule()

    private val resources = InstrumentationRegistry.getInstrumentation().targetContext.resources

    /**
     * What the screen is showing.
     *
     * A flow rather than repeated `setContent` calls, which a compose rule permits
     * exactly once. It also matches how the app feeds the screen: one immutable
     * snapshot at a time out of a `StateFlow`.
     */
    private val shown = MutableStateFlow(UiState())

    /**
     * Every Entry the screen asked to have deleted, in order.
     *
     * Wired here rather than per-test because a compose rule permits one
     * `setContent`: a test that wants to press a delete has to have had the
     * lambda in place before the first frame.
     */
    private val deleted = mutableListOf<Long>()

    @Before
    fun render() {
        compose.setContent {
            SharepasteTheme {
                HistoryScreen(
                    state = shown.collectAsState().value,
                    actions = noActions(deleteEntry = { deleted += it.id }),
                )
            }
        }
    }

    private fun show(state: UiState) {
        shown.value = state
        compose.waitForIdle()
    }

    /**
     * An indented, multi-line plaintext, and the Preview the facade built from
     * it.
     *
     * Not derived here — this is what `render::preview` returns for
     * [indentedPlaintext]: every control character becomes a space, then the
     * whole thing is trimmed. Pinned in Rust by
     * `render::tests::preview_of_an_indented_entry_starts_at_its_first_word`.
     */
    private val indentedPlaintext = "\n\t  cafe on the corner\n\n  8pm\n"
    private val normalisedPreview = "cafe on the corner    8pm"

    /*
     * The blank-row bug, pinned.
     *
     * The Entry is the one the facade would hand over: `plaintext` begins with a
     * newline and a tab, `preview` is the flattened line. A row that rendered
     * `plaintext` at `maxLines = 1` would show the leading whitespace and read
     * as an empty row — which is what a person reports as "sync is broken".
     */
    @Test
    fun an_indented_entry_renders_as_one_readable_line_and_never_as_a_blank_row() {
        show(
            UiState(
                session = SessionPhase.InContact("u"),
                entries = listOf(
                    entry(id = 1, preview = normalisedPreview, plaintext = indentedPlaintext),
                ),
            ),
        )

        compose.onNodeWithTag(entryPreviewTag(1)).assertIsDisplayed()
        compose.onNodeWithTag(entryPreviewTag(1)).assertTextEquals(normalisedPreview)
        assertTrue(
            "the row must not begin with the whitespace the raw plaintext begins with",
            !normalisedPreview.first().isWhitespace(),
        )
        Evidence.log("preview       = rendered verbatim as \"$normalisedPreview\"")
        Evidence.log("plaintext     = ignored by the row; it begins \"\\n\\t  \"")
    }

    @Test
    fun an_undecryptable_entry_is_marked_cannot_be_recalled_and_can_still_be_deleted() {
        // As it arrives from the facade: no cached plaintext at all, which is
        // both an empty Preview and the flag. Only the flag is read.
        show(
            UiState(
                session = SessionPhase.InContact("u"),
                entries = listOf(entry(id = 7, preview = "", plaintext = null, undecryptable = true)),
            ),
        )

        val marker = resources.getString(R.string.entry_undecryptable)
        compose.onNodeWithTag(entryUndecryptableTag(7)).assertTextEquals(marker)
        compose.onNodeWithTag(entryRecallTag(7)).assertIsNotEnabled()
        compose.onNodeWithTag(entryDeleteTag(7)).assertIsEnabled()
        Evidence.log("undecryptable = marked \"$marker\"; Recall disabled, Delete offered")
    }

    /**
     * The regression the desktop spent ticket 06 removing.
     *
     * An Entry whose plaintext is genuinely the empty string is perfectly
     * decryptable, and anything inferring the flag from the Preview marks it
     * Undecryptable and refuses to Recall it. This is the test that fails if
     * someone reaches for `preview.isEmpty()` again.
     */
    @Test
    fun an_entry_whose_plaintext_is_genuinely_empty_is_not_marked_undecryptable() {
        show(
            UiState(
                session = SessionPhase.InContact("u"),
                entries = listOf(entry(id = 8, preview = "", plaintext = "", undecryptable = false)),
            ),
        )

        compose.onNodeWithTag(entryUndecryptableTag(8)).assertDoesNotExist()
        compose.onNodeWithTag(entryPreviewTag(8)).assertExists()
        compose.onNodeWithTag(entryRecallTag(8)).assertIsEnabled()
        Evidence.log("empty preview = not marked, and still recallable — the flag is the signal")
    }

    @Test
    fun origin_is_shown_only_for_an_entry_from_another_device() {
        show(
            UiState(
                session = SessionPhase.InContact("u"),
                ownDeviceId = "this-phone",
                entries = listOf(
                    entry(id = 2, deviceId = "the-laptop", deviceLabel = "MBP-14"),
                    entry(id = 1, deviceId = "this-phone", deviceLabel = "Pixel in my pocket"),
                ),
            ),
        )

        compose.onNodeWithTag(entryOriginTag(1)).assertDoesNotExist()
        compose.onNodeWithTag(entryOriginTag(2))
            .assertTextEquals(resources.getString(R.string.entry_origin, "MBP-14"))
        Evidence.log("origin        = shown for the laptop's Entry, absent on this phone's own")
    }

    /**
     * ADR 0001 permits a null Device Label, so a row cannot assume one.
     *
     * The rule itself lives in the core — `render::origin_label`, pinned by
     * `render::tests::origin_label_falls_back_to_a_short_device_id_slice` — so
     * what is left to assert here is that the row shows the answer the facade
     * resolved and does not reach for [Entry.deviceLabel] behind its back.
     */
    @Test
    fun an_unlabelled_device_shows_the_short_id_slice_the_facade_resolved() {
        show(
            UiState(
                session = SessionPhase.InContact("u"),
                ownDeviceId = "this-phone",
                entries = listOf(entry(id = 3, deviceId = "abcdef123456", deviceLabel = null)),
            ),
        )
        compose.onNodeWithTag(entryOriginTag(3))
            .assertTextEquals(resources.getString(R.string.entry_origin, "abcd"))
        Evidence.log("null label    = origin falls back to the id slice \"abcd\"")
    }

    /**
     * Each refusal an Offer can receive says something different.
     *
     * A rejection with no reason is a button that appears to do nothing, which is
     * the one outcome a person cannot act on. The three reachable reasons need
     * three sentences because each one needs a different thing done about it.
     */
    @Test
    fun each_offer_refusal_reads_as_its_own_sentence() {
        val reachable = listOf(SkipReason.NON_TEXT, SkipReason.TOO_LARGE, SkipReason.DUPLICATE)
        val sentences = reachable.map { resources.getString(offerRefusalMessage(it)) }
        assertEquals("each reachable refusal needs its own words", 3, sentences.toSet().size)

        reachable.forEach { reason ->
            show(
                UiState(
                    session = SessionPhase.InContact("u"),
                    notice = Notice.OfferRefused(reason),
                ),
            )
            val sentence = resources.getString(offerRefusalMessage(reason))
            compose.onNodeWithTag(TAG_NOTICE).assertIsDisplayed()
            compose.onNodeWithText(sentence).assertIsDisplayed()
            Evidence.log("refusal $reason = $sentence")
        }
    }

    /**
     * The four reasons an Offer cannot receive share one sentence.
     *
     * They describe Watched Capture, which a phone never performs, and the facade
     * passes their inputs in inert — so they are unreachable by construction. Four
     * invented sentences nobody can ever read is copy nobody keeps true; one that
     * reads as the surprise it would be is honest, and the exhaustive `when` is
     * what stops them being quietly omitted instead.
     */
    @Test
    fun the_refusals_an_offer_cannot_receive_share_one_sentence() {
        val inert = listOf(
            SkipReason.DISABLED,
            SkipReason.DENY_LIST,
            SkipReason.SELF_WRITE,
            SkipReason.TRANSIENT,
        )
        assertEquals(
            "the inert reasons must not sprout copy that can never be shown",
            setOf(R.string.offer_refused_unreachable),
            inert.map { offerRefusalMessage(it) }.toSet(),
        )
        Evidence.log(
            "inert reasons = ${inert.joinToString()} all read: " +
                resources.getString(R.string.offer_refused_unreachable),
        )
    }

    /**
     * A Recall Latest that fell back to the cache says so, in a container of its
     * own.
     *
     * `TAG_NOTICE_STALE` is what the end-to-end test asserts, so it is worth
     * knowing here that the tag really carries the sentence and that no other
     * notice wears it.
     */
    @Test
    fun a_recall_from_the_cache_says_so_and_nothing_else_wears_that_tag() {
        show(UiState(session = SessionPhase.OutOfContact("u"), notice = Notice.RecalledFromCache))
        val sentence = resources.getString(R.string.recall_from_cache)
        compose.onNodeWithTag(TAG_NOTICE_STALE).assertIsDisplayed()
        compose.onNodeWithText(sentence).assertIsDisplayed()
        Evidence.log("stale recall  = $sentence")
    }

    @Test
    fun a_successful_recall_does_not_wear_the_stale_tag() {
        show(UiState(session = SessionPhase.InContact("u"), notice = Notice.Recalled))
        compose.onNodeWithTag(TAG_NOTICE_STALE).assertDoesNotExist()
        compose.onNodeWithText(resources.getString(R.string.recall_done)).assertIsDisplayed()
    }

    /**
     * The pending count is on screen when there is one, and gone when there is
     * not.
     *
     * Sync is foreground-only, so an Offer made with no connection waits for the
     * next time the app is opened. A queue nobody can see is a queue nobody comes
     * back for. The depth is a readout beside the sentence rather than a number
     * inside it, so both halves are asserted: a band that lost its numeral would
     * still read as a sentence about nothing in particular.
     */
    @Test
    fun the_pending_count_is_surfaced_and_disappears_when_the_queue_drains() {
        show(UiState(session = SessionPhase.OutOfContact("u"), pending = 2))
        val two = resources.getQuantityString(R.plurals.pending_count, 2)
        compose.onNodeWithTag(TAG_PENDING).assertTextEquals(two)
        compose.onNodeWithTag(TAG_PENDING_COUNT).assertTextEquals("2")
        Evidence.log("pending 2     = $two")

        show(UiState(session = SessionPhase.InContact("u"), pending = 0))
        compose.onNodeWithTag(TAG_PENDING).assertDoesNotExist()
        compose.onNodeWithTag(TAG_PENDING_COUNT).assertDoesNotExist()
    }

    /**
     * A readable Entry is deleted by dragging for it, never by a tap next to
     * Recall.
     *
     * The guard the desktop's unguarded `✕` never had, and the reason the row
     * lost its second word-button: a Delete fans out over SSE to every paired
     * device and cannot be undone, while Recall is a single-tap errand that
     * happens twenty times a day. So the two are not adjacent targets — one is a
     * 48dp square and the other is a gesture.
     *
     * The assertion that matters is the *pair*: pressing where the delete panel
     * sits does nothing at all, and only the swipe reaches the action.
     */
    @Test
    fun a_readable_entry_is_deleted_by_a_swipe_and_not_by_a_tap() {
        show(
            UiState(
                session = SessionPhase.InContact("u"),
                entries = listOf(entry(id = 4, preview = "caddy reverse_proxy localhost:8787")),
            ),
        )

        compose.onNodeWithTag(entryDeleteTag(4)).performClick()
        assertTrue("a tap must not delete an Entry", deleted.isEmpty())

        compose.onNodeWithTag(entryRowTag(4)).performTouchInput { swipeLeft() }
        compose.waitUntil(5_000) { deleted.isNotEmpty() }
        assertEquals("the swipe must ask for exactly this Entry", listOf(4L), deleted)
        Evidence.log("delete        = a tap does nothing; a swipe asks for Entry 4")
    }
}
