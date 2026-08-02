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
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.ui.HistoryScreen
import com.sharepaste.android.ui.Notice
import com.sharepaste.android.ui.SessionPhase
import com.sharepaste.android.ui.SharepasteTheme
import com.sharepaste.android.ui.TAG_FILTER_CLEAR
import com.sharepaste.android.ui.TAG_FILTER_COUNT
import com.sharepaste.android.ui.TAG_FILTER_FIELD
import com.sharepaste.android.ui.TAG_HISTORY_CACHE_BOUNDARY
import com.sharepaste.android.ui.TAG_HISTORY_EMPTY
import com.sharepaste.android.ui.TAG_HISTORY_LIST
import com.sharepaste.android.ui.TAG_HISTORY_NO_MATCHES
import com.sharepaste.android.ui.TAG_NOTICE
import com.sharepaste.android.ui.TAG_PENDING
import com.sharepaste.android.ui.TAG_PENDING_COUNT
import com.sharepaste.android.ui.TAG_RECALL_FIRST
import com.sharepaste.android.ui.UiState
import com.sharepaste.android.ui.entryDeleteTag
import com.sharepaste.android.ui.entryOriginTag
import com.sharepaste.android.ui.entryPreviewTag
import com.sharepaste.android.ui.entryRecallTag
import com.sharepaste.android.ui.entryRowTag
import com.sharepaste.android.ui.entryUndecryptableTag
import com.sharepaste.android.ui.filtered
import com.sharepaste.android.ui.offerRefusalMessage
import com.sharepaste.core.SkipReason
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
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
 * And **where the list is** when a row takes the head, which the desktop never
 * had to answer because its History was a pane beside a detail view rather than
 * the whole screen. Here it is the screen, the first row is the one
 * `RECALL FIRST` hands over, and four bands above the list can each appear and
 * push that row under the top of the viewport. So an Entry that arrives has to
 * come to the person — and nothing else may move the list, because a viewport
 * that jumps while somebody is reading loses their place for them.
 *
 * And **the Filter**, which is the one control on this screen that changes what
 * the list is rather than what it says: the rows it leaves, the count beside
 * the field, and the two different things an empty list can mean.
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

    /** Every Entry the verb bar or a row asked to have recalled, in order. */
    private val recalled = mutableListOf<Long>()

    /** What the Filter field was last set to, so a `✕` can be read back. */
    private val typed = mutableListOf<String>()

    /**
     * The state holder's own scroll signal, driven by hand.
     *
     * The list follows the head on an **arrival** and on a **Use this device
     * made**, and on nothing else — so a test that could only change the state
     * could not tell those apart from a remote Use, which is exactly the
     * distinction the rule exists for. Configured as the state holder
     * configures it, so a signal raised before the screen collects is dropped
     * here too.
     */
    private val headMoves = MutableSharedFlow<Long>(
        extraBufferCapacity = 1,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )

    @Before
    fun render() {
        compose.setContent {
            SharepasteTheme {
                HistoryScreen(
                    state = shown.collectAsState().value,
                    actions = noActions(
                        setFilter = { typed += it },
                        recall = { recalled += it.id },
                        deleteEntry = { deleted += it.id },
                    ),
                    headMoves = headMoves,
                )
            }
        }
    }

    private fun show(state: UiState) {
        shown.value = state
        compose.waitForIdle()
    }

    /**
     * A row took the head, and the list should follow it there.
     *
     * What the state holder emits from `EntryAdded` and from its own `recall`.
     */
    private fun headMovedTo(id: Long) {
        headMoves.tryEmit(id)
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
     * A Recall says nothing on this screen at all.
     *
     * `RECALL FIRST` selects from the cache and performs no round trip
     * (ADR 0010), so the one outcome that used to need a band — the fetch
     * failed and yesterday's link is on your clipboard — cannot arise here any
     * more. The Standing Actions' Recall still fetches and still says so, out
     * loud, on a surface that has no band; `StandingActionsOnAClosedPhoneTest`
     * is where that rule is held.
     *
     * The assertion is that the chrome is empty, which is stronger than the tag
     * comparison it replaces: a second notice growing a claim about a Recall
     * would fail here rather than merely fail to be the wrong colour.
     */
    @Test
    fun a_recall_draws_no_band_on_this_screen() {
        show(
            UiState(
                session = SessionPhase.OutOfContact("u"),
                entries = listOf(entry(id = 1, preview = "ssh me@box")),
            ),
        )
        compose.onNodeWithTag(TAG_RECALL_FIRST).performClick()

        assertEquals("the verb bar must recall the first displayed row", listOf(1L), recalled)
        compose.onNodeWithTag(TAG_NOTICE).assertDoesNotExist()
        Evidence.log("recall        = Entry 1 asked for, and no band anywhere on the screen")
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
     * This is the whole point of the rule. The first row is the one
     * `RECALL FIRST` will hand over, and the distance to it is not fixed: the
     * pending band, the divergence band, the blocked-notifications note and a
     * notice can each be there or not, and each one pushes index 0 further
     * under the top of the viewport. An Offer made from this phone has to leave
     * its own row visible, or the person presses the verb bar on something they
     * cannot see.
     *
     * **Without the effect this fails outright**, and not marginally: nothing
     * else on the screen holds the `LazyListState`, so a prepend leaves the
     * list exactly where the scroll to index 20 put it, and Entry 41 sits at
     * index 0 twenty-one rows above the viewport — where a `LazyColumn`
     * composes nothing, so `awaitRow` times out with no node to fetch rather
     * than fetching one that is off screen.
     */
    @Test
    fun a_new_entry_brings_the_top_of_the_list_back_into_view() {
        show(history(40L downTo 1L))
        scrollTo(readingIndex)
        compose.onNodeWithTag(entryRowTag(20)).assertIsDisplayed()
        compose.onNodeWithTag(entryRowTag(40)).assertDoesNotExist()

        // The Offer lands: the list grows and the state holder says an Entry
        // arrived. Nobody touches the screen between here and the assertion.
        show(history(41L downTo 1L))
        headMovedTo(41)

        awaitRow(41)
        Evidence.log("arrival       = Entry 41 landed 21 rows above the reader and came into view")
    }

    /**
     * A **Use another device made** reorders the list and must not move the
     * reader.
     *
     * The case Last Use created and the reason the old rule broke. Under
     * capture ordering "a new head with the old head still under it" *was* "an
     * Entry arrived", because nothing else could put a row on top. It can now:
     * somebody recalls a buried Entry on their laptop, `HistoryChanged`
     * refetches, and this phone's head changes with nothing new in the list.
     *
     * Chasing it costs the reader their place to show them a row they already
     * had, so nothing is emitted and nothing moves. **The mutant this kills** is
     * any rule derived from the list itself — head id, head identity, list
     * length — because every one of them fires here.
     */
    @Test
    fun a_use_on_another_device_reorders_the_list_and_leaves_the_reader_alone() {
        show(history(40L downTo 1L))
        scrollTo(readingIndex)
        compose.onNodeWithTag(entryRowTag(20)).assertIsDisplayed()

        // Entry 7 was used somewhere else, so it leads the History now. Same
        // forty Entries, same ids, one new order, and no arrival.
        show(history(listOf(7L) + (40L downTo 8L) + (6L downTo 1L)))

        compose.onNodeWithTag(entryRowTag(7)).assertDoesNotExist()
        compose.onNodeWithTag(entryRowTag(20)).assertIsDisplayed()

        // And the list can still move, so the two assertions above are about
        // this reorder and not about an effect that does nothing.
        show(history(listOf(41L, 7L) + (40L downTo 8L) + (6L downTo 1L)))
        headMovedTo(41)
        awaitRow(41)
        Evidence.log("remote use    = the head changed, Entry 20 stayed; an arrival still scrolls")
    }

    /**
     * A **Use this device made** follows the row.
     *
     * The other half of the pair, and the reason the signal carries the id
     * rather than being derived: the core cannot say whose Use it was —
     * `HistoryChanged` carries only a `user_id` — but the state holder can,
     * because `recall` knows the Entry it just passed.
     *
     * `animateScrollToItem(0)` *is* the "has it left the viewport" check: the
     * recalled row always lands at index 0, so following it is a no-op when the
     * top is already in view and a scroll when it is not.
     */
    @Test
    fun a_use_this_device_made_follows_the_row_to_the_head() {
        show(history(40L downTo 1L))
        scrollTo(readingIndex)
        compose.onNodeWithTag(entryRowTag(40)).assertDoesNotExist()

        // The row under the reader's thumb is recalled here. The same forty
        // Entries in a new order, and the state holder says it was ours.
        show(history(listOf(20L) + (40L downTo 21L) + (19L downTo 1L)))
        headMovedTo(20)

        awaitRow(20)
        Evidence.log("own use       = Entry 20 took the head and the list followed it there")
    }

    /**
     * Deleting the newest row is not an arrival and must not be answered like
     * one.
     *
     * Removing index 0 changes the head too. Somebody twenty rows down who has
     * just swiped a row away has said nothing whatsoever about wanting to be at
     * the top, and hauling them there costs them the place they were reading —
     * over a row they got rid of on purpose.
     *
     * `LazyListState.firstVisibleItemIndex` belongs to a `rememberLazyListState`
     * inside [com.sharepaste.android.ui.HistoryScreen] and is not reachable from
     * out here, so the assertion is observational: the row that was under the
     * reader's eyes is still under them, and the new head is still not composed.
     */
    @Test
    fun deleting_the_newest_row_does_not_move_the_reader() {
        show(history(40L downTo 1L))
        scrollTo(readingIndex)
        compose.onNodeWithTag(entryRowTag(20)).assertIsDisplayed()

        // The head leaves. Entry 39 is the first row now, and that is not news
        // anybody asked to be shown.
        show(history(39L downTo 1L))

        compose.onNodeWithTag(entryRowTag(20)).assertIsDisplayed()
        compose.onNodeWithTag(entryRowTag(39)).assertDoesNotExist()

        show(history(listOf(41L) + (39L downTo 1L)))
        headMovedTo(41)
        awaitRow(41)
        Evidence.log("deletion      = the head left, Entry 20 stayed; an arrival still scrolls")
    }

    /**
     * A list somebody has not read a word of is not somewhere to drag them to
     * the top of.
     *
     * Switching the Viewed Pairing replaces every Entry at once, so the head
     * changes for a fourth distinct reason and shares no id with what was there
     * before. The `LazyColumn` keeps its index because that is all it has to go
     * on, and the new Pairing therefore opens roughly where the last one was
     * left. That is not ideal and it is not this effect's business: what matters
     * is that nothing here adds a *deliberate* jump on top of it. Somebody who
     * switched Pairings to look for something is about to scroll anyway;
     * somebody the app has yanked to the top has lost the only reference point
     * they had for where they already were.
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
        headMovedTo(141)
        awaitRow(141)
        Evidence.log("switch        = a wholly new list did not jump to its own index 0")
    }

    // -- the Filter -----------------------------------------------------------

    /**
     * A [UiState] as the state holder would hand one over for [needle].
     *
     * `scanned` is the answer the filtering job would have written back, so this
     * is the snapshot a screen actually sees rather than one hand-assembled to
     * suit an assertion. With a blank needle it is not read at all — see
     * [UiState.shown].
     */
    private fun filtering(needle: String, entries: List<com.sharepaste.core.Entry>) = UiState(
        session = SessionPhase.InContact("u"),
        entries = entries,
        filter = needle,
        scanned = filtered(needle.trim(), entries),
    )

    private val threeEntries = listOf(
        entry(id = 3, preview = "ssh deploy@staging"),
        entry(id = 2, preview = "the standing order reference"),
        entry(id = 1, preview = "SSH KEY FINGERPRINT"),
    )

    /**
     * The field is a field: what is typed into it reaches the state holder.
     *
     * The one assertion nothing else here can make. Every test below hands the
     * screen a needle that is already in the snapshot, which proves what the
     * screen does with one and says nothing about whether a keystroke ever
     * becomes one.
     */
    @Test
    fun typing_into_the_filter_reaches_the_state_holder_verbatim() {
        show(filtering("", threeEntries))

        compose.onNodeWithTag(TAG_FILTER_FIELD).performTextInput("Ssh ")

        // Joined rather than taken last: the field is controlled and this test
        // never feeds its own value back, so whether the platform commits the
        // string in one edit or four is not something this is about.
        assertEquals(
            "the field must hand over what was typed, un-recased and untrimmed",
            "Ssh ",
            typed.joinToString(""),
        )
        Evidence.log("typing        = \"Ssh \" reached the state holder exactly as typed")
    }

    /**
     * The Filter narrows the rows, and the count says how far.
     *
     * Case-insensitively, because a needle is matched against text somebody
     * else copied and nobody types a fingerprint in capitals to find it. The
     * count is `shown/held`, so the denominator is the whole History and not
     * some other filtered number.
     */
    @Test
    fun a_needle_leaves_the_rows_that_contain_it_whatever_their_case() {
        show(filtering("ssh", threeEntries))

        compose.onNodeWithTag(entryRowTag(3)).assertIsDisplayed()
        compose.onNodeWithTag(entryRowTag(1)).assertIsDisplayed()
        compose.onNodeWithTag(entryRowTag(2)).assertDoesNotExist()
        compose.onNodeWithTag(TAG_FILTER_COUNT).assertTextEquals("2/3")
        Evidence.log("needle \"ssh\"  = Entries 3 and 1 survive, 2 does not, and the count reads 2/3")
    }

    /**
     * The predicate reads the whole plaintext, not the Preview.
     *
     * The Preview is capped at 80 characters, which truncates exactly the long
     * Entries a Filter exists to tell apart: the distinguishing word of a
     * config block or a URL list is almost never in its first line. A phone has
     * no reader pane, so the match cannot be shown — that is the accepted cost
     * of finding the row at all.
     */
    @Test
    fun the_needle_matches_text_the_preview_does_not_show() {
        val buried = entry(
            id = 9,
            preview = "server {",
            plaintext = "server {\n  listen 443;\n  server_name borogoves.example;\n}",
        )
        show(filtering("borogoves", listOf(buried)))

        compose.onNodeWithTag(entryRowTag(9)).assertIsDisplayed()
        compose.onNodeWithTag(entryPreviewTag(9)).assertTextEquals("server {")
        Evidence.log("off-preview   = \"borogoves\" appears nowhere in the row and still found it")
    }

    /**
     * An **Undecryptable** Entry carries no plaintext and matches nothing.
     *
     * The truth about it rather than an omission: this device holds ciphertext
     * and no key, so there is no text for a needle to be in. It is still in the
     * denominator, because the phone does hold it.
     */
    @Test
    fun an_undecryptable_entry_matches_no_needle_at_all() {
        val sealed = entry(id = 5, preview = "", plaintext = null, undecryptable = true)
        show(filtering("e", listOf(sealed)))

        compose.onNodeWithTag(entryRowTag(5)).assertDoesNotExist()
        compose.onNodeWithTag(TAG_HISTORY_NO_MATCHES).assertIsDisplayed()
        compose.onNodeWithTag(TAG_FILTER_COUNT).assertTextEquals("0/1")
        Evidence.log("undecryptable = no plaintext, so no needle matches; still counted as held")
    }

    /**
     * An empty History and a needle that matched nothing are two different
     * sentences, and the first one wins.
     *
     * `NO MATCHES` on a phone holding no Entries at all would send somebody
     * looking for a query to fix when the answer is that nothing has been
     * offered yet.
     */
    @Test
    fun an_empty_history_says_so_rather_than_blaming_the_needle() {
        show(filtering("anything", emptyList()))

        compose.onNodeWithTag(TAG_HISTORY_EMPTY).assertIsDisplayed()
        compose.onNodeWithTag(TAG_HISTORY_NO_MATCHES).assertDoesNotExist()

        show(filtering("nothing here matches this", threeEntries))
        compose.onNodeWithTag(TAG_HISTORY_NO_MATCHES).assertIsDisplayed()
        compose.onNodeWithTag(TAG_HISTORY_EMPTY).assertDoesNotExist()
        Evidence.log("empty states  = HISTORY EMPTY outranks NO MATCHES, and both are reachable")
    }

    /**
     * The cache boundary is named only at the cap.
     *
     * A phone keeps the hundred most recently used Entries. Below that the list
     * is everything there is, so "what you are looking for may be on the Relay"
     * would be a lecture about a boundary nobody is near — and above it, it is
     * the one thing this panel can say that is not already on screen.
     */
    @Test
    fun the_cache_boundary_is_named_when_the_phone_is_full_and_not_before() {
        show(filtering("nothing here matches this", threeEntries))
        compose.onNodeWithTag(TAG_HISTORY_CACHE_BOUNDARY).assertDoesNotExist()

        show(filtering("nothing here matches this", (100L downTo 1L).map { entry(id = it) }))
        compose.onNodeWithTag(TAG_HISTORY_CACHE_BOUNDARY)
            .assertTextEquals(resources.getString(R.string.history_cache_boundary))
        Evidence.log("cache cap     = the boundary is stated at 100 Entries and at no fewer")
    }

    /**
     * The `✕` is there exactly while there is a needle to clear.
     *
     * Chrome that is always present and usually inert is chrome a thumb learns
     * to ignore. The count is not conditional in the same way: a phone with
     * Entries always says how many, which is where the hundred-row boundary is
     * legible before anybody reaches it.
     */
    @Test
    fun the_clear_control_appears_only_with_something_to_clear() {
        show(filtering("", threeEntries))
        compose.onNodeWithTag(TAG_FILTER_CLEAR).assertDoesNotExist()
        compose.onNodeWithTag(TAG_FILTER_COUNT).assertTextEquals("3/3")

        show(filtering("ssh", threeEntries))
        compose.onNodeWithTag(TAG_FILTER_CLEAR).performClick()

        assertEquals("the ✕ must empty the field and nothing else", listOf(""), typed)
        Evidence.log("clear         = absent with no needle, and empties the field when pressed")
    }

    /**
     * The marked row and the verb bar are the same decision.
     *
     * This is the whole of rows 15–18: once a Filter can hide the head,
     * "the newest Entry" and "the row wearing the emitter's rule" are two
     * different Entries with one button between them. Both are positional now,
     * so the button hands over the row the list says it will.
     */
    @Test
    fun the_verb_bar_recalls_the_first_displayed_row_and_not_the_newest() {
        show(filtering("standing", threeEntries))

        compose.onNodeWithTag(entryRowTag(3)).assertDoesNotExist()
        compose.onNodeWithTag(TAG_RECALL_FIRST).performClick()

        assertEquals("the Filter hid Entry 3, so the verb bar owes Entry 2", listOf(2L), recalled)
        Evidence.log("recall first  = Entry 3 is the newest and hidden; the bar handed over 2")
    }

    /**
     * With no first row the verb says no where the person is looking.
     *
     * Disabled rather than absent, and rather than a notice: the control someone
     * is reaching for has to still be there, saying no. An Undecryptable first
     * row counts as none — its own Recall is disabled for want of a key, and a
     * verb bar that fired on it would contradict the row it is marking.
     */
    @Test
    fun the_verb_bar_is_disabled_when_there_is_no_first_row_to_hand_over() {
        show(filtering("nothing here matches this", threeEntries))
        compose.onNodeWithTag(TAG_RECALL_FIRST).assertIsNotEnabled()

        show(filtering("", listOf(entry(id = 5, preview = "", plaintext = null, undecryptable = true))))
        compose.onNodeWithTag(TAG_RECALL_FIRST).assertIsNotEnabled()

        show(filtering("", threeEntries))
        compose.onNodeWithTag(TAG_RECALL_FIRST).assertIsEnabled()
        Evidence.log("verb bar      = disabled with no match and with a sealed head; enabled otherwise")
    }
}
