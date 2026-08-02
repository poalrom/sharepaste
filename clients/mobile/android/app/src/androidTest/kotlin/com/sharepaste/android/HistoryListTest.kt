package com.sharepaste.android

import androidx.compose.runtime.collectAsState
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToIndex
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.ui.HistoryScreen
import com.sharepaste.android.ui.Notice
import com.sharepaste.android.ui.SessionPhase
import com.sharepaste.android.ui.SharepasteTheme
import com.sharepaste.android.ui.TAG_HISTORY_LIST
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
 * And **where the list is** when an Entry arrives, which the desktop never had
 * to answer because its History was a pane beside a detail view rather than the
 * whole screen. Here it is the screen, the newest row is the one `RECALL
 * LATEST` hands over, and four bands above the list can each appear and push
 * that row under the top of the viewport. So the newest Entry has to come to
 * the person — and nothing else may move the list, because a viewport that
 * jumps while somebody is reading loses their place for them.
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
     * the one outcome a person cannot act on. The two reachable reasons need two
     * sentences because each one needs a different thing done about it.
     */
    @Test
    fun each_offer_refusal_reads_as_its_own_sentence() {
        val reachable = listOf(SkipReason.NON_TEXT, SkipReason.TOO_LARGE)
        val sentences = reachable.map { resources.getString(offerRefusalMessage(it)) }
        assertEquals("each reachable refusal needs its own words", 2, sentences.toSet().size)

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

    /**
     * A Recall that reached the Relay says nothing on this screen, and only the
     * fallback does.
     *
     * The stale tag has to stay distinguishable from an ordinary success,
     * because it is the only thing between a person and yesterday's link: a
     * Recall Latest that could not reach the Relay hands over the cache, and the
     * hand-over itself looks identical either way.
     *
     * Since a plain Recall became a [com.sharepaste.android.ui.Receipt] there is
     * no band at all for a success to wear the wrong tag on, so the assertion is
     * that the chrome is empty. That is a stronger statement of the same rule
     * than the one it replaces: "it wore the other tag" allowed a band, and this
     * allows none — a second notice growing a claim about a Recall would fail
     * here rather than merely fail to be amber.
     */
    @Test
    fun a_successful_recall_draws_no_band_and_only_the_cache_fallback_does() {
        show(UiState(session = SessionPhase.InContact("u"), notice = null))
        compose.onNodeWithTag(TAG_NOTICE).assertDoesNotExist()
        compose.onNodeWithTag(TAG_NOTICE_STALE).assertDoesNotExist()

        show(UiState(session = SessionPhase.OutOfContact("u"), notice = Notice.RecalledFromCache))
        compose.onNodeWithTag(TAG_NOTICE_STALE).assertIsDisplayed()
        Evidence.log("plain recall  = no band at all; the cache fallback still wears the stale tag")
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
     * A readable Entry is deleted by dragging for it, and then by pressing the
     * thing the drag uncovered.
     *
     * The guard the desktop's unguarded `✕` never had, and the reason the row
     * lost its second word-button: a Delete fans out over SSE to every paired
     * device and cannot be undone, while Recall is a single-tap errand that
     * happens twenty times a day. So the two are not adjacent targets — one is a
     * 48dp square and the other is a gesture.
     *
     * **What changed is the second half of that gesture.** The panel used to
     * answer a press with nothing, which is the worst moment for a control to be
     * inert: it has just been revealed to somebody who has this instant
     * discovered the swipe. It is a button now, and the *arming* is what keeps
     * the guard — the same node is a control only while the drag is holding it
     * open.
     *
     * The pair is the assertion. At rest the row offers one target and a press
     * where the panel sits deletes nothing; uncovered, that node is enabled and
     * fires the same `onDelete` a completed swipe fires.
     */
    @Test
    fun the_delete_panel_is_a_control_only_while_the_swipe_holds_it_open() {
        show(
            UiState(
                session = SessionPhase.InContact("u"),
                entries = listOf(entry(id = 4, preview = "caddy reverse_proxy localhost:8787")),
            ),
        )

        // At rest. The panel is composed behind the row on every frame, so the
        // question is not whether it is there but whether it is a control.
        compose.onNodeWithTag(entryDeleteTag(4)).assertIsNotEnabled()
        compose.onNodeWithTag(entryDeleteTag(4)).performClick()
        assertTrue("a tap on an un-swiped row must not delete an Entry", deleted.isEmpty())

        // Held open, with no release, so nothing settles: the box's own
        // `onDismiss` cannot be what fires here, and the panel is the only thing
        // that can. Short of half the row's width on purpose, because the state
        // reports the *nearest* anchor and reaching the far one would trip the
        // row's own reset while a finger is still down.
        compose.onNodeWithTag(entryRowTag(4)).performTouchInput {
            down(centerRight)
            moveTo(Offset(width * 0.6f, centerY))
        }
        compose.onNodeWithTag(entryDeleteTag(4)).assertIsEnabled()
        compose.onNodeWithTag(entryDeleteTag(4)).performSemanticsAction(SemanticsActions.OnClick)
        assertEquals("the uncovered panel must ask for exactly this Entry", listOf(4L), deleted)

        // Put the finger back where it started before lifting it, so the row
        // settles home rather than into a dismissal this test never asked for.
        compose.onNodeWithTag(entryRowTag(4)).performTouchInput {
            moveTo(centerRight)
            up()
        }
        Evidence.log("delete panel  = inert at rest, a button once the swipe uncovers it")
    }

    /**
     * The gesture on its own still deletes.
     *
     * The panel becoming a button adds a way to finish the delete; it does not
     * replace the one that was already there. Somebody who drags the row all the
     * way off has said what they want as plainly as anyone can, and must not be
     * left holding an armed control they now have to find and press.
     */
    @Test
    fun a_completed_swipe_still_deletes_the_entry_on_its_own() {
        show(
            UiState(
                session = SessionPhase.InContact("u"),
                entries = listOf(entry(id = 4, preview = "caddy reverse_proxy localhost:8787")),
            ),
        )

        compose.onNodeWithTag(entryRowTag(4)).performTouchInput { swipeLeft() }
        compose.waitUntil(5_000) { deleted.isNotEmpty() }
        assertEquals("the swipe must ask for exactly this Entry", listOf(4L), deleted)
        Evidence.log("delete swipe  = a completed drag asks for Entry 4 with no tap at all")
    }

    /**
     * A History of [ids], newest first.
     *
     * Forty rows is far past any viewport this runs in, which is what lets the
     * newest row be scrolled out of the *composition* and not merely out of
     * sight — so `assertDoesNotExist` on index 0 is a reading of where the list
     * is rather than a guess about clipping. The rows are keyed by `entry.id`,
     * so a list that loses or gains a row above the reader keeps the reader on
     * the row they were on: whatever the viewport does after a state change is
     * the effect's doing and not the `LazyColumn` finding its place again.
     */
    private fun history(ids: Iterable<Long>) = UiState(
        session = SessionPhase.InContact("u"),
        entries = ids.map { entry(id = it) },
    )

    /**
     * Twenty rows below the newest, which is where every case below starts.
     *
     * Far enough down that index 0 is not composed, and far enough from the end
     * of forty that the list can genuinely put this row at the top rather than
     * running out of content and stopping short.
     */
    private val readingIndex = 20

    private fun scrollTo(index: Int) {
        compose.onNodeWithTag(TAG_HISTORY_LIST).performScrollToIndex(index)
        compose.waitForIdle()
    }

    /**
     * The row is composed and on screen, once the list has stopped moving.
     *
     * `animateScrollToItem` is a suspending animation and not a jump, so an
     * assertion made on the frame the state changed would be reading the list
     * mid-flight and would pass or fail on timing rather than on behaviour.
     */
    private fun awaitRow(id: Long) {
        compose.waitUntil(5_000) {
            compose.onAllNodesWithTag(entryRowTag(id)).fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithTag(entryRowTag(id)).assertIsDisplayed()
    }

    /**
     * An Entry that arrives while somebody is twenty rows down is not an Entry
     * they should have to go looking for.
     *
     * This is the whole point of the rule. The newest row is the one `RECALL
     * LATEST` will hand over, and the distance to it is not fixed: the pending
     * band, the divergence band, the blocked-notifications note and a notice can
     * each be there or not, and each one pushes index 0 further under the top of
     * the viewport. Retiring the two confirmation banners bought back the height
     * and none of the guarantee — an Offer made from this phone has to leave its
     * own row visible, or the person taps `RECALL LATEST` on something they
     * cannot see.
     *
     * **Without `NewestEntryStaysInView` this fails outright**, and not
     * marginally: nothing else on the screen holds the `LazyListState`, so a
     * prepend leaves the list exactly where the scroll to index 20 put it, and
     * Entry 41 sits at index 0 twenty-one rows above the viewport — where a
     * `LazyColumn` composes nothing, so `awaitRow` times out with no node to
     * fetch rather than fetching one that is off screen.
     */
    @Test
    fun a_new_entry_brings_the_top_of_the_list_back_into_view() {
        show(history(40L downTo 1L))
        scrollTo(readingIndex)
        compose.onNodeWithTag(entryRowTag(20)).assertIsDisplayed()
        compose.onNodeWithTag(entryRowTag(40)).assertDoesNotExist()

        // The Offer lands. Nothing else about the state changes, and nobody
        // touches the screen between here and the assertion.
        show(history(41L downTo 1L))

        awaitRow(41)
        Evidence.log("arrival       = Entry 41 landed 21 rows above the reader and came into view")
    }

    /**
     * Deleting the newest row is not an arrival and must not be answered like
     * one.
     *
     * This is the case the effect's keying is subtle about, and the reason it
     * asks *"a new head with the old head still under it"* rather than the
     * obvious *"the head changed"*. Removing index 0 changes the head too.
     * Somebody twenty rows down who has just swiped a row away has said nothing
     * whatsoever about wanting to be at the top, and hauling them there costs
     * them the place they were reading — over a row they got rid of on purpose.
     *
     * `LazyListState.firstVisibleItemIndex` belongs to a `rememberLazyListState`
     * inside [com.sharepaste.android.ui.HistoryScreen] and is not reachable from
     * out here, so the assertion is observational: the row that was under the
     * reader's eyes is still under them, and the new head is still not composed.
     *
     * **The mutant this kills** is `if (newest != seen) animateScrollToItem(0)`:
     * under it the delete scrolls to Entry 39, and both assertions fail. The
     * prepend at the end is not decoration — an assertion that the list did not
     * move is only worth having beside proof that this list still can, or
     * deleting the effect outright would satisfy the first half.
     */
    @Test
    fun deleting_the_newest_row_does_not_move_the_reader() {
        show(history(40L downTo 1L))
        scrollTo(readingIndex)
        compose.onNodeWithTag(entryRowTag(20)).assertIsDisplayed()

        // The head leaves. Entry 39 is the newest now, and that is not news
        // anybody asked to be shown.
        show(history(39L downTo 1L))

        compose.onNodeWithTag(entryRowTag(20)).assertIsDisplayed()
        compose.onNodeWithTag(entryRowTag(39)).assertDoesNotExist()

        // And the list can still move, so the two assertions above are about
        // this delete and not about an effect that does nothing.
        show(history(listOf(41L) + (39L downTo 1L)))
        awaitRow(41)
        Evidence.log("deletion      = the head left, Entry 20 stayed; an arrival still scrolls")
    }

    /**
     * A list somebody has not read a word of is not somewhere to drag them to
     * the top of.
     *
     * Switching the Viewed Pairing replaces every Entry at once, so the head
     * changes for a third distinct reason and shares no id with what was there
     * before. The `LazyColumn` keeps its index because that is all it has to go
     * on, and the new Pairing therefore opens roughly where the last one was
     * left. That is not ideal and it is not this effect's business: what matters
     * is that nothing here adds a *deliberate* jump on top of it. Somebody who
     * switched Pairings to look for something is about to scroll anyway;
     * somebody the app has yanked to the top has lost the only reference point
     * they had for where they already were.
     *
     * **Same mutant, third route.** `newest != seen` is true for a switch as
     * much as for a prepend, and a scroll keyed on that alone would put Entry
     * 140 on screen — which is exactly what the first assertion refuses. The
     * only thing that separates this from an arrival is that the old head is
     * gone, which is the `entries.any { it.id == seen }` guard and nothing else.
     * The closing prepend is the same control as in the case above.
     */
    @Test
    fun switching_the_viewed_pairing_does_not_drag_the_reader_to_the_top() {
        show(history(40L downTo 1L))
        scrollTo(readingIndex)
        compose.onNodeWithTag(entryRowTag(20)).assertIsDisplayed()

        // A different Pairing's Entries, sharing no id with the ones on screen.
        show(history(140L downTo 101L))

        compose.onNodeWithTag(entryRowTag(140)).assertDoesNotExist()
        compose.onNodeWithTag(entryRowTag(120)).assertIsDisplayed()

        show(history(listOf(141L) + (140L downTo 101L)))
        awaitRow(141)
        Evidence.log("switch        = a wholly new list did not jump to its own index 0")
    }
}
