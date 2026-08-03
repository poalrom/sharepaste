package com.sharepaste.android.ui

import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.VisibilityThreshold
import androidx.compose.animation.core.spring
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
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.sharepaste.android.R
import com.sharepaste.core.Entry
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow

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
 * screen. Now the identity, Contact, the Filter and the background policy are
 * four fixed bands — 168dp of them — and the rows peek under the verb bar
 * instead. The last of them is the only one a person can be rid of, and only by
 * acknowledging it: see [ForegroundOnlyNote]; 138dp is what is left once they
 * have.
 *
 * The Filter sits third, directly under [ContactReadout] and above every
 * transient band, because a control has to be in the same place every time it
 * is reached for. Below the four bands that come and go its Y would swing by
 * some 200dp depending on the news.
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
fun HistoryScreen(
    state: UiState,
    actions: AppActions,
    modifier: Modifier = Modifier,
    headMoves: Flow<Long> = emptyFlow(),
) {
    val rows = rememberLazyListState()
    TheNewHeadStaysInView(headMoves, rows)
    // A reorder is legible only if it moves; a re-filter is legible only if it
    // does not. Both re-lay-out the same keyed list, so the difference has to be
    // stated: `shown` answering a different needle than it did last frame is a
    // re-layout the person caused by typing, and a hundred rows sliding to new
    // slots per character is a mess. Anything else is a Use, and a row that
    // teleports while the viewport holds still is a move nobody was shown.
    var answered by remember { mutableStateOf(state.shown.needle) }
    val settled = answered == state.shown.needle
    SideEffect { answered = state.shown.needle }
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
        FilterBand(state, actions.setFilter)
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
            // Three states in this order, and the order is the decision. An
            // empty History is a fact about the Pairing and outranks anything a
            // needle has to say about it: `NO MATCHES` on a phone holding no
            // Entries at all would send somebody looking for a query to fix.
            when {
                state.entries.isEmpty() -> item { EmptyHistory() }
                state.shown.entries.isEmpty() -> item { NoMatches(state.entries.size >= CACHE_CAP) }
                else -> entryRows(
                    entries = state.shown.entries,
                    ownDeviceId = state.ownDeviceId,
                    animatePlacement = settled,
                    onRecall = actions.recall,
                    onDelete = actions.deleteEntry,
                )
            }
        }
        VerbBar(state.shown.entries.firstOrNull(), actions)
    }
}

/**
 * The row that has just taken the head is where it can be seen.
 *
 * Entries lead the History by **Last Use**, so index 0 is the destination and
 * the destination never changes — but the distance to it does. Everything above
 * the list is chrome and four of those bands come and go: a pending queue, a
 * divergence, a blocked notification and a notice can each be there or not, and
 * each one pushes the row `RECALL FIRST` will hand over further under the top
 * of the viewport.
 *
 * **The question is not "did the head change".** It used to be, by proxy: under
 * capture ordering only an arrival could put a new row on top, so "new head,
 * old head still under it" meant "something arrived". Last Use broke the proxy
 * rather than the rule — a Use changes the head too, from this device and from
 * any other, and neither is an arrival. So the two cases are named at the
 * source instead, and [SharepasteViewModel.headMoves] carries exactly them: a
 * new Entry on the Viewed Pairing, and a Use this device made. A **remote** Use
 * raises nothing, because nothing new exists and chasing it would cost the
 * reader their place to show them a row they already had.
 *
 * `animateScrollToItem(0)` *is* the "has it left the viewport" check: it does
 * nothing when the top is already in view and follows the row when it is not.
 * No viewport arithmetic, and the two cases the old rule existed to exclude —
 * deleting the newest row, switching the Viewed Pairing — stay excluded for
 * free, because neither is an arrival or a Use.
 */
@Composable
private fun TheNewHeadStaysInView(headMoves: Flow<Long>, rows: LazyListState) {
    LaunchedEffect(headMoves, rows) {
        headMoves.collect { rows.animateScrollToItem(0) }
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
                .padding(start = Fui.Gutter, end = Fui.RowInset),
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

/**
 * The Filter: the History on screen narrowed to what was typed.
 *
 * It never asks the Relay. It hides rows this phone already holds, which is why
 * it is `FILTER HISTORY` and not `SEARCH` — somebody who searched and found
 * nothing would reasonably conclude the Relay had been asked and had nothing.
 *
 * **Everything here reads [UiState.shown] and nothing derives a second opinion.**
 * The rows, the `n/m` and the `NO MATCHES` branch all come from one value that
 * carries the needle it answers, because the scan runs off the main thread and
 * a count one keystroke ahead of the list it counts is the failure that costs.
 * The denominator is [UiState.entries], which is the whole History: `3/100` is
 * "three of the hundred this phone holds", not three of some other filtered
 * number.
 *
 * The `✕` appears only with something to clear, so the band is inert chrome
 * until it is used. The count is not conditional in the same way — a phone with
 * Entries always says how many, which is where the hundred-row boundary is
 * legible before anybody hits it.
 *
 * **The band is the field, edge to edge, and draws no outline in any state.**
 * Material's outlined field is a box you put on a form beside other boxes; this
 * is the one thing on its band, so a frame around it only says "not the part
 * either side of me" about two strips of nothing. Focus has the caret to say it
 * with. That leaves [BasicTextField] and a decoration this file owns, which is
 * also what buys the exact insets: the glyph on the screen's gutter like every
 * heading, and the `✕` on [Fui.RowInset] like every button on the rows below.
 */
@Composable
private fun FilterBand(state: UiState, onFilter: (String) -> Unit) {
    // Keyed off `shown.needle` rather than off the field: what is announced has
    // to be a count that exists, and during a scan the field is a keystroke
    // ahead of the answer.
    val filtering = state.shown.needle.isNotEmpty()
    val named = stringResource(R.string.filter_field)
    ChromeBand(height = FilterHeight, background = Fui.Recess, gutter = 0.dp) {
        Text(
            text = "⌕",
            style = Fui.Glyph,
            color = Fui.TextDim,
            // Decorative, as every glyph in this app that is not itself a
            // control is: the field beside it already has a name.
            modifier = Modifier.padding(start = Fui.Gutter, end = 10.dp).clearAndSetSemantics {},
        )
        BasicTextField(
            value = state.filter,
            onValueChange = onFilter,
            singleLine = true,
            textStyle = Fui.Data.copy(color = Fui.TextPrimary),
            cursorBrush = SolidColor(Fui.Cyan400),
            keyboardOptions = KeyboardOptions(
                // A needle is matched case-insensitively against text somebody
                // else copied, so a capital first letter and a corrected word
                // are both the keyboard answering a question nobody asked.
                capitalization = KeyboardCapitalization.None,
                autoCorrectEnabled = false,
            ),
            decorationBox = { field ->
                // Stacked, not sequenced: the caret belongs at the start of an
                // empty field, which it only is if the two share an origin.
                //
                // The placeholder is the only thing naming this field on screen
                // and it leaves at the first keystroke, which is why the node
                // carries a name of its own for the TalkBack user who has typed.
                Box {
                    if (state.filter.isEmpty()) {
                        Text(
                            text = stringResource(R.string.filter_placeholder),
                            style = Fui.Data,
                            color = Fui.TextDim,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    field()
                }
            },
            modifier = Modifier
                .weight(1f)
                .testTag(TAG_FILTER_FIELD)
                .semantics { contentDescription = named },
        )
        FilterTrailing(state, filtering, onFilter)
    }
}

/**
 * How much of the History survives the needle, and the way out.
 *
 * Ends on [Fui.RowInset], the same inset every row's button ends on, because
 * the `✕` sits directly above a column of them and a control half a gutter out
 * of line with that column is the first thing an eye finds on this screen.
 *
 * **The count is its own accessible node.** It was a fragment of the field's
 * name for as long as it was a text field's decoration, which left a live
 * region with nothing to be; now it is a sibling of the field rather than
 * inside it, and merging its descendants is all it takes.
 *
 * Spoken as a sentence rather than as `3/100`, which TalkBack reads out as a
 * date or as "three slash one hundred" depending on the engine. The visible
 * text stays the terse form; only the announcement is prose.
 *
 * A live region only **while a needle is on**, and `Polite` so it waits for the
 * keystroke to finish being announced. Every announcement then answers a key
 * just pressed; unconditional, an Entry arriving in the background would bump
 * the denominator and interrupt whatever a blind user was reading to tell them
 * a number they had not asked for.
 */
@Composable
private fun FilterTrailing(state: UiState, filtering: Boolean, onFilter: (String) -> Unit) {
    val shown = state.shown.entries.size
    val held = state.entries.size
    val spoken = stringResource(R.string.filter_count_spoken, shown, held)
    Row(
        modifier = Modifier.padding(start = 10.dp, end = Fui.RowInset),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(
            text = "$shown/$held",
            style = Fui.Micro,
            color = Fui.TextDim,
            modifier = Modifier
                .testTag(TAG_FILTER_COUNT)
                .semantics(mergeDescendants = true) {
                    contentDescription = spoken
                    if (filtering) liveRegion = LiveRegionMode.Polite
                },
        )
        if (state.filter.isNotEmpty()) {
            GlyphButton(
                glyph = "✕",
                onClick = { onFilter("") },
                contentDescription = stringResource(R.string.filter_clear),
                modifier = Modifier.testTag(TAG_FILTER_CLEAR),
            )
        }
    }
}

/**
 * The needle excluded everything this phone holds.
 *
 * No echo of the query: the field is 56dp above and still has it, and repeating
 * it here would be the app reading a person their own typing back. No
 * `CLEAR FILTER` button either — the `✕` is in the same band as the field, which
 * is where a hand already is.
 *
 * [atTheCap] is the one thing this panel can say that is not already on screen.
 * A phone holds the hundred most recently used Entries and no more, so a needle
 * that matches nothing may be matching nothing *here* while the Relay has it —
 * and only at the cap, because below it the list is everything there is and the
 * sentence would be a lecture about a boundary nobody is near.
 */
@Composable
private fun NoMatches(atTheCap: Boolean) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(24.dp)
            .testTag(TAG_HISTORY_NO_MATCHES),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(
            text = stringResource(R.string.history_no_matches),
            style = Fui.Heading,
            color = Fui.TextPrimary,
        )
        if (atTheCap) {
            Text(
                text = stringResource(R.string.history_cache_boundary),
                style = Fui.Prose,
                color = Fui.TextBody,
                textAlign = TextAlign.Center,
                modifier = Modifier.testTag(TAG_HISTORY_CACHE_BOUNDARY),
            )
        }
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
    animatePlacement: Boolean,
    onRecall: (Entry) -> Unit,
    onDelete: (Entry) -> Unit,
) {
    itemsIndexed(entries, key = { _, entry -> entry.id }) { index, entry ->
        // Keyed above, so a reorder is identity-tracked and no row swaps its
        // content. Without a placement spec it would still teleport, and a
        // stationary viewport plus a teleport is the one combination where
        // something moved and nothing showed you.
        val placed = Modifier.animateItem(
            placementSpec = if (animatePlacement) RowPlacement else null,
        )
        if (entry.undecryptable) {
            UndecryptableRow(entry, onDelete, placed)
        } else {
            // **Positional, not the newest.** The Filter can hide whatever was
            // at the head, and the marked row has to be the row `RECALL FIRST`
            // will hand over or the mark is a lie. The two read the same list.
            EntryRow(entry, ownDeviceId, first = index == 0, onRecall, onDelete, placed)
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
    first: Boolean,
    onRecall: (Entry) -> Unit,
    onDelete: (Entry) -> Unit,
    modifier: Modifier = Modifier,
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
        modifier = modifier.testTag(entryRowTag(entry.id)),
    ) {
        // Two layers, not one: the emitter's tint is 12% alpha and the delete
        // panel is sitting right behind it, so a single translucent background
        // paints the marked row red.
        Column(
            Modifier
                .background(Fui.Panel)
                .then(if (first) Modifier.background(Fui.Active) else Modifier),
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(Fui.RowHeight)
                    .padding(end = Fui.RowInset),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                // The emitter's own rule, on the one row `RECALL FIRST` will
                // hand over. Every other row is flush with the gutter.
                if (first) {
                    Box(Modifier.width(2.dp).fillMaxHeight().background(Fui.Cyan400))
                }
                Column(
                    modifier = Modifier.weight(1f).padding(start = if (first) 12.dp else Fui.Gutter),
                    verticalArrangement = Arrangement.spacedBy(5.dp),
                ) {
                    Text(
                        // The facade's Preview, verbatim — see `entryRows`.
                        text = entry.preview,
                        style = Fui.Data,
                        color = if (first) Fui.TextPrimary else Fui.TextBody,
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
                if (first) {
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
private fun UndecryptableRow(entry: Entry, onDelete: (Entry) -> Unit, modifier: Modifier = Modifier) {
    Column(modifier.testTag(entryRowTag(entry.id))) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(Fui.RowHeight)
                .background(Fui.AlertA16)
                .padding(end = Fui.RowInset),
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
 * Offer and Recall First, the two verbs that need no row *selected*.
 *
 * Deliberately **not** called Standing Actions: those are the verbs a device
 * exposes *without being opened*, and these are buttons on an open screen.
 *
 * **`RECALL FIRST` takes [first], the first row of the displayed list**, and
 * that is the whole of ADR 0010. `RECALL LATEST` fetched, which meant the row
 * it handed over need not be a row on screen at all — and once a Filter can
 * hide the head, "latest" and "the marked row" are two different Entries with
 * one button between them. So the verb bar and the marker read the same list
 * and the same index, and the button says which. The notification keeps
 * `RECALL LATEST` and keeps the fetch; under Last Use ordering the two select
 * the same Entry whenever nothing is filtered.
 *
 * It performs no fetch and it is not therefore inert: `recall` spawns a **Use**
 * after the clipboard write, so the Entry leads the History on every device
 * afterwards. Unfiltered that is idempotent — the head recalls itself and stays
 * the head, renewing its tenure.
 *
 * **Disabled rather than absent, and rather than a notice.** With no first row
 * there is nothing to hand over, and the control someone is looking for has to
 * still be where they are looking, saying no. An Undecryptable first row counts
 * as none: its own Recall is disabled for want of a key, and a verb bar that
 * fired on it would contradict the row it is marking.
 *
 * It is the solid one and comes first, because Recall is why a phone is opened
 * at all — the laptop copied something and the phone has to paste it. Offer
 * keeps a full-height target beside it, in the order the notification lists them.
 */
@Composable
private fun VerbBar(first: Entry?, actions: AppActions) {
    val recallable = first?.takeUnless { it.undecryptable }
    Column(Modifier.fillMaxWidth()) {
        Hairline(color = Fui.Frame)
        Row(
            modifier = Modifier.fillMaxWidth().background(Fui.Band).padding(Fui.Gutter),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            FuiButton(
                text = stringResource(R.string.recall_first_bar),
                onClick = { recallable?.let(actions.recall) },
                solid = true,
                enabled = recallable != null,
                modifier = Modifier.weight(1.6f).testTag(TAG_RECALL_FIRST),
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

/**
 * The Filter band's height, and the 56dp in "168dp of chrome".
 *
 * Material's own minimum for an outlined field, which is also the smallest one
 * that holds a 48dp `✕` without cropping it.
 */
private val FilterHeight = 56.dp

/**
 * How many Entries this phone keeps.
 *
 * `MAX_PER_USER` in the core's entries cache, which `list_recent` clamps every
 * request to and which `SharepasteRepository.listHistory` asks for by default.
 * Named here for one reason: at the cap, and only at the cap, `NO MATCHES` owes
 * the person the boundary — what is not on screen may still be on the Relay.
 */
private const val CACHE_CAP = 100

/**
 * How a row travels when the History reorders under it.
 *
 * Foundation's own default placement spec, spelled out because the gate needs
 * something to withhold: `null` is "snap", and a re-layout caused by typing has
 * to snap. Only a Use animates.
 */
private val RowPlacement = spring(
    stiffness = Spring.StiffnessMediumLow,
    visibilityThreshold = IntOffset.VisibilityThreshold,
)

const val TAG_HISTORY_SCREEN = "history-screen"

/**
 * The scrolling list itself.
 *
 * A `LazyColumn` composes only what is on screen, so a test that wants a row has
 * to scroll to it first, and without a handle on the list there is nothing to ask.
 */
const val TAG_HISTORY_LIST = "history-list"

const val TAG_HISTORY_EMPTY = "history-empty"

/** The Filter's own field, which a test types into. */
const val TAG_FILTER_FIELD = "filter-field"

/** `n/m`: how much of the History the needle left. */
const val TAG_FILTER_COUNT = "filter-count"

/** The `✕`. On screen only while there is a needle to clear. */
const val TAG_FILTER_CLEAR = "filter-clear"

/**
 * The panel that says the needle excluded everything.
 *
 * Distinct from [TAG_HISTORY_EMPTY], and the distinction is the assertion: a
 * phone with no Entries at all says `NOTHING HERE YET`, and a test that could
 * not tell the two apart could not hold that rule.
 */
const val TAG_HISTORY_NO_MATCHES = "history-no-matches"

/** The sentence naming the hundred-Entry cache boundary. Only at the cap. */
const val TAG_HISTORY_CACHE_BOUNDARY = "history-cache-boundary"

/** Which Pairing's History is on screen: `<user> @ <relay host>`. */
const val TAG_IDENTITY = "history-identity"

const val TAG_OFFER = "offer-clipboard"

/** The verb bar's Recall. It acts on the first *displayed* row. */
const val TAG_RECALL_FIRST = "recall-first"

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
