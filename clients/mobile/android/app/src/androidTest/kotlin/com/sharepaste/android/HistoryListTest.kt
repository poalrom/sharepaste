package com.sharepaste.android

import androidx.compose.runtime.collectAsState
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PixelMap
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToIndex
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
import androidx.compose.ui.test.swipeUp
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.ui.HeadMove
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
import com.sharepaste.android.ui.entryReadTag
import com.sharepaste.android.ui.entryRecallTag
import com.sharepaste.android.ui.entryRefusedTag
import com.sharepaste.android.ui.entryResendTag
import com.sharepaste.android.ui.entryRowTag
import com.sharepaste.android.ui.entryTextTag
import com.sharepaste.android.ui.entryUndecryptableTag
import com.sharepaste.android.ui.filtered
import com.sharepaste.android.ui.offerRefusalMessage
import com.sharepaste.core.SkipReason
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
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
 * And **the Place** — where the list is — when a row takes the head, which the
 * desktop never had to answer because its History was a pane beside a detail
 * view rather than the whole screen. Here it is the screen, the first row is the
 * one `RECALL FIRST` hands over, and four bands above the list can each appear
 * and push that row under the top of the viewport. Two things move it and no
 * others (ADR 0019): the jump a phone that was away is owed at an open, and a
 * **Use** this phone made. An arrival mid-session moves nothing, which is the
 * assertion this file used to make in reverse.
 *
 * The motions arrive as [HeadMove]s, so the cases here are about what the screen
 * does with each one. *Which* cause raises which is the state holder's, and its
 * gate — armed at an open, spent by the first change or the first hand — is
 * pinned on the JVM by `OpenJumpTest`, where a sequence can be stated without a
 * device.
 *
 * **What is deliberately not asserted here is jump-versus-animation.** Both end
 * at index 0, so telling them apart means counting frames rather than reading
 * behaviour, and a test that passes or fails on how many frames a scroll takes
 * is a test about Compose. What *is* asserted is that the screen's own motion is
 * not mistaken for a hand, which is the failure that reading
 * `isScrollInProgress` would have caused: one programmatic scroll and every
 * later open would silently stop jumping.
 *
 * And **the Filter**, which is the one control on this screen that changes what
 * the list is rather than what it says: the rows it leaves, the count beside
 * the field, and the two different things an empty list can mean.
 *
 * And **the reader**, which is the one thing a row does that reaches nothing at
 * all: a tap opens the Entry whole under it, and reading an Entry is not a Use.
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

    /** Every refused Entry a row's `RESEND` asked to have put back, in order. */
    private val resent = mutableListOf<Long>()

    /** What the Filter field was last set to, so a `✕` can be read back. */
    private val typed = mutableListOf<String>()

    /**
     * How many times the screen has reported that a hand moved the list.
     *
     * The one member of `AppActions` that reports a fact rather than asking for
     * something, and the reason it is counted rather than merely observed: it is
     * what spends the open's jump, so a screen that raised it for its own scrolls
     * would close the gate on somebody who never touched anything.
     */
    private var hands = 0

    /**
     * The state holder's own motion signal, driven by hand.
     *
     * Two motions and nothing else moves the list (ADR 0019), so a test that
     * could only change the state could not tell either of them from a remote
     * Use, from a delete or from an arrival — which is exactly the set of
     * distinctions the rule is made of. Configured as the state holder configures
     * it, so a motion raised before the screen collects is dropped here too.
     */
    private val headMoves = MutableSharedFlow<HeadMove>(
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
                        resend = { resent += it.id },
                        handOnTheList = { hands++ },
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
     * A phone that was away opened, and its first Catch-Up found something.
     *
     * What the state holder emits from the first `HistoryChanged` of a
     * foreground, and from a Viewed Pairing switch.
     */
    private fun jump() {
        headMoves.tryEmit(HeadMove.Jump)
        compose.waitForIdle()
    }

    /**
     * A **Use this phone made** put a row at the head.
     *
     * What the state holder emits from its own `recall`, and the one motion that
     * outlives the open.
     */
    private fun follow() {
        headMoves.tryEmit(HeadMove.Follow)
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

    // -- the queue, drawn -----------------------------------------------------

    private fun rowPixels(id: Long): PixelMap =
        compose.onNodeWithTag(entryRowTag(id)).captureToImage().toPixelMap()

    /**
     * The colour a row was actually painted.
     *
     * A tint is a colour and nothing else: no semantics node carries a
     * background, so a test that read the flag back off the snapshot would pass
     * on a row that drew nothing at all. Capturing the row is the only
     * assertion available that is about what is on the screen.
     *
     * The **most common** pixel, so no coordinate has to be guessed at: a band
     * carrying one line of Preview and one control is overwhelmingly its own
     * fill, and the mode is that fill by construction.
     */
    private fun fillOf(pixels: PixelMap): Color {
        val tally = mutableMapOf<Int, Int>()
        for (y in 0 until pixels.height) {
            for (x in 0 until pixels.width) {
                val argb = pixels[x, y].toArgb()
                tally[argb] = (tally[argb] ?: 0) + 1
            }
        }
        return Color(tally.maxByOrNull { it.value }!!.key)
    }

    /**
     * Warm rather than cool, which is what the amber tint *is*.
     *
     * `Fui.Panel` is a blue-black and every untinted row composites over it —
     * bare, or under row 0's cyan wash — so blue sits above red on both.
     * `Fui.AmberA16` inverts that, and by a wide margin. Pinning the exact
     * composite would say no more and would fail on a rounding nobody can see.
     */
    private fun isTinted(fill: Color) = fill.red > fill.blue

    /**
     * An Entry the Relay has never stamped.
     *
     * Both clocks are zero, because there is one clock in this system and it is
     * the Relay's: an un-flushed capture has no age at all. Nothing on this row
     * can render that as 1970, and the reason is the other half of the same
     * decision — the phone row shows no time, so the tint carries the state and
     * no status word does.
     */
    private fun waiting(id: Long, preview: String, refusedReason: String? = null) = entry(
        id = id,
        preview = preview,
        pending = true,
        refusedReason = refusedReason,
        createdAt = 0,
        lastUse = 0,
    )

    /**
     * A row this phone still owes the Relay is drawn in the queue's own amber,
     * and a row it does not owe is not.
     *
     * The band above the list is the head of a region and the region is these
     * rows: the tint retreats from the bottom up as the uploader drains, which
     * is the flush order shown for nothing. A settled row has to stay on the
     * bare panel, or the tint says nothing by being everywhere.
     *
     * Read off row 1 rather than row 0, so the emitter's rule cannot be what is
     * being read: this row wears no cyan at all.
     */
    @Test
    fun a_pending_row_is_tinted_and_a_settled_one_is_not() {
        show(
            UiState(
                session = SessionPhase.OutOfContact("u"),
                pending = 2,
                entries = listOf(
                    waiting(3, "wg genkey | tee privatekey"),
                    waiting(2, "caddy reverse_proxy localhost:8787"),
                    entry(id = 1, preview = "the Relay has this one"),
                ),
            ),
        )

        val owed = fillOf(rowPixels(2))
        val settled = fillOf(rowPixels(1))
        assertTrue("an un-flushed row wears the queue's amber: $owed", isTinted(owed))
        assertTrue("a row the Relay has is on the bare panel: $settled", !isTinted(settled))

        // And the band is still the head of the region rather than a member of
        // it — whole, and above the rows, exactly where it was (ADR 0014).
        val band = compose.onNodeWithTag(TAG_PENDING).getUnclippedBoundsInRoot()
        val head = compose.onNodeWithTag(entryRowTag(3)).getUnclippedBoundsInRoot()
        assertTrue("the band heads the tinted region, so it stays above it", band.bottom <= head.top)
        Evidence.log("tint          = owed row $owed, settled row $settled")
    }

    /**
     * Row 0 while offline is legitimately both, and says both.
     *
     * A left edge and a background do not compete: the 2dp emitter rule says
     * which row `RECALL FIRST` will hand over, and the field says the act is
     * still owed. What row 0 gives up is its cyan *wash* — 12% cyan under 16%
     * amber composites to a green nobody chose, and offline row 0 is always
     * also pending, so that would have been the ordinary case and not a corner.
     */
    @Test
    fun the_first_row_offline_wears_the_cyan_edge_and_the_amber_tint_at_once() {
        show(
            UiState(
                session = SessionPhase.OutOfContact("u"),
                pending = 1,
                entries = listOf(waiting(1, "ssh deploy@staging")),
            ),
        )

        val pixels = rowPixels(1)
        val fill = fillOf(pixels)
        val edge = pixels[1, pixels.height / 2]

        assertTrue("row 0 offline is an un-flushed row like any other: $fill", isTinted(fill))
        assertTrue(
            "and still the row RECALL FIRST acts on, which its 2dp edge says: $edge",
            edge.green > 0.8f && edge.blue > 0.8f && edge.red < 0.5f,
        )
        compose.onNodeWithTag(entryRecallTag(1)).assertIsEnabled()
        Evidence.log("row 0 offline = amber field $fill behind a cyan edge $edge")
    }

    /**
     * A refusal says what the Relay said, offers the way back into the queue,
     * and does not take the Recall away.
     *
     * The row has one trailing slot and it keeps Recall on purpose: a refused
     * capture's text is stranded on this device, and having it somewhere else
     * was the only reason anybody offered it. So the reason and its `RESEND`
     * are a second line, and the extra height is the row asking for something.
     *
     * The reason is the Relay's own sentence, unreworded. The row names the
     * state in front of it because a red fragment names no state and colour is
     * not a fact a screen reader can read.
     */
    @Test
    fun a_refused_row_carries_its_reason_and_its_resend_and_still_recalls() {
        show(
            UiState(
                session = SessionPhase.OutOfContact("u"),
                pending = 1,
                entries = listOf(waiting(6, "the whole of the log file", "payload too large")),
            ),
        )

        compose.onNodeWithTag(entryRefusedTag(6))
            .assertTextEquals(resources.getString(R.string.entry_refused, "payload too large"))

        compose.onNodeWithTag(entryResendTag(6)).performClick()
        assertEquals("RESEND must ask for exactly this Entry", listOf(6L), resent)

        compose.onNodeWithTag(entryRecallTag(6)).assertIsEnabled()
        compose.onNodeWithTag(entryRecallTag(6)).performClick()
        assertEquals("a stranded capture has to stay recallable", listOf(6L), recalled)

        // Still amber, because a refusal is still owed: it leaves the queue's
        // head so nothing waits behind it, and it does not leave this phone.
        // The alert is the second line, not the field.
        assertTrue("a refused act is still owed", isTinted(fillOf(rowPixels(6))))
        Evidence.log("refused       = \"payload too large\", RESEND offered, Recall kept")
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

    // -- reading an Entry -----------------------------------------------------

    /**
     * A tap on the row opens the Entry whole, under it, and a second tap closes
     * it again.
     *
     * A row shows one flattened line of at most 80 characters, which is the
     * Preview and is all a phone's width affords. Three `ss://` URLs that
     * diverge at character 60 are then one row three times over until something
     * shows the rest (ADR 0003) — the desktop reads in a pane beside its list,
     * and a phone has no beside, so the reader is the row opened out.
     *
     * **The Preview line does not change when the panel opens.** It stays the
     * facade's own flattened line: the raw text is what the panel is for, and a
     * row that swapped one for the other would put an indented Entry's leading
     * whitespace back on the list, which is the blank-row bug pinned above.
     *
     * **And reading reaches nothing.** Reading an Entry is not a Use
     * (CONTEXT.md), so no verb fires and no Last Use moves. Asserted rather than
     * left to a reading of the screen: a reader that quietly bumped what it
     * opened would reorder the History on every paired device for somebody who
     * only looked.
     */
    @Test
    fun a_tap_on_a_row_opens_the_whole_entry_under_it_and_a_second_tap_closes_it() {
        show(
            UiState(
                session = SessionPhase.InContact("u"),
                entries = listOf(
                    entry(id = 1, preview = normalisedPreview, plaintext = indentedPlaintext),
                ),
            ),
        )

        // Closed, and the panel is not composed at all rather than composed at
        // no height — so this is a reading of the screen and not of a flag.
        compose.onNodeWithTag(entryTextTag(1)).assertDoesNotExist()

        compose.onNodeWithTag(entryReadTag(1)).performClick()
        compose.waitForIdle()
        compose.onNodeWithTag(entryTextTag(1)).assertIsDisplayed()
        compose.onNodeWithTag(entryTextTag(1)).assertTextEquals(indentedPlaintext)
        compose.onNodeWithTag(entryPreviewTag(1)).assertTextEquals(normalisedPreview)

        assertTrue(
            "reading an Entry is not a Use: it must recall, delete and resend nothing",
            recalled.isEmpty() && deleted.isEmpty() && resent.isEmpty(),
        )

        compose.onNodeWithTag(entryReadTag(1)).performClick()
        compose.waitForIdle()
        compose.onNodeWithTag(entryTextTag(1)).assertDoesNotExist()
        Evidence.log("read          = raw text under the row, the flattened line still on it")
    }

    /**
     * There is nothing to read on a row this phone holds no key for, and it does
     * not pretend otherwise.
     *
     * An Undecryptable Entry carries no plaintext at all — that missing
     * plaintext *is* the flag — so the row says so inline and offers no target
     * that could open onto it. A reader that opened an empty panel here would
     * say "this Entry holds no text" about ciphertext, which is a different fact
     * and the wrong one.
     */
    @Test
    fun an_undecryptable_row_offers_nothing_to_read() {
        show(
            UiState(
                session = SessionPhase.InContact("u"),
                entries = listOf(entry(id = 7, preview = "", plaintext = null, undecryptable = true)),
            ),
        )

        compose.onNodeWithTag(entryReadTag(7)).assertDoesNotExist()
        compose.onNodeWithTag(entryTextTag(7)).assertDoesNotExist()
        compose.onNodeWithTag(entryUndecryptableTag(7)).assertIsDisplayed()
        Evidence.log("sealed        = no read target and no panel; the row says why inline")
    }

    /**
     * An Entry whose text is genuinely the empty string says so when it is
     * opened, rather than opening onto nothing.
     *
     * It is decryptable and perfectly real, and a panel that drew zero
     * characters would read as a reader that had failed. The two states a blank
     * panel would conflate are the two this list refuses to conflate anywhere
     * else: an empty plaintext, and a missing key.
     */
    @Test
    fun an_entry_that_holds_no_text_says_so_when_it_is_opened() {
        show(
            UiState(
                session = SessionPhase.InContact("u"),
                entries = listOf(entry(id = 8, preview = "", plaintext = "", undecryptable = false)),
            ),
        )

        compose.onNodeWithTag(entryReadTag(8)).performClick()
        compose.waitForIdle()

        val empty = resources.getString(R.string.entry_read_empty)
        compose.onNodeWithTag(entryTextTag(8)).assertTextEquals(empty)
        compose.onNodeWithTag(entryUndecryptableTag(8)).assertDoesNotExist()
        Evidence.log("empty         = \"$empty\", and not the sealed row's sentence")
    }

    /**
     * A row that is open stays open when the History reorders under it.
     *
     * Rows are keyed by `entry.id` and `LazyColumn` keys its saveable state the
     * same way, so being open belongs to the Entry rather than to the slot it is
     * in. That is the whole reason it is state at all: a Use on another device
     * reorders this list under a reader's thumb, and a panel that closed because
     * its row moved would take away the text they were halfway through for a
     * reason they never saw.
     */
    @Test
    fun an_open_row_survives_the_history_reordering_under_it() {
        val entries = listOf(
            entry(id = 3, preview = "wg genkey | tee privatekey"),
            entry(id = 2, preview = normalisedPreview, plaintext = indentedPlaintext),
            entry(id = 1, preview = "the oldest one"),
        )
        show(UiState(session = SessionPhase.InContact("u"), entries = entries))

        compose.onNodeWithTag(entryReadTag(2)).performClick()
        compose.waitForIdle()
        compose.onNodeWithTag(entryTextTag(2)).assertTextEquals(indentedPlaintext)

        // Entry 1 was used somewhere else, so it leads the History now: the same
        // three Entries in a new order, with nobody touching the screen.
        show(UiState(session = SessionPhase.InContact("u"), entries = entries.sortedBy { it.id }))

        compose.onNodeWithTag(entryTextTag(2)).assertTextEquals(indentedPlaintext)
        Evidence.log("reorder       = the row somebody was reading is still open")
    }

    /**
     * A long press on an open Entry's text selects it.
     *
     * `RECALL` takes the Entry whole and was the only way to take any of it at
     * all, so a person who can see the host in the middle of a connection string
     * had been shown it rather than handed it. The desktop's pane has been
     * selectable since it shipped, because a webview is; this is the phone's half
     * of ADR 0003 doing the same.
     *
     * **Read off the screen and not off a flag**, because there is no flag to
     * read: the platform draws the highlight, and the menu it offers over it is a
     * window of the system's own that is in no semantics tree. So the assertion is
     * that the press *painted* something over the text — the emitter-coloured wash
     * [SharepasteTheme] resolves for a selection — and the menu itself was
     * verified by hand on a device, because it is the platform's and not ours.
     *
     * The press lands mid-word by construction: the panel renders whitespace
     * verbatim, and a long press on a blank line selects nothing anywhere in
     * Android. That is the platform's rule and not this screen's, so the fixture
     * puts text where the finger goes rather than the test asserting around it.
     *
     * **And selecting is still not a Use.** No phone runs a clipboard watcher
     * (ADR 0007), so nothing here may reach a verb: text taken out of this panel
     * lands on the clipboard, and only an `OFFER` after that is an act on the
     * Entry.
     */
    @Test
    fun the_text_of_an_open_entry_can_be_selected() {
        // Three lines with words across the middle of the middle one, which is
        // where `longClick` presses.
        val conf = "server {\n  listen 443;\n}"
        show(
            UiState(
                session = SessionPhase.InContact("u"),
                entries = listOf(entry(id = 1, preview = "server {", plaintext = conf)),
            ),
        )
        compose.onNodeWithTag(entryReadTag(1)).performClick()
        compose.waitForIdle()

        val unselected = compose.onNodeWithTag(entryTextTag(1)).captureToImage().toPixelMap()
        compose.onNodeWithTag(entryTextTag(1)).performTouchInput { longClick() }
        compose.waitForIdle()
        val selected = compose.onNodeWithTag(entryTextTag(1)).captureToImage().toPixelMap()

        val painted = repainted(unselected, selected)
        val area = unselected.width * unselected.height
        assertTrue(
            "a long press must paint a selection over the text, and painted $painted of $area pixels",
            painted > area / 100,
        )

        // Still open, still the same text: selecting is not closing.
        compose.onNodeWithTag(entryTextTag(1)).assertTextEquals(conf)
        assertTrue(
            "selecting in the reader is not a Use: it must recall, delete and resend nothing",
            recalled.isEmpty() && deleted.isEmpty() && resent.isEmpty(),
        )
        Evidence.log("select        = $painted of $area pixels washed by the selection, and no verb fired")
    }

    /**
     * The sentence about an Entry holding no text is the app's own words, and
     * they are not on offer.
     *
     * A selection over them offers to put a UI string on somebody's clipboard,
     * which is the one thing in this panel that is not somebody's content. The
     * container is around the Entry's text and not around the panel, and this is
     * that difference read back: the same press, and not a pixel moves.
     */
    @Test
    fun the_app_s_own_words_about_an_empty_entry_cannot_be_selected() {
        show(
            UiState(
                session = SessionPhase.InContact("u"),
                entries = listOf(entry(id = 8, preview = "", plaintext = "", undecryptable = false)),
            ),
        )
        compose.onNodeWithTag(entryReadTag(8)).performClick()
        compose.waitForIdle()

        val before = compose.onNodeWithTag(entryTextTag(8)).captureToImage().toPixelMap()
        compose.onNodeWithTag(entryTextTag(8)).performTouchInput { longClick() }
        compose.waitForIdle()
        val after = compose.onNodeWithTag(entryTextTag(8)).captureToImage().toPixelMap()

        assertEquals("the app's own sentence must not select", 0, repainted(before, after))
        Evidence.log("empty press   = a long press over the app's own sentence paints nothing")
    }

    /**
     * How many pixels the two captures disagree on.
     *
     * A selection is a colour and nothing else — it carries no semantics and no
     * node of its own — so the screen before and the screen after are the only
     * things there are to compare. The bounds are taken from the smaller capture
     * because a selection must not resize the text it is drawn over, and a test
     * that crashed instead of failing would say that badly.
     */
    private fun repainted(before: PixelMap, after: PixelMap): Int {
        var painted = 0
        for (x in 0 until minOf(before.width, after.width)) {
            for (y in 0 until minOf(before.height, after.height)) {
                if (before[x, y] != after[x, y]) painted += 1
            }
        }
        return painted
    }

    /**
     * A swipe that starts on the text of an open row still deletes the Entry.
     *
     * The gesture the reader introduces is a long press, and Delete's is a
     * horizontal drag on a row that now has a screenful of selectable text in
     * the middle of it — `swipeLeft` starts at this node's centre, which is
     * inside the panel. If selection answered a plain drag, the guard the swipe
     * *is* would be unreachable on exactly the rows a person spends time on.
     */
    @Test
    fun a_swipe_across_an_open_row_still_reaches_the_delete() {
        show(
            UiState(
                session = SessionPhase.InContact("u"),
                entries = listOf(
                    entry(id = 4, preview = normalisedPreview, plaintext = indentedPlaintext),
                ),
            ),
        )
        compose.onNodeWithTag(entryReadTag(4)).performClick()
        compose.waitForIdle()
        compose.onNodeWithTag(entryTextTag(4)).assertIsDisplayed()

        compose.onNodeWithTag(entryRowTag(4)).performTouchInput { swipeLeft() }
        compose.waitUntil(5_000) { deleted.isNotEmpty() }

        assertEquals("the swipe must still ask for exactly this Entry", listOf(4L), deleted)
        Evidence.log("open swipe    = a drag over the reader's own text still asks to delete 4")
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
     * A phone that was away opens at the head.
     *
     * The case ADR 0019 exists for, and the one this screen never used to
     * answer. Somebody left the list twenty rows down, put the phone in a
     * pocket, and five Entries landed while it was closed — the composition
     * survives `onStop`, so the **Place** survived with it and they came back to
     * new rows above them with nothing saying so. Nobody's place was worth
     * keeping at that moment: nothing was under their eyes, and whether anything
     * happened is what they opened the phone to find out.
     *
     * **Without the effect this fails outright**, and not marginally: nothing
     * else on the screen holds the `LazyListState`, so a wholesale replacement
     * leaves the list exactly where the scroll to index 20 put it, and Entry 45
     * sits at index 0 twenty-one rows above the viewport — where a `LazyColumn`
     * composes nothing, so `awaitRow` times out with no node to fetch rather
     * than fetching one that is off screen.
     */
    @Test
    fun a_phone_that_was_away_opens_at_the_head() {
        show(history(40L downTo 1L))
        scrollTo(readingIndex)
        compose.onNodeWithTag(entryRowTag(20)).assertIsDisplayed()
        compose.onNodeWithTag(entryRowTag(40)).assertDoesNotExist()

        // The open's first Catch-Up: five Entries at once, ingested under one
        // database guard and announced as one change. Nobody has touched the
        // list, so the gate still has its jump to spend.
        show(history(45L downTo 1L))
        jump()

        awaitRow(45)
        Evidence.log("open          = five Entries landed while the phone was away; the list is at the head")
    }

    /**
     * An Entry from **another Device** that arrives while somebody is reading does
     * not move them.
     *
     * The assertion this file used to make in reverse, and the half of ADR 0019
     * that costs something: a phone left open on the table is a phone whose list
     * is quietly stale until the next open or the next scroll up. No band and no
     * chip is bought to cover it — ADR 0002 charges rent for chrome that only
     * informs and ADR 0007 rules out a notification — because chasing the row
     * would cost a reader their place to show them something they can reach by
     * scrolling.
     *
     * The state grows and **nothing is emitted at all**. The mutant this kills is
     * any rule derived from the list — head id, head identity, length — every one
     * of which fires here. An Entry *this phone* captured is the other case and is
     * not this one: a Capture is a **Use**, so an Offer still follows its own row
     * to the head, which is what
     * [a_use_this_device_made_follows_the_row_to_the_head] covers. Which of the
     * two an `EntryAdded` is, is the state holder's to decide from the Origin.
     */
    @Test
    fun an_entry_arriving_mid_session_leaves_the_reader_alone() {
        show(history(40L downTo 1L))
        scrollTo(readingIndex)
        compose.onNodeWithTag(entryRowTag(20)).assertIsDisplayed()
        compose.onNodeWithTag(entryRowTag(40)).assertDoesNotExist()

        // The Entry lands over the live stream, twenty-one rows above the reader.
        show(history(41L downTo 1L))

        compose.onNodeWithTag(entryRowTag(20)).assertIsDisplayed()
        compose.onNodeWithTag(entryRowTag(41)).assertDoesNotExist()

        // And the list can still move, so the two assertions above are about the
        // rule and not about an effect that does nothing.
        jump()
        awaitRow(41)
        Evidence.log("mid-session   = Entry 41 landed above the reader and Entry 20 stayed under them")
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
     * had, so nothing is emitted and nothing moves — mid-session and at an open
     * alike, where it is the Catch-Up rather than the cause that is answered.
     * **The mutant this kills** is any rule derived from the list itself — head
     * id, head identity, list length — because every one of them fires here.
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
        // this reorder and not about an effect that does nothing: the same row
        // at the head by a Use *this* phone made is followed there.
        follow()
        awaitRow(7)
        Evidence.log("remote use    = the head changed, Entry 20 stayed; our own Use still scrolls")
    }

    /**
     * A **Use this device made** follows the row.
     *
     * The other half of the pair, and the reason the motion is carried rather
     * than derived: the core cannot say whose Use it was — `HistoryChanged`
     * carries only a `user_id` — but the state holder can, because `recall` knows
     * the Entry it just passed.
     *
     * It is also the one motion that outlives the open (ADR 0019), and the one
     * that still animates: the person did something, a row moved because of it,
     * and the animation is what says those are the same fact.
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
        follow()

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

        // And the list can still move, so this is a reading of the rule and not
        // of an effect that does nothing.
        follow()
        awaitRow(39)
        Evidence.log("deletion      = the head left, Entry 20 stayed; our own Use still scrolls")
    }

    /**
     * Switching the Viewed Pairing gives up the Place.
     *
     * It replaces every Entry at once, so the head changes for a reason that
     * shares no id with what was there before, and the row somebody was reading
     * is not in the list any more. The `LazyColumn` keeps its index because that
     * is all it has to go on, which used to leave the new Pairing opening roughly
     * where the last one was left — a place in one History applied to another.
     * ADR 0019 stopped conceding that: the cause is named at the source, like
     * every other cause this screen acts on, and it jumps.
     *
     * A jump and not a follow, for the same reason the open is one: nothing
     * changed under anybody's eyes, because what was under them is gone.
     */
    @Test
    fun switching_the_viewed_pairing_gives_up_the_place() {
        show(history(40L downTo 1L))
        scrollTo(readingIndex)
        compose.onNodeWithTag(entryRowTag(20)).assertIsDisplayed()

        // A different Pairing's Entries, sharing no id with the ones on screen.
        show(history(140L downTo 101L))
        jump()

        awaitRow(140)
        compose.onNodeWithTag(entryRowTag(120)).assertDoesNotExist()
        Evidence.log("switch        = a wholly new list opened at its own head")
    }

    /**
     * A drag on the list is reported, and the screen's own scrolling is not.
     *
     * Both halves in one case because the pair is the decision. A hand is what
     * spends the open's jump — there is no clock in the gate, because ADR 0007
     * makes an open with no signal the nominal case and a late Catch-Up must
     * still reach somebody who has not touched anything. So the report has to
     * fire for a finger and must **not** fire for this screen's own motion:
     * `LazyListState.isScrollInProgress` is true during both, and reading it
     * would have let one programmatic scroll close the gate on every open that
     * followed. A drag interaction is the honest reading, and a fling is covered
     * because a fling can only follow a drag.
     */
    @Test
    fun a_finger_on_the_list_is_a_hand_and_the_screens_own_motion_is_not() {
        show(history(40L downTo 1L))

        // Semantics-driven, which is how every other case here moves the list:
        // it reaches `scrollToItem` without a pointer anywhere near the screen.
        scrollTo(readingIndex)
        follow()
        awaitRow(40)
        assertEquals(
            "the screen's own scrolling is not a hand. Reported as one, the first jump of a " +
                "foreground would close the gate on the Catch-Up it was armed for",
            0,
            hands,
        )

        compose.onNodeWithTag(TAG_HISTORY_LIST).performTouchInput { swipeUp() }
        compose.waitUntil(5_000) { hands > 0 }
        Evidence.log("a hand        = a drag reported one; a semantics scroll and a follow reported none")
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
     * The `✕` ends where the buttons on the rows end.
     *
     * The Filter band is full-bleed and pays its own insets, which is the only
     * arrangement in which this can be true: a band carrying the screen's
     * gutter would set its control a gutter further in than the column of row
     * buttons directly beneath it, and a control out of line with the column it
     * sits on top of is the first thing an eye finds here.
     *
     * Asserted rather than eyeballed because the two numbers live in different
     * files and neither one is wrong on its own. They agree today because both
     * read [Fui.RowInset]; this fails the moment one of them stops.
     */
    @Test
    fun the_clear_control_ends_where_the_rows_own_buttons_end() {
        show(filtering("ssh", threeEntries))

        val clear = compose.onNodeWithTag(TAG_FILTER_CLEAR).getUnclippedBoundsInRoot().right
        val recall = compose.onNodeWithTag(entryRecallTag(3)).getUnclippedBoundsInRoot().right

        assertEquals(
            "the ✕ and the first row's Recall have to end on one line",
            recall.value,
            clear.value,
            0.5f,
        )
        Evidence.log("right edge    = ✕ and RECALL both end at ${clear.value.toInt()}dp from the left")
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
     * The Filter finds an un-flushed capture by its own text.
     *
     * The defect this whole effect is named for. The payload of a pending
     * capture is an Entry from the moment it is made, so the row carries
     * plaintext and the one predicate matches it — nothing about the queue is
     * special-cased here, which is the point. An outbox beside the History
     * would hold ciphertext, and a needle would have nothing to be in.
     *
     * It is in the denominator too, because this phone holds it.
     */
    @Test
    fun the_filter_finds_an_un_flushed_capture_by_its_text() {
        val pending = waiting(4, "wg genkey | tee privatekey")
        show(filtering("genkey", listOf(pending) + threeEntries))

        compose.onNodeWithTag(entryRowTag(4)).assertIsDisplayed()
        compose.onNodeWithTag(entryPreviewTag(4)).assertTextEquals("wg genkey | tee privatekey")
        compose.onNodeWithTag(TAG_FILTER_COUNT).assertTextEquals("1/4")
        assertTrue("and it is drawn as owed while it is found", isTinted(fillOf(rowPixels(4))))
        Evidence.log("filter+queue  = \"genkey\" found the un-flushed capture, still tinted")
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
     * The `✕` is there exactly while there is a needle to clear, and the empty
     * band says what it is for.
     *
     * Chrome that is always present and usually inert is chrome a thumb learns
     * to ignore. The count is not conditional in the same way: a phone with
     * Entries always says how many, which is where the hundred-row boundary is
     * legible before anybody reaches it.
     *
     * The placeholder is the only label this field has — it has no room for the
     * floating one a Material field would charge it for — so an empty band that
     * has lost it is a band nobody can tell is typeable. It is drawn under the
     * field rather than beside it, which is a thing that renders identically
     * right up until the caret lands in the wrong place.
     */
    @Test
    fun the_clear_control_appears_only_with_something_to_clear() {
        show(filtering("", threeEntries))
        compose.onNodeWithTag(TAG_FILTER_CLEAR).assertDoesNotExist()
        compose.onNodeWithTag(TAG_FILTER_COUNT).assertTextEquals("3/3")
        compose.onNodeWithText(resources.getString(R.string.filter_placeholder)).assertIsDisplayed()

        show(filtering("ssh", threeEntries))
        compose.onNodeWithText(resources.getString(R.string.filter_placeholder)).assertDoesNotExist()
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
