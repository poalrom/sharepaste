package com.sharepaste.android.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.sharepaste.android.R
import com.sharepaste.core.PairingSummary

/**
 * The phone's Settings: every Pairing it holds, and the little it can be told.
 *
 * **Titled `SETTINGS`, with the Pairings as a section of it.** `PAIRINGS` was
 * honest while a Pairing was the only thing here; the moment the screen grew a
 * preference of its own, a title naming one of its four sections sent anyone
 * looking for a switch to a screen that does not exist. `Screen.Pairings` stays
 * the enum case — this is an information-architecture change, not a routing one,
 * and renaming the case would touch every call site to say nothing new.
 *
 * **Two distinctions, taken from the desktop rather than reinvented.** Exactly
 * one Pairing is the Active one: it is what this device syncs and captures to,
 * and the choice survives a restart. Any Pairing may be the Viewed one: that
 * changes nothing about syncing or capture and is forgotten when the app is put
 * down. When they diverge, [DivergenceBand] says so on both screens — otherwise
 * the History shows one Pairing while the device syncs another and nothing on
 * screen admits it.
 *
 * **The order is the cards, adding one, [ThisPhoneSection], [PhoneSettings].**
 * The one live switch sits under a heading of its own rather than among the
 * inert `N/A` chips, because a switch three lines above `WATCHED CAPTURE · N/A`
 * makes the chips read as switches somebody stopped wiring up — which is the
 * exact misreading the chips exist to prevent.
 */
@Composable
fun PairingsScreen(state: UiState, actions: AppActions, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier
            .fillMaxSize()
            .background(Fui.Panel)
            .fuiBackdrop()
            .testTag(TAG_PAIRINGS_SCREEN),
    ) {
        TitleBand(
            title = stringResource(R.string.pairings_title),
            onBack = actions.openHistory,
            backDescription = stringResource(R.string.pairings_back),
            backTag = TAG_BACK_TO_HISTORY,
        )
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
            contentPadding = PaddingValues(Fui.Gutter),
            verticalArrangement = Arrangement.spacedBy(14.dp),
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
            item { AddPairingSection(actions.openAddPairing) }
            item { Hairline() }
            item { ThisPhoneSection(state, actions) }
            item { Hairline() }
            item { PhoneSettings() }
        }
    }
}

/**
 * One Pairing, and everything a phone can do to it.
 *
 * The panel is headed by the **User** and its address, never by this machine's
 * Device Label: heading a Pairing with the local machine's name made every
 * Pairing on the desktop look like an account named after the computer. The
 * Device Label is a line *inside* the card, where it reads as what it is — what
 * this phone told the Relay to call itself.
 *
 * That address is the Relay's host and nothing else. The `user_id` used to lead
 * it, and the argument for putting it back is that it is the only truly unique
 * thing here — two Pairings can share a username. It stays out anyway: the
 * subtitle is not the disambiguator, [ConfirmStrip] is, and that is where a
 * choice with no way back gets spelled out. On the card the uuid bought nothing
 * and cost the host, which ellipsised away behind it.
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
    val phase = pairingPhase(pairing, foreground)
    FuiPanel(
        title = pairing.username ?: pairing.userId,
        code = pairing.relayHost,
        accent = if (toneOf(phase) == Tone.Fault) Accent.Alert else Accent.Emitter,
        modifier = Modifier.testTag(pairCardTag(pairing.userId)),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            // Exactly one card carries SYNCING; SHOWING moves independently of
            // it, because viewing a Pairing changes nothing about what the phone
            // syncs or captures to.
            if (pairing.isActive || viewed) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (pairing.isActive) {
                        FuiBadge(
                            text = stringResource(R.string.pairings_active_badge),
                            accent = Accent.Emitter,
                            solid = true,
                            modifier = Modifier.testTag(pairActiveTag(pairing.userId)),
                        )
                    }
                    if (viewed) {
                        FuiBadge(
                            text = stringResource(R.string.pairings_viewed_badge),
                            accent = Accent.Neutral,
                            modifier = Modifier.testTag(pairViewedTag(pairing.userId)),
                        )
                    }
                }
            }
            // The Device Label, shown and not editable. The Relay sets it once
            // at `POST /devices` and serves it from `GET /me`; there is no
            // rename route, so a local-only rename would show this phone one
            // name and every other device another. The rule itself is stated
            // where a name is still being chosen — `pair_label_explainer`, on
            // the pairing flow — not here, where only the result is on show.
            Text(
                text = stringResource(R.string.pairings_this_phone, pairing.label),
                style = Fui.Data,
                color = Fui.TextBody,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.testTag(pairLabelTag(pairing.userId)),
            )
            PairingStatus(phase = phase, tag = pairStatusTag(pairing.userId))
            // The one surface on this phone that shows a queue belonging to a
            // Pairing the device has switched away from. Nothing else would:
            // the History's own count is the Active Pairing's.
            if (pairing.pending > 0) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(Fui.AmberA16)
                        .padding(10.dp, 6.dp),
                ) {
                    Text(
                        text = pluralStringResource(
                            R.plurals.pairings_pending,
                            pairing.pending.toInt(),
                            pairing.pending,
                        ),
                        style = Fui.Micro,
                        color = Fui.Amber400,
                        modifier = Modifier.testTag(pairPendingTag(pairing.userId)),
                    )
                }
            }

            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                if (!viewed) {
                    FuiButton(
                        text = stringResource(R.string.pairings_view),
                        onClick = { actions.viewPairing(pairing.userId) },
                        height = Fui.TargetSmall,
                        modifier = Modifier.testTag(pairViewTag(pairing.userId)),
                    )
                }
                if (!pairing.isActive) {
                    FuiButton(
                        text = stringResource(R.string.pairings_use),
                        onClick = { actions.activatePairing(pairing.userId) },
                        height = Fui.TargetSmall,
                        modifier = Modifier.testTag(pairUseTag(pairing.userId)),
                    )
                }
                FuiButton(
                    text = stringResource(R.string.pairings_clear_history),
                    onClick = { actions.confirm(Confirmation.ClearHistory(pairing.userId)) },
                    accent = Accent.Neutral,
                    height = Fui.TargetSmall,
                    modifier = Modifier.testTag(pairClearTag(pairing.userId)),
                )
                FuiButton(
                    text = stringResource(R.string.pairings_forget),
                    onClick = { actions.confirm(Confirmation.Forget(pairing.userId)) },
                    accent = Accent.Alert,
                    height = Fui.TargetSmall,
                    modifier = Modifier.testTag(pairForgetTag(pairing.userId)),
                )
            }

            confirming?.let { ConfirmStrip(it, pairing, actions) }

            Hairline()
            // ADR 0002 puts cipher disclosure beside pairing — where the choice to
            // trust a Relay is being made — and nowhere else. One line per card,
            // and it is the only cipher this product names.
            Text(
                text = stringResource(R.string.cipher_disclosure),
                style = Fui.Micro,
                color = Fui.TextMuted,
                modifier = Modifier.testTag(pairCipherTag(pairing.userId)),
            )
        }
    }
}

/**
 * The yes-or-no strip for the two things that cannot be undone.
 *
 * Inline and inside the card it is about, never a dialog: the scope of the erase
 * stays on screen while the choice is being made. It names the **User and the
 * Relay**, not the heading — two Pairings can share a username, and this is the
 * one action with no way back.
 *
 * `KEEP IT` is the outline and the destructive verb is the solid one, which is
 * the opposite of the usual advice and is right here: the person has already
 * asked for this, the strip exists to make them read what it costs, and burying
 * the verb they came for behind the safe-looking button would just be answered
 * twice.
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
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(Fui.AlertA16)
            .padding(12.dp)
            .testTag(pairConfirmTag(pairing.userId)),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        FuiBadge(stringResource(R.string.pairings_confirm_badge), Accent.Alert, solid = true)
        Text(question, style = Fui.Prose, color = Fui.TextPrimary)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            FuiButton(
                text = stringResource(R.string.pairings_cancel),
                onClick = { actions.confirm(null) },
                height = Fui.TargetSmall,
                modifier = Modifier.testTag(pairCancelTag(pairing.userId)),
            )
            FuiButton(
                text = stringResource(
                    when (confirming) {
                        is Confirmation.ClearHistory -> R.string.pairings_clear_history
                        is Confirmation.Forget -> R.string.pairings_forget
                    },
                ),
                onClick = {
                    when (confirming) {
                        is Confirmation.ClearHistory -> actions.clearHistory(pairing.userId)
                        is Confirmation.Forget -> actions.forgetPairing(pairing.userId)
                    }
                },
                accent = Accent.Alert,
                solid = true,
                height = Fui.TargetSmall,
                modifier = Modifier.testTag(pairConfirmYesTag(pairing.userId)),
            )
        }
    }
}

@Composable
private fun AddPairingSection(onAdd: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SectionHeading(stringResource(R.string.pairings_add_heading))
        Text(
            text = stringResource(R.string.pairings_add_body),
            style = Fui.Prose,
            color = Fui.TextBody,
        )
        FuiButton(
            text = stringResource(R.string.pairings_add_button),
            onClick = onAdd,
            modifier = Modifier.testTag(TAG_ADD_PAIRING),
        )
    }
}

/**
 * The one thing this phone can actually be told.
 *
 * A Recall reaches the clipboard whichever way the switch is set; all it decides
 * is whether the Receipt names what arrived. That earns a control because the
 * Receipt is the only part of a Recall legible to whoever is standing next to
 * you — and it earns *only* this control. Silencing the Recall Receipt while the
 * Offer's still speaks is the whole feature, so this is not a quiet mode and is
 * not worded as one.
 *
 * Its own section rather than a row inside [PhoneSettings] — see this file's
 * header for why a live switch may not stand next to the inert chips.
 */
@Composable
private fun ThisPhoneSection(state: UiState, actions: AppActions) {
    Column(
        verticalArrangement = Arrangement.spacedBy(4.dp),
        modifier = Modifier.testTag(TAG_THIS_PHONE),
    ) {
        SectionHeading(stringResource(R.string.settings_this_phone_heading))
        SettingSwitch(
            label = stringResource(R.string.settings_show_recalled),
            checked = state.showRecalled,
            onCheckedChange = actions.setShowRecalled,
            modifier = Modifier.testTag(TAG_SHOW_RECALLED),
        )
        // Not a [QuotedNote]: the rule down its left edge would cut the sentence
        // away from the switch it explains, and two short sentences do not need
        // an idiom built for prose a box would make look like an alert.
        // `ADD ANOTHER PAIRING` states its own body exactly this way.
        Text(
            text = stringResource(R.string.settings_show_recalled_note),
            style = Fui.Prose,
            color = Fui.TextBody,
            modifier = Modifier.testTag(TAG_SHOW_RECALLED_NOTE),
        )
    }
}

/**
 * A preference, as a row: the word on the left, its state on the right.
 *
 * There is no Material `Switch` here and there is not going to be one. This
 * screen's vocabulary is [FuiButton], [FuiBadge] and [FuiTag] — square-cornered
 * borders over the void — and a filled pill with a circular thumb is the one
 * shape in the palette that would announce it came from somewhere else. The
 * thumb is a square for the same reason [StatusLight]'s lamp is: a HUD does not
 * draw circles, and a small square survives a low-density screen that eats a
 * small circle's edges.
 *
 * The whole row is the target, never the track. A 40dp track is under the 48dp
 * floor on its own, and the label is what a thumb aims at anyway. `Role.Switch`
 * is what makes a screen reader say "on" instead of describing a rectangle, and
 * it is why this is `toggleable` rather than a [FuiButton] that flips a flag.
 *
 * Off drops the label to prose ink and never to [Fui.TextDim], which on this
 * screen means inert: a preference that is switched off is still a preference,
 * and the three chips below are the only thing here entitled to look disabled.
 */
@Composable
private fun SettingSwitch(
    label: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = Fui.Target)
            .toggleable(value = checked, role = Role.Switch, onValueChange = onCheckedChange),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = label,
            style = Fui.Label,
            color = if (checked) Fui.TextEmitter else Fui.TextBody,
            modifier = Modifier.weight(1f),
        )
        Box(
            modifier = Modifier
                .size(width = 40.dp, height = 22.dp)
                .then(if (checked) Modifier.background(Fui.CyanA12) else Modifier)
                .border(1.dp, if (checked) Fui.Frame else Fui.Inert)
                .padding(3.dp),
            contentAlignment = if (checked) Alignment.CenterEnd else Alignment.CenterStart,
        ) {
            Box(Modifier.size(14.dp).background(if (checked) Fui.Cyan400 else Fui.Inert))
        }
    }
}

/**
 * What this phone is, and why the computer's two switches are not here.
 *
 * Stated rather than left as a gap. Someone who knows the desktop will come
 * looking for the capture switch and the deny-list, and finding nothing is
 * indistinguishable from finding a half-built screen. Both are inert on a phone —
 * one governs Watched Capture, which a phone never performs; the other matches a
 * frontmost application, which a phone has no notion of — and saying so takes two
 * sentences and three chips.
 *
 * The third chip is the one the desktop cannot show: a phone carries no update
 * code at all (ADR 0008), so unlike a desktop it never asks an update source
 * anything and the Relay is the only counterparty it has.
 *
 * **The foreground-only rule is stated here at full length, and permanently.**
 * The History's band says the same thing, but `▴ CLOSE` retires it for good, and
 * a fact that can be dismissed for good needs somewhere it cannot be — which has
 * to be the section describing what this phone *is*. It reads above
 * `settings_absent_note` because that note is the caption for the chips directly
 * under it, and anything wedged between the two leaves three chips heading
 * nothing.
 *
 * The Device Label note is deliberately gone. Its one load-bearing fact — that
 * the Relay names a device once and has no rename route — now sits in
 * `pair_label_explainer`, beside the field where a name is still being chosen. A
 * rule stated where it can be acted on beats the same rule stated on the screen
 * that only displays the result.
 */
@Composable
private fun PhoneSettings() {
    Column(
        verticalArrangement = Arrangement.spacedBy(10.dp),
        modifier = Modifier.testTag(TAG_PHONE_SETTINGS),
    ) {
        SectionHeading(stringResource(R.string.settings_heading))
        QuotedNote(
            text = stringResource(R.string.foreground_only_note),
            modifier = Modifier.testTag(TAG_SETTINGS_FOREGROUND_NOTE),
        )
        QuotedNote(
            text = stringResource(R.string.settings_absent_note),
            modifier = Modifier.testTag(TAG_SETTINGS_ABSENT_NOTE),
        )
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
            modifier = Modifier.padding(top = 2.dp),
        ) {
            FuiTag(stringResource(R.string.settings_tag_watched_capture), inert = true)
            FuiTag(stringResource(R.string.settings_tag_deny_list), inert = true)
            FuiTag(stringResource(R.string.settings_tag_update_check), inert = true)
        }
    }
}

const val TAG_PAIRINGS_SCREEN = "pairings-screen"
const val TAG_PAIRINGS_LIST = "pairings-list"
const val TAG_BACK_TO_HISTORY = "pairings-back"
const val TAG_ADD_PAIRING = "pairings-add"
const val TAG_THIS_PHONE = "this-phone"
const val TAG_SHOW_RECALLED = "settings-show-recalled"
const val TAG_SHOW_RECALLED_NOTE = "settings-show-recalled-note"
const val TAG_PHONE_SETTINGS = "phone-settings"
const val TAG_SETTINGS_FOREGROUND_NOTE = "settings-foreground-note"
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
