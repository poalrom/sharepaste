package com.sharepaste.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.sharepaste.android.R
import com.sharepaste.core.Entry

/**
 * The Entries of the Active Pairing, and the two things a person can do here.
 *
 * The list is the whole point of the client: something copied on the computer
 * shows up as an Entry, and a Recall puts it back on this phone's clipboard.
 * Above it, the Contact readout and the foreground-only note stay exactly where
 * ticket 09 put them — the note especially, because "nothing arrives while this
 * is closed" is the single most surprising thing about how this app works and it
 * belongs where a puzzled person is already looking.
 *
 * Takes a whole [UiState] and an [AppActions] rather than a widening list of
 * pieces: no composable here sees the state holder, the repository or the core,
 * which is what lets every sentence on this screen be asserted with no facade
 * behind it.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HistoryScreen(state: UiState, actions: AppActions, modifier: Modifier = Modifier) {
    Scaffold(
        modifier = modifier.testTag(TAG_HISTORY_SCREEN),
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.history_title)) },
                // The only way to the Pairings, and it belongs here rather than
                // behind a drawer: on a phone that holds one Pairing it is a door
                // to the settings, and on a phone that holds several it is the
                // only place the other ones exist at all.
                actions = {
                    TextButton(
                        onClick = actions.openPairings,
                        modifier = Modifier.testTag(TAG_OPEN_PAIRINGS),
                    ) {
                        Text(stringResource(R.string.pairings_open))
                    }
                },
            )
        },
        bottomBar = { OfferAndRecall(actions) },
    ) { insets ->
        Column(modifier = Modifier.fillMaxSize().padding(insets)) {
            // Above the list rather than in it: what just happened is not an
            // Entry, and a notice that scrolled away with the rows would be a
            // notice the person never read.
            state.notice?.let { NoticeBanner(it, actions.dismissNotice) }
            // The same reasoning, more urgently. These rows belong to a Pairing
            // this phone is not syncing, so nothing here is being kept up to
            // date — and a frozen list looks exactly like a current one.
            if (state.diverged) {
                DivergenceBand(
                    viewedName = state.nameOf(state.viewedPairing),
                    activeName = state.nameOf(state.activeUserId),
                    onUseViewed = { state.viewedPairing?.let(actions.activatePairing) },
                )
            }
            if (state.pending > 0) PendingReadout(state.pending)
            LazyColumn(
                modifier = Modifier.fillMaxSize().testTag(TAG_HISTORY_LIST),
                contentPadding = PaddingValues(16.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                item { ContactReadout(state.session) }
                item { ForegroundOnlyNote() }
                // Beside the foreground-only note, because they are the same
                // kind of fact: what this app will and will not do while you
                // are not looking at it. Only when the platform is actually
                // refusing — a phone whose notifications work says nothing.
                if (state.standingActionsBlocked) {
                    item { StandingActionsBlockedNote(actions.enableStandingActions) }
                }
                if (state.entries.isEmpty()) {
                    item { EmptyHistory() }
                } else {
                    entryRows(state.entries, state.ownDeviceId, actions.recall, actions.deleteEntry)
                }
            }
        }
    }
}

@Composable
private fun EmptyHistory() {
    Column(
        verticalArrangement = Arrangement.spacedBy(8.dp),
        modifier = Modifier.testTag(TAG_HISTORY_EMPTY),
    ) {
        Text(
            text = stringResource(R.string.history_empty_heading),
            style = MaterialTheme.typography.titleMedium,
        )
        Text(
            text = stringResource(R.string.history_empty_body),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/**
 * The rows.
 *
 * One item per Entry, and the surrounding screen never has to change — the seam
 * ticket 09 left. Three things each row has to get right, and each of them is a
 * mistake the desktop made first:
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
    items(entries, key = { it.id }) { entry ->
        EntryRow(entry, ownDeviceId, onRecall, onDelete)
    }
}

@Composable
private fun EntryRow(
    entry: Entry,
    ownDeviceId: String?,
    onRecall: (Entry) -> Unit,
    onDelete: (Entry) -> Unit,
) {
    Column(
        verticalArrangement = Arrangement.spacedBy(4.dp),
        modifier = Modifier.fillMaxWidth().testTag(entryRowTag(entry.id)),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            if (entry.undecryptable) {
                Text(
                    text = stringResource(R.string.entry_undecryptable),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error,
                    maxLines = 2,
                    modifier = Modifier.weight(1f).testTag(entryUndecryptableTag(entry.id)),
                )
            } else {
                Text(
                    // The facade's Preview, verbatim — see `entryRows`.
                    text = entry.preview,
                    style = MaterialTheme.typography.bodyLarge,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f).testTag(entryPreviewTag(entry.id)),
                )
            }
            // Disabled rather than hidden for an Undecryptable Entry, following
            // the desktop's detail pane and not its row: the control someone is
            // looking for has to still be where they are looking, saying no. The
            // marker beside it is the reason.
            TextButton(
                onClick = { onRecall(entry) },
                enabled = !entry.undecryptable,
                modifier = Modifier.testTag(entryRecallTag(entry.id)),
            ) {
                Text(stringResource(R.string.entry_recall))
            }
            // Always offered. Ciphertext this phone cannot read is the row a
            // person most wants gone, and deleting is the only thing they can do
            // with it.
            TextButton(
                onClick = { onDelete(entry) },
                modifier = Modifier.testTag(entryDeleteTag(entry.id)),
            ) {
                Text(stringResource(R.string.entry_delete))
            }
        }
        if (entry.deviceId != ownDeviceId) {
            Text(
                // Resolved by the core: the Device Label, or a slice of the
                // Device id when the mirror has none.
                text = stringResource(R.string.entry_origin, entry.originLabel),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                modifier = Modifier.testTag(entryOriginTag(entry.id)),
            )
        }
    }
}

/**
 * Offer and Recall Latest, the two verbs that need no row selected.
 *
 * Deliberately **not** called Standing Actions: those are the verbs a device
 * exposes *without being opened*, and these are buttons on an open screen. They
 * call the same two repository entry points ticket 12's Standing Actions will,
 * which is the point — neither of those entry points assumes a composition
 * exists.
 */
@Composable
private fun OfferAndRecall(actions: AppActions) {
    Surface(tonalElevation = 3.dp) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            TextButton(
                onClick = actions.offerClipboard,
                modifier = Modifier.weight(1f).testTag(TAG_OFFER),
            ) {
                Text(stringResource(R.string.offer_button))
            }
            TextButton(
                onClick = actions.recallLatest,
                modifier = Modifier.weight(1f).testTag(TAG_RECALL_LATEST),
            ) {
                Text(stringResource(R.string.recall_latest_button))
            }
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
 */
@Composable
private fun PendingReadout(pending: Long) {
    Surface(color = MaterialTheme.colorScheme.surfaceVariant, modifier = Modifier.fillMaxWidth()) {
        Text(
            text = pluralStringResource(R.plurals.pending_count, pending.toInt(), pending),
            style = MaterialTheme.typography.bodyMedium,
            modifier = Modifier.padding(16.dp, 8.dp).testTag(TAG_PENDING),
        )
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
const val TAG_OFFER = "offer-clipboard"
const val TAG_RECALL_LATEST = "recall-latest"
const val TAG_OPEN_PAIRINGS = "open-pairings"
const val TAG_PENDING = "pending-count"

fun entryRowTag(id: Long) = "entry-$id"
fun entryPreviewTag(id: Long) = "entry-preview-$id"
fun entryOriginTag(id: Long) = "entry-origin-$id"
fun entryUndecryptableTag(id: Long) = "entry-undecryptable-$id"
fun entryRecallTag(id: Long) = "entry-recall-$id"
fun entryDeleteTag(id: Long) = "entry-delete-$id"
