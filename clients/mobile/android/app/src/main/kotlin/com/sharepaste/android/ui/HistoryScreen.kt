package com.sharepaste.android.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.sharepaste.android.R
import com.sharepaste.core.Entry

/**
 * The Entries of the Viewed Pairing, and the two verbs that need no row.
 *
 * The list is the whole point of the client: something copied on the computer
 * shows up as an Entry, and a Recall puts it back on this phone's clipboard.
 *
 * **Everything above the list is chrome and cannot scroll away.** That is the
 * one structural change the redesign made here, and it is worth stating because
 * the previous arrangement looked identical until you scrolled: the Contact
 * readout and the foreground-only note were the first two items *in* the list,
 * so the two facts a puzzled person most needs were the first two to leave the
 * screen. Now the identity, Contact and the background policy are three fixed
 * bands — 112dp of them — and the eleventh row peeks under the verb bar instead.
 * The third of them is the only one a person can be rid of, and only by
 * acknowledging it: see [ForegroundOnlyNote].
 *
 * Contact is permanent rather than degraded-only, inverting the desktop's rule
 * (ADR 0002) rather than copying it: a phone is out of contact almost always, so
 * a band that appeared only when disconnected would be a band that was always
 * there and always looked like bad news. See [ContactReadout].
 *
 * Takes a whole [UiState] and an [AppActions] rather than a widening list of
 * pieces: no composable here sees the state holder, the repository or the core,
 * which is what lets every sentence on this screen be asserted with no facade
 * behind it.
 */
@Composable
fun HistoryScreen(state: UiState, actions: AppActions, modifier: Modifier = Modifier) {
    val rows = rememberLazyListState()
    NewestEntryStaysInView(entries = state.entries, rows = rows)
    Column(
        modifier = modifier
            .fillMaxSize()
            .background(Fui.Panel)
            .fuiBackdrop()
            .testTag(TAG_HISTORY_SCREEN),
    ) {
        IdentityBand(state, actions)
        // Permanent, in every phase. A revoked token is the one that grows a
        // sentence and a way out of it.
        ContactReadout(state.session, onPairAgain = actions.openAddPairing)
        // The one band here a person can be rid of, and only by acknowledging
        // it. Gone entirely rather than drawn empty, and nothing is lost with
        // it: the sentence keeps its full length on the Settings Screen, so
        // what leaves this screen is the reminder and not the disclosure.
        if (!state.foregroundNoteDismissed) ForegroundOnlyNote(actions.dismissForegroundNote)
        // Beside the background policy, because they are the same kind of fact:
        // what this app will and will not do while you are not looking at it.
        // Only when the platform is actually refusing — a phone whose
        // notifications work says nothing.
        if (state.standingActionsBlocked) {
            StandingActionsBlockedNote(actions.enableStandingActions)
        }
        // What just happened is not an Entry, and a notice that scrolled away
        // with the rows would be a notice the person never read.
        state.notice?.let { NoticeBanner(it, actions.dismissNotice) }
        // The same reasoning, more urgently. These rows belong to a Pairing this
        // phone is not syncing, so nothing here is being kept up to date — and a
        // frozen list looks exactly like a current one.
        if (state.diverged) {
            DivergenceBand(
                viewedName = state.nameOf(state.viewedPairing),
                activeName = state.nameOf(state.activeUserId),
                onUseViewed = { state.viewedPairing?.let(actions.activatePairing) },
            )
        }
        if (state.pending > 0) PendingReadout(state.pending)

        LazyColumn(
            state = rows,
            modifier = Modifier.weight(1f).testTag(TAG_HISTORY_LIST),
        ) {
            if (state.entries.isEmpty()) {
                item { EmptyHistory() }
            } else {
                entryRows(state.entries, state.ownDeviceId, actions.recall, actions.deleteEntry)
            }
        }
        VerbBar(actions)
    }
}

/**
 * The newest Entry is where it can be seen, the moment it arrives.
 *
 * Entries are prepended, so the newest is index 0 and the destination never
 * changes — but the distance to it does. Everything above the list is chrome
 * and four of those bands come and go: a pending queue, a divergence, a blocked
 * notification and a notice can each be there or not, and each one pushes the
 * row `RECALL LATEST` will hand over further under the top of the viewport.
 * Retiring the two confirmation banners recovered most of that height and none
 * of the guarantee, which is why this exists as well and not instead.
 *
 * **An arrival is a new head with the old head still under it**, which is a
 * narrower question than either "the head changed" or "the list grew" and is the
 * only one that means what this is for. Deleting the newest row changes the head
 * too, and so does switching the Viewed Pairing; scrolling somebody to the top
 * because they removed a row, or because they are now looking at a list they
 * have not read a word of, is not a courtesy. Both of those lose the old head,
 * and a prepend is exactly the case that keeps it.
 *
 * [seen] is what makes the comparison possible across recompositions. It is
 * `rememberSaveable`, so a rotation restores it beside the [LazyListState]'s own
 * restored offset, and the effect that necessarily re-runs against the new
 * composition finds the head already accounted for and leaves the person where
 * they were reading. The first population jumps rather than animates: an empty
 * list is already at index 0, and an animation from nowhere to nowhere is a
 * movement nobody asked for.
 */
@Composable
private fun NewestEntryStaysInView(entries: List<Entry>, rows: LazyListState) {
    val newest = entries.firstOrNull()?.id
    var seen by rememberSaveable { mutableStateOf<Long?>(null) }
    LaunchedEffect(newest) {
        if (newest != null && newest != seen) {
            if (seen == null) {
                rows.scrollToItem(0)
            } else if (entries.any { it.id == seen }) {
                rows.animateScrollToItem(0)
            }
        }
        seen = newest
    }
}

/**
 * Who this phone is, and the only door off this screen.
 *
 * The identity is the **Viewed** Pairing rather than the Active one, because it
 * heads the list underneath it and the list is the Viewed Pairing's. When the two
 * differ the badge says so here as well as in [DivergenceBand] — the band
 * explains, this states, and a person who has scrolled the band out of a long
 * History still has the fact in front of them.
 *
 * The User slot holds the **username or nothing at all**, never the `user_id`.
 * Until the Relay's `/me` mirror answers there is no name, and the id is a
 * 36-character uuid: it fills the line, pushes the Relay host off the end of it
 * and tells the person nothing they could act on or recognise. An ellipsis is
 * one glyph wide and says the true thing, which is that the name is not known
 * yet. [UiState.nameOf] still falls back to the id and that is not an
 * inconsistency — it names a Pairing inside a sentence, where an ellipsis would
 * name nothing.
 */
@Composable
private fun IdentityBand(state: UiState, actions: AppActions) {
    val viewed = state.pairings.firstOrNull { it.userId == state.viewedPairing }
    Column(Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(52.dp)
                .background(Brush.verticalGradient(listOf(Fui.CyanA08, Color.Transparent)))
                .padding(start = Fui.Gutter, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Column(Modifier.weight(1f)) {
                Text(
                    text = stringResource(R.string.history_title),
                    style = Fui.Micro,
                    color = Fui.TextEmitter,
                )
                if (viewed != null) {
                    Text(
                        text = stringResource(
                            R.string.history_identity,
                            viewed.username ?: stringResource(R.string.history_identity_unknown),
                            viewed.relayHost,
                        ),
                        style = Fui.Data,
                        color = Fui.TextBody,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.testTag(TAG_IDENTITY),
                    )
                }
            }
            if (state.diverged) {
                FuiBadge(stringResource(R.string.pairings_viewed_badge), Accent.Neutral)
            }
            // The only way to the Pairings, and it belongs here rather than
            // behind a drawer: on a phone that holds one Pairing it is a door to
            // the settings, and on a phone that holds several it is the only
            // place the other ones exist at all.
            GlyphButton(
                glyph = "◎",
                onClick = actions.openPairings,
                contentDescription = stringResource(R.string.pairings_open),
                modifier = Modifier.testTag(TAG_OPEN_PAIRINGS),
            )
        }
        Hairline()
    }
}

@Composable
private fun EmptyHistory() {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(24.dp)
            .testTag(TAG_HISTORY_EMPTY),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(
            text = stringResource(R.string.history_empty_heading),
            style = Fui.Heading,
            color = Fui.TextPrimary,
        )
        Text(
            text = stringResource(R.string.history_empty_body),
            style = Fui.Prose,
            color = Fui.TextBody,
            textAlign = TextAlign.Center,
        )
    }
}

/**
 * The rows.
 *
 * One item per Entry, and the surrounding screen never has to change. Three
 * things each row has to get right, and each of them is a mistake the desktop
 * made first:
 *
 *  * The **Preview** is rendered as it arrives. [Entry.preview] is the Preview
 *    on every path the core produces an Entry on — one line, control characters
 *    already spaces, trimmed and capped — so this row neither re-derives it nor
 *    reads [Entry.plaintext], which is the raw text and would render an indented
 *    Entry as a blank line.
 *  * **Undecryptable** comes from [Entry.undecryptable] and from nowhere else.
 *    Not from an empty Preview: an Entry whose plaintext is genuinely empty is
 *    indistinguishable from ciphertext this device has no key for to anything
 *    guessing, and the desktop guessed in four places.
 *  * **Origin** is shown only when the Entry came from somewhere else. It is "the
 *    device an Entry was captured on, as distinct from the device viewing it",
 *    so on the rows this phone produced there is nothing to distinguish.
 */
private fun LazyListScope.entryRows(
    entries: List<Entry>,
    ownDeviceId: String?,
    onRecall: (Entry) -> Unit,
    onDelete: (Entry) -> Unit,
) {
    itemsIndexed(entries, key = { _, entry -> entry.id }) { index, entry ->
        if (entry.undecryptable) {
            UndecryptableRow(entry, onDelete)
        } else {
            // The newest Entry is the one `RECALL LATEST` will hand over, so it
            // is the one row drawn as the emitter's: the verb bar's target,
            // named in the list rather than only in the bar.
            EntryRow(entry, ownDeviceId, newest = index == 0, onRecall, onDelete)
        }
    }
}

/**
 * One readable Entry: a single tap target, and a Delete that has to be dragged
 * for.
 *
 * **The row used to carry two word-buttons.** On a full screen that is twenty
 * targets, with the destructive one a thumb's width from the safe one and
 * nothing between them — and a Delete fans out over SSE to every paired device
 * and cannot be undone, while the desktop's `✕` was never guarded either. So
 * Recall keeps its 48dp square on every row and Delete is behind a swipe, which
 * is the guard neither client had.
 *
 * The swipe panel carries [entryDeleteTag] because it *is* this row's delete —
 * exactly one node per row wears that tag, here or on the button an
 * [UndecryptableRow] draws instead.
 */
@Composable
private fun EntryRow(
    entry: Entry,
    ownDeviceId: String?,
    newest: Boolean,
    onRecall: (Entry) -> Unit,
    onDelete: (Entry) -> Unit,
) {
    val swipe = rememberSwipeToDismissBoxState()
    // The list is the source of truth about which Entries exist. The swipe asks
    // for the delete and then springs back; the row leaves when the facade says
    // it left, so a delete the Relay refuses does not hide a row that is still
    // there.
    LaunchedEffect(swipe.currentValue) {
        if (swipe.currentValue == SwipeToDismissBoxValue.EndToStart) swipe.reset()
    }
    SwipeToDismissBox(
        state = swipe,
        enableDismissFromStartToEnd = false,
        onDismiss = { onDelete(entry) },
        backgroundContent = {
            DeleteBehindTheRow(
                entry = entry,
                // A control exactly while it is uncovered, and the reason is
                // hit-testing rather than taste. `SwipeToDismissBox` composes
                // this panel under the row on every frame, and an opaque
                // background is not a pointer target: a tap landing where the
                // row holds no button falls straight through to whatever is
                // behind it. A panel that were clickable at rest would put an
                // undoable Delete under most of every row — the two targets a
                // thumb apart that the swipe was introduced to remove, only now
                // one of them is invisible.
                uncovered = swipe.dismissDirection == SwipeToDismissBoxValue.EndToStart,
                onDelete = onDelete,
            )
        },
        modifier = Modifier.testTag(entryRowTag(entry.id)),
    ) {
        // Two layers, not one: the emitter's tint is 12% alpha and the delete
        // panel is sitting right behind it, so a single translucent background
        // paints the newest row red.
        Column(
            Modifier
                .background(Fui.Panel)
                .then(if (newest) Modifier.background(Fui.Active) else Modifier),
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(Fui.RowHeight)
                    .padding(end = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                // The emitter's own rule, on the one row Recall Latest will hand
                // over. Every other row is flush with the gutter.
                if (newest) {
                    Box(Modifier.width(2.dp).fillMaxHeight().background(Fui.Cyan400))
                }
                Column(
                    modifier = Modifier.weight(1f).padding(start = if (newest) 12.dp else Fui.Gutter),
                    verticalArrangement = Arrangement.spacedBy(5.dp),
                ) {
                    Text(
                        // The facade's Preview, verbatim — see `entryRows`.
                        text = entry.preview,
                        style = Fui.Data,
                        color = if (newest) Fui.TextPrimary else Fui.TextBody,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.testTag(entryPreviewTag(entry.id)),
                    )
                    if (entry.deviceId != ownDeviceId) {
                        Text(
                            // Resolved by the core: the Device Label, or a slice
                            // of the Device id when the mirror has none.
                            text = stringResource(R.string.entry_origin, entry.originLabel),
                            style = Fui.Micro,
                            color = Fui.TextMuted,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.testTag(entryOriginTag(entry.id)),
                        )
                    }
                }
                if (newest) {
                    // Named, filled and wider, because this is the row the verb
                    // bar acts on and a person should be able to see which one
                    // that is before they press it.
                    FuiButton(
                        text = stringResource(R.string.entry_recall),
                        onClick = { onRecall(entry) },
                        solid = true,
                        modifier = Modifier.width(96.dp).testTag(entryRecallTag(entry.id)),
                    )
                } else {
                    GlyphButton(
                        glyph = "↓",
                        onClick = { onRecall(entry) },
                        contentDescription = stringResource(R.string.entry_recall),
                        modifier = Modifier.testTag(entryRecallTag(entry.id)),
                    )
                }
            }
            Hairline(color = Fui.CyanA08)
        }
    }
}

/**
 * What the swipe uncovers, and what it arms.
 *
 * The panel *is* the delete rather than a picture of one: the drag exposes it
 * and a press on it fires. That second half was missing, and its absence was
 * the worst version of the guard — a `✕ DELETE` shown to somebody who has just
 * discovered the gesture, which then answers a press with nothing. A control
 * that appears and does not work does not teach the gesture, it teaches that
 * the row is broken.
 *
 * A completed swipe still deletes on its own, through the box's `onDismiss`,
 * and both routes call the same [onDelete]. So dragging all the way and
 * dragging then tapping are one outcome rather than two behaviours to learn —
 * and [uncovered] is what keeps the guard, because a Delete that is only a
 * control while a finger is holding it open cannot be reached by accident.
 */
@Composable
private fun DeleteBehindTheRow(entry: Entry, uncovered: Boolean, onDelete: (Entry) -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxSize()
            .background(Fui.Alert500),
        horizontalArrangement = Arrangement.End,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(
            modifier = Modifier
                .width(96.dp)
                .fillMaxHeight()
                .clickable(enabled = uncovered, role = Role.Button) { onDelete(entry) }
                .testTag(entryDeleteTag(entry.id)),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            // The glyph is a picture of the verb and the word under it is the
            // verb's name, so only one of the two is worth reading out. Same
            // correction, same reason, as [GlyphButton].
            Text(
                text = "✕",
                style = Fui.Glyph,
                color = Fui.OnEmitter,
                modifier = Modifier.clearAndSetSemantics {},
            )
            Text(stringResource(R.string.entry_delete), style = Fui.Micro, color = Fui.OnEmitter)
        }
    }
}

/**
 * Ciphertext this phone holds no key for: named, not blanked, and deletable.
 *
 * It keeps both controls inline rather than the swipe, and that asymmetry is the
 * point. Recall is **disabled rather than hidden**, following the desktop's
 * detail pane and not its row — the control someone is looking for has to still
 * be where they are looking, saying no, with the marker beside it as the reason.
 * And Delete is the one thing a person can actually do with the row, so it is
 * not put behind a gesture they would have to discover.
 */
@Composable
private fun UndecryptableRow(entry: Entry, onDelete: (Entry) -> Unit) {
    Column(Modifier.testTag(entryRowTag(entry.id))) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(Fui.RowHeight)
                .background(Fui.AlertA16)
                .padding(end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Box(Modifier.width(2.dp).fillMaxHeight().background(Fui.Alert400))
            Column(
                modifier = Modifier.weight(1f).padding(start = 12.dp),
                verticalArrangement = Arrangement.spacedBy(5.dp),
            ) {
                Text(
                    text = stringResource(R.string.entry_undecryptable_marker),
                    style = Fui.Label,
                    color = Fui.Alert400,
                    maxLines = 1,
                )
                Text(
                    text = stringResource(R.string.entry_undecryptable),
                    style = Fui.Micro,
                    color = Fui.TextBody,
                    maxLines = 2,
                    modifier = Modifier.testTag(entryUndecryptableTag(entry.id)),
                )
            }
            GlyphButton(
                glyph = "↓",
                onClick = {},
                contentDescription = stringResource(R.string.entry_recall),
                enabled = false,
                modifier = Modifier.testTag(entryRecallTag(entry.id)),
            )
            GlyphButton(
                glyph = "✕",
                onClick = { onDelete(entry) },
                contentDescription = stringResource(R.string.entry_delete),
                accent = Accent.Alert,
                modifier = Modifier.testTag(entryDeleteTag(entry.id)),
            )
        }
        Hairline(color = Fui.CyanA08)
    }
}

/**
 * Offer and Recall Latest, the two verbs that need no row selected.
 *
 * Deliberately **not** called Standing Actions: those are the verbs a device
 * exposes *without being opened*, and these are buttons on an open screen. They
 * call the same two repository entry points the Standing Actions do, which is
 * the point — neither of those entry points assumes a composition exists.
 *
 * **Recall Latest is the solid one and comes first.** Recall is why a phone is
 * opened at all — the laptop copied something and the phone has to paste it —
 * and it is the one verb that fetches rather than trusting the cache, so it is
 * the one that must never hand over something stale. Two equal outlines were
 * truthful about their symmetry in the code and mute about which one a person
 * reaches for. Offer keeps a full-height target beside it, in the order the
 * notification lists them.
 */
@Composable
private fun VerbBar(actions: AppActions) {
    Column(Modifier.fillMaxWidth()) {
        Hairline(color = Fui.Frame)
        Row(
            modifier = Modifier.fillMaxWidth().background(Fui.Band).padding(Fui.Gutter),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            FuiButton(
                text = stringResource(R.string.recall_latest_bar),
                onClick = actions.recallLatest,
                solid = true,
                modifier = Modifier.weight(1.6f).testTag(TAG_RECALL_LATEST),
            )
            FuiButton(
                text = stringResource(R.string.offer_bar),
                onClick = actions.offerClipboard,
                modifier = Modifier.weight(1f).testTag(TAG_OFFER),
            )
        }
    }
}

/**
 * How many Entries are still waiting for the Relay.
 *
 * Shown only when there are some, and shown at all because sync is
 * foreground-only: an Offer made with no connection sits in the queue until the
 * app is next opened, and a queue nobody can see is a queue nobody comes back
 * for. It disappears of its own accord when the uploader drains it.
 *
 * The count is a readout beside the sentence rather than a number inside it,
 * because the number is what is being reported and the sentence is what it
 * means. [TAG_PENDING] stays on the sentence.
 */
@Composable
private fun PendingReadout(pending: Long) {
    Column(Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier.fillMaxWidth().background(Fui.AmberA16).padding(Fui.Gutter, 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Text(
                text = pending.toString(),
                style = Fui.Readout,
                color = Fui.Amber400,
                modifier = Modifier.testTag(TAG_PENDING_COUNT),
            )
            Text(
                text = pluralStringResource(R.plurals.pending_count, pending.toInt()),
                style = Fui.Prose,
                color = Fui.TextBody,
                modifier = Modifier.weight(1f).testTag(TAG_PENDING),
            )
        }
        Hairline(color = Fui.AmberA40)
    }
}

const val TAG_HISTORY_SCREEN = "history-screen"

/**
 * The scrolling list itself.
 *
 * A `LazyColumn` composes only what is on screen, so a test that wants a row has
 * to scroll to it first, and without a handle on the list there is nothing to ask.
 */
const val TAG_HISTORY_LIST = "history-list"

const val TAG_HISTORY_EMPTY = "history-empty"

/** Which Pairing's History is on screen: `<user> @ <relay host>`. */
const val TAG_IDENTITY = "history-identity"

const val TAG_OFFER = "offer-clipboard"
const val TAG_RECALL_LATEST = "recall-latest"
const val TAG_OPEN_PAIRINGS = "open-pairings"

/** The sentence that says what a queue is. */
const val TAG_PENDING = "pending-count"

/** The queue depth itself, as a readout. */
const val TAG_PENDING_COUNT = "pending-readout"

fun entryRowTag(id: Long) = "entry-$id"
fun entryPreviewTag(id: Long) = "entry-preview-$id"
fun entryOriginTag(id: Long) = "entry-origin-$id"
fun entryUndecryptableTag(id: Long) = "entry-undecryptable-$id"
fun entryRecallTag(id: Long) = "entry-recall-$id"

/**
 * This row's Delete, wherever it is.
 *
 * Exactly one node per row: the panel a swipe uncovers on a readable Entry, or
 * the `✕` an [UndecryptableRow] draws inline because deleting is the only thing
 * a person can do with it.
 */
fun entryDeleteTag(id: Long) = "entry-delete-$id"
