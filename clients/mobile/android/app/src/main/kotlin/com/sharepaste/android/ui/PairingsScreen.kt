package com.sharepaste.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
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
import com.sharepaste.core.PairingSummary

/**
 * Every Pairing this phone holds, and the settings a phone actually has.
 *
 * **Two distinctions, taken from the desktop rather than reinvented.** Exactly
 * one Pairing is the Active one: it is what this device syncs and captures to,
 * and the choice survives a restart. Any Pairing may be the Viewed one: that
 * changes nothing about syncing or capture and is forgotten when the app is put
 * down. When they diverge, [DivergenceBand] says so on both screens — otherwise
 * the History shows one Pairing while the device syncs another and nothing on
 * screen admits it.
 *
 * **The settings are thinner than the desktop's, and the screen says why.** A
 * phone never performs Watched Capture, so `capture_enabled` governs nothing
 * here; a phone has no frontmost-application identity, so a deny-list has nothing
 * to match. Both are inert rather than missing, and [PhoneSettings] states that
 * in the product's voice rather than leaving it to look like an omission. What
 * remains is on the cards: the Device Label this phone chose, clearing a History,
 * forgetting a Pairing, and the cipher disclosure ADR 0002 puts beside pairing.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PairingsScreen(state: UiState, actions: AppActions, modifier: Modifier = Modifier) {
    Scaffold(
        modifier = modifier.testTag(TAG_PAIRINGS_SCREEN),
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.pairings_title)) },
                navigationIcon = {
                    TextButton(
                        onClick = actions.openHistory,
                        modifier = Modifier.testTag(TAG_BACK_TO_HISTORY),
                    ) {
                        Text(stringResource(R.string.pairings_back))
                    }
                },
            )
        },
    ) { insets ->
        Column(modifier = Modifier.fillMaxSize().padding(insets)) {
            state.notice?.let { NoticeBanner(it, actions.dismissNotice) }
            if (state.diverged) {
                DivergenceBand(
                    viewedName = state.nameOf(state.viewedPairing),
                    activeName = state.nameOf(state.activeUserId),
                    onUseViewed = { state.viewedPairing?.let(actions.activatePairing) },
                )
            }
            LazyColumn(
                modifier = Modifier.fillMaxSize().testTag(TAG_PAIRINGS_LIST),
                contentPadding = PaddingValues(16.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                items(state.pairings, key = { it.userId }) { pairing ->
                    PairingCard(
                        pairing = pairing,
                        viewed = pairing.userId == state.viewedPairing,
                        foreground = state.foreground,
                        confirming = state.confirming?.takeIf { it.userId == pairing.userId },
                        actions = actions,
                    )
                }
                item { AddPairingRow(actions.openAddPairing) }
                item { PhoneSettings() }
            }
        }
    }
}

/**
 * One Pairing, and everything a phone can do to it.
 *
 * The heading names the **User**, not this machine: `label` is the Device Label
 * this phone chose when it paired, and heading the card with it made every
 * Pairing on the desktop look like an account named after the local machine.
 *
 * A card that is not the Active one is *resting*, not faulty — see
 * [pairingPhase]. That is the whole reason [PairingStatus] exists rather than a
 * status colour chosen here.
 */
@Composable
private fun PairingCard(
    pairing: PairingSummary,
    viewed: Boolean,
    foreground: Boolean,
    confirming: Confirmation?,
    actions: AppActions,
) {
    Surface(
        color = MaterialTheme.colorScheme.surfaceVariant,
        shape = MaterialTheme.shapes.medium,
        modifier = Modifier.fillMaxWidth().testTag(pairCardTag(pairing.userId)),
    ) {
        Column {
            Column(
                modifier = Modifier.padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        text = pairing.username ?: pairing.userId,
                        style = MaterialTheme.typography.titleMedium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false),
                    )
                    if (pairing.isActive) {
                        Badge(R.string.pairings_active_badge, pairActiveTag(pairing.userId))
                    }
                    if (viewed) {
                        Badge(R.string.pairings_viewed_badge, pairViewedTag(pairing.userId))
                    }
                }
                Text(
                    text = "${pairing.userId} @ ${pairing.relayHost}",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                // The Device Label, shown and not editable. The Relay sets it once
                // at `POST /devices` and serves it from `GET /me`; there is no
                // rename route, so a local-only rename would show this phone one
                // name and every other device another. See `settings_label_note`.
                Text(
                    text = stringResource(R.string.pairings_this_phone, pairing.label),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.testTag(pairLabelTag(pairing.userId)),
                )
                PairingStatus(
                    phase = pairingPhase(pairing, foreground),
                    tag = pairStatusTag(pairing.userId),
                )
                // The one surface on this phone that shows a queue belonging to a
                // Pairing the device has switched away from. Nothing else would:
                // the History's own count is the Active Pairing's.
                if (pairing.pending > 0) {
                    Text(
                        text = pluralStringResource(
                            R.plurals.pending_count,
                            pairing.pending.toInt(),
                            pairing.pending,
                        ),
                        style = MaterialTheme.typography.bodySmall,
                        modifier = Modifier.testTag(pairPendingTag(pairing.userId)),
                    )
                }

                FlowRow(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    if (!viewed) {
                        TextButton(
                            onClick = { actions.viewPairing(pairing.userId) },
                            modifier = Modifier.testTag(pairViewTag(pairing.userId)),
                        ) {
                            Text(stringResource(R.string.pairings_view))
                        }
                    }
                    if (!pairing.isActive) {
                        TextButton(
                            onClick = { actions.activatePairing(pairing.userId) },
                            modifier = Modifier.testTag(pairUseTag(pairing.userId)),
                        ) {
                            Text(stringResource(R.string.pairings_use))
                        }
                    }
                    TextButton(
                        onClick = { actions.confirm(Confirmation.ClearHistory(pairing.userId)) },
                        modifier = Modifier.testTag(pairClearTag(pairing.userId)),
                    ) {
                        Text(stringResource(R.string.pairings_clear_history))
                    }
                    TextButton(
                        onClick = { actions.confirm(Confirmation.Forget(pairing.userId)) },
                        modifier = Modifier.testTag(pairForgetTag(pairing.userId)),
                    ) {
                        Text(stringResource(R.string.pairings_forget))
                    }
                }
            }

            confirming?.let { ConfirmStrip(it, pairing, actions) }

            HorizontalDivider()
            // ADR 0002 puts cipher disclosure beside pairing — where the choice to
            // trust a Relay is being made — and nowhere else. One line per card,
            // and it is the only cipher this product names.
            Text(
                text = stringResource(R.string.cipher_disclosure),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier
                    .padding(horizontal = 16.dp, vertical = 6.dp)
                    .testTag(pairCipherTag(pairing.userId)),
            )
        }
    }
}

/**
 * The yes-or-no strip for the two things that cannot be undone.
 *
 * Inline and under the card it is about, never a dialog: the scope of the erase
 * stays on screen while the choice is being made. It names the **User and the
 * Relay**, not the heading — two Pairings can share a username, and this is the
 * one action with no way back.
 */
@Composable
private fun ConfirmStrip(
    confirming: Confirmation,
    pairing: PairingSummary,
    actions: AppActions,
) {
    val target = "${pairing.username ?: pairing.userId} @ ${pairing.relayHost}"
    val question = when (confirming) {
        is Confirmation.ClearHistory -> stringResource(R.string.pairings_clear_confirm, target)
        is Confirmation.Forget -> stringResource(R.string.pairings_forget_confirm, target)
    }
    Surface(
        color = MaterialTheme.colorScheme.errorContainer,
        modifier = Modifier.fillMaxWidth().testTag(pairConfirmTag(pairing.userId)),
    ) {
        Column(
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                text = question,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onErrorContainer,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton(
                    onClick = { actions.confirm(null) },
                    modifier = Modifier.testTag(pairCancelTag(pairing.userId)),
                ) {
                    Text(stringResource(R.string.pairings_cancel))
                }
                TextButton(
                    onClick = {
                        when (confirming) {
                            is Confirmation.ClearHistory -> actions.clearHistory(pairing.userId)
                            is Confirmation.Forget -> actions.forgetPairing(pairing.userId)
                        }
                    },
                    modifier = Modifier.testTag(pairConfirmYesTag(pairing.userId)),
                ) {
                    Text(
                        stringResource(
                            when (confirming) {
                                is Confirmation.ClearHistory -> R.string.pairings_clear_history
                                is Confirmation.Forget -> R.string.pairings_forget
                            },
                        ),
                    )
                }
            }
        }
    }
}

@Composable
private fun Badge(message: Int, tag: String) {
    Surface(
        color = MaterialTheme.colorScheme.secondaryContainer,
        shape = MaterialTheme.shapes.extraSmall,
        modifier = Modifier.padding(start = 8.dp).testTag(tag),
    ) {
        Text(
            text = stringResource(message),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSecondaryContainer,
            modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
        )
    }
}

@Composable
private fun AddPairingRow(onAdd: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(
            text = stringResource(R.string.pairings_add_heading),
            style = MaterialTheme.typography.titleSmall,
        )
        Text(
            text = stringResource(R.string.pairings_add_body),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        TextButton(onClick = onAdd, modifier = Modifier.testTag(TAG_ADD_PAIRING)) {
            Text(stringResource(R.string.pairings_add_button))
        }
    }
}

/**
 * The settings a phone has, and the reason the rest are not here.
 *
 * Stated rather than left as a gap. Someone who knows the desktop will come
 * looking for the capture switch and the deny-list, and finding nothing is
 * indistinguishable from finding a half-built screen. Both are inert on a phone —
 * one governs Watched Capture, which a phone never performs; the other matches a
 * frontmost application, which a phone has no notion of — and saying so takes two
 * sentences.
 *
 * The Device Label rule belongs here for the same reason. It is display-only
 * because the Relay sets it once at `POST /devices` and has no rename route, so a
 * local rename would desync from what every other device sees: worse than not
 * offering one.
 */
@Composable
private fun PhoneSettings() {
    Column(
        verticalArrangement = Arrangement.spacedBy(8.dp),
        modifier = Modifier.testTag(TAG_PHONE_SETTINGS),
    ) {
        Text(
            text = stringResource(R.string.settings_heading),
            style = MaterialTheme.typography.titleSmall,
        )
        Text(
            text = stringResource(R.string.settings_label_note),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.testTag(TAG_SETTINGS_LABEL_NOTE),
        )
        Text(
            text = stringResource(R.string.settings_absent_note),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.testTag(TAG_SETTINGS_ABSENT_NOTE),
        )
    }
}

const val TAG_PAIRINGS_SCREEN = "pairings-screen"
const val TAG_PAIRINGS_LIST = "pairings-list"
const val TAG_BACK_TO_HISTORY = "pairings-back"
const val TAG_ADD_PAIRING = "pairings-add"
const val TAG_PHONE_SETTINGS = "phone-settings"
const val TAG_SETTINGS_LABEL_NOTE = "settings-label-note"
const val TAG_SETTINGS_ABSENT_NOTE = "settings-absent-note"

fun pairCardTag(userId: String) = "pair-$userId"
fun pairActiveTag(userId: String) = "pair-active-$userId"
fun pairViewedTag(userId: String) = "pair-viewed-$userId"
fun pairLabelTag(userId: String) = "pair-label-$userId"
fun pairStatusTag(userId: String) = "pair-status-$userId"
fun pairPendingTag(userId: String) = "pair-pending-$userId"
fun pairViewTag(userId: String) = "pair-view-$userId"
fun pairUseTag(userId: String) = "pair-use-$userId"
fun pairClearTag(userId: String) = "pair-clear-$userId"
fun pairForgetTag(userId: String) = "pair-forget-$userId"
fun pairCipherTag(userId: String) = "pair-cipher-$userId"
fun pairConfirmTag(userId: String) = "pair-confirm-$userId"
fun pairConfirmYesTag(userId: String) = "pair-confirm-yes-$userId"
fun pairCancelTag(userId: String) = "pair-cancel-$userId"
