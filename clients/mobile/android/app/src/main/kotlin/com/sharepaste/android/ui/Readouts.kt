package com.sharepaste.android.ui

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.sharepaste.android.R

/**
 * What the phone says about its own Contact with the Relay.
 *
 * **"Not in contact" is nominal here, and this composable is where that rule is
 * kept.** On a desktop, relay health is surfaced only when it is degraded (ADR
 * 0002) — a sensible rule for something that is always connected. A phone is out
 * of contact almost all of the time, because sync is foreground-only (ADR 0007),
 * so the same rule would paint a perfectly healthy phone as permanently broken.
 *
 * Concretely: every phase except [SessionPhase.Refused] renders in the ordinary
 * secondary body colour, with no icon, no badge, no container and no
 * [TAG_FAULT]. A revoked Pairing is the only thing a person has to act on, and it
 * is the only thing that looks like it.
 */
@Composable
fun ContactReadout(phase: SessionPhase, modifier: Modifier = Modifier) {
    val text = phaseMessage(phase)?.let { stringResource(it) } ?: return

    when (toneOf(phase)) {
        Tone.Nominal -> Text(
            text = text,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = modifier.testTag(TAG_NOMINAL),
        )

        Tone.Fault -> Surface(
            color = MaterialTheme.colorScheme.errorContainer,
            shape = MaterialTheme.shapes.medium,
            modifier = modifier.testTag(TAG_FAULT),
        ) {
            Text(
                text = text,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onErrorContainer,
                modifier = Modifier.padding(12.dp),
            )
        }
    }
}

/**
 * The sentence for a phase, in one exhaustive `when`.
 *
 * Shared by the whole-phone readout above and by a single Pairing's card, so
 * there is one set of words for one set of states rather than two that drift.
 * `null` is "say nothing": an unpaired phone is on the pairing flow, where a
 * status line would be noise.
 */
@StringRes
private fun phaseMessage(phase: SessionPhase): Int? = when (phase) {
    SessionPhase.Unpaired -> null
    SessionPhase.Looking -> R.string.contact_looking
    is SessionPhase.InContact -> R.string.contact_online
    is SessionPhase.OutOfContact -> R.string.contact_offline
    is SessionPhase.Resting -> R.string.contact_resting
    is SessionPhase.NotActive -> R.string.contact_not_active
    is SessionPhase.Refused -> R.string.contact_refused
}

/**
 * One Pairing's own status, on its card.
 *
 * The same words and the same tone rule as [ContactReadout], with one difference
 * that earns its keep: the ordinary branch carries the card's own tag so a test
 * can read *this* Pairing's line out of a list of them, while [TAG_FAULT] stays
 * attached by exactly one branch across the whole app. Asserting that tag absent
 * therefore still *is* the assertion that nothing here reads as an error — which
 * is what a Pairing that is merely resting must not do.
 */
@Composable
fun PairingStatus(phase: SessionPhase, tag: String, modifier: Modifier = Modifier) {
    val text = phaseMessage(phase)?.let { stringResource(it) } ?: return

    when (toneOf(phase)) {
        Tone.Nominal -> Text(
            text = text,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = modifier.testTag(tag),
        )

        Tone.Fault -> Surface(
            color = MaterialTheme.colorScheme.errorContainer,
            shape = MaterialTheme.shapes.medium,
            modifier = modifier.testTag(TAG_FAULT),
        ) {
            Text(
                text = text,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onErrorContainer,
                modifier = Modifier.padding(12.dp).testTag(tag),
            )
        }
    }
}

/**
 * The Viewed Pairing is not the one this phone syncs, said out loud.
 *
 * Without this band the History shows one Pairing's Entries while the device
 * syncs another, and nothing on screen admits it — so a list that is quietly
 * frozen looks exactly like a list that is up to date. It offers the one action
 * that resolves the divergence rather than merely reporting it.
 *
 * Nominal in tone, and deliberately so: viewing a Pairing this phone is not
 * syncing is a thing a person chose to do, not a fault. No [TAG_FAULT], no error
 * container.
 */
@Composable
fun DivergenceBand(
    viewedName: String,
    activeName: String,
    onUseViewed: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Surface(
        color = MaterialTheme.colorScheme.secondaryContainer,
        modifier = modifier.fillMaxWidth().testTag(TAG_DIVERGED),
    ) {
        Column(
            modifier = Modifier.padding(start = 16.dp, end = 8.dp, top = 8.dp, bottom = 8.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                text = stringResource(R.string.pairing_diverged, viewedName, activeName),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSecondaryContainer,
            )
            TextButton(onClick = onUseViewed, modifier = Modifier.testTag(TAG_DIVERGED_USE)) {
                Text(stringResource(R.string.pairing_diverged_use))
            }
        }
    }
}

/**
 * What the last thing the person asked for did.
 *
 * One `when` over [Notice], so an outcome added without words for it does not
 * compile. [Notice.RecalledFromCache] is the only one that gets a container of
 * its own, and that is not decoration: every other notice is a plain statement
 * about something the person just did, while that one is a warning about the
 * content now on their clipboard — it may be yesterday's link. The rest read in
 * the ordinary voice, because being out of contact is nominal on a phone (ADR
 * 0007) and a refusal is a fact, not a fault.
 *
 * Lives here rather than on one screen because two screens now raise notices: a
 * Recall happens on the History and a Pairing is forgotten on the Pairings, and
 * both owe the person the same one sentence in the same place.
 */
@Composable
fun NoticeBanner(notice: Notice, onDismiss: () -> Unit, modifier: Modifier = Modifier) {
    val stale = notice is Notice.RecalledFromCache
    val text = when (notice) {
        is Notice.Offered -> stringResource(R.string.offer_queued)
        is Notice.OfferRefused -> stringResource(offerRefusalMessage(notice.reason))
        Notice.Recalled -> stringResource(R.string.recall_done)
        Notice.RecalledFromCache -> stringResource(R.string.recall_from_cache)
        Notice.Unpaired -> stringResource(R.string.action_unpaired)
        is Notice.HistoryCleared -> stringResource(R.string.history_cleared, notice.pairing)
        is Notice.PairingForgotten -> notice.promoted?.let {
            stringResource(R.string.pairing_forgotten_promoted, notice.pairing, it)
        } ?: stringResource(R.string.pairing_forgotten_last, notice.pairing)

        is Notice.Failed -> {
            val sentence = stringResource(notice.message)
            notice.detail?.let { "$sentence\n$it" } ?: sentence
        }
    }
    Surface(
        color = if (stale) {
            MaterialTheme.colorScheme.tertiaryContainer
        } else {
            MaterialTheme.colorScheme.surfaceVariant
        },
        modifier = modifier
            .fillMaxWidth()
            .testTag(if (stale) TAG_NOTICE_STALE else TAG_NOTICE),
    ) {
        Row(
            modifier = Modifier.padding(start = 16.dp, top = 8.dp, bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = text,
                style = MaterialTheme.typography.bodyMedium,
                modifier = Modifier.weight(1f),
            )
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.notice_dismiss)) }
        }
    }
}

/**
 * The one surprising thing about how this app works, said out loud.
 *
 * Sync is foreground only, so something copied on the laptop does not reach the
 * phone until Sharepaste is opened. Left unsaid, that reads as a bug — a person
 * copies a link, picks up their phone, and the history is empty. Said here, it
 * reads as a decision, which is what it is.
 */
@Composable
fun ForegroundOnlyNote(modifier: Modifier = Modifier) {
    Surface(
        color = MaterialTheme.colorScheme.surfaceVariant,
        shape = MaterialTheme.shapes.medium,
        modifier = modifier.fillMaxWidth().testTag(TAG_FOREGROUND_NOTE),
    ) {
        Column(Modifier.padding(16.dp)) {
            Text(
                text = stringResource(R.string.foreground_only_note),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/**
 * Why the Standing Actions notification is not there.
 *
 * Shown only when the platform is refusing to display it. Deliberately in the
 * **ordinary** voice with no [TAG_FAULT] and no error container: nothing is
 * broken. `POST_NOTIFICATIONS` is a runtime grant from API 33 and implicit
 * below it, and on any version a person can switch notifications off in
 * Settings — all of which are choices they are entitled to make. What they are
 * not entitled to is a feature that vanishes without a word, because a
 * notification that never appears looks exactly like one that was never built,
 * and `NotificationManager.notify` reports nothing at all when the permission
 * is denied.
 *
 * The sentence says the two verbs still work from this screen, because that is
 * the part a person cannot see for themselves and the part that decides whether
 * a denial has broken the app.
 */
@Composable
fun StandingActionsBlockedNote(onEnable: () -> Unit, modifier: Modifier = Modifier) {
    Surface(
        color = MaterialTheme.colorScheme.surfaceVariant,
        shape = MaterialTheme.shapes.medium,
        modifier = modifier.fillMaxWidth().testTag(TAG_STANDING_ACTIONS_BLOCKED),
    ) {
        Column(Modifier.padding(start = 16.dp, end = 8.dp, top = 16.dp, bottom = 8.dp)) {
            Text(
                text = stringResource(R.string.standing_actions_blocked),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            TextButton(onClick = onEnable, modifier = Modifier.testTag(TAG_STANDING_ACTIONS_ENABLE)) {
                Text(stringResource(R.string.standing_actions_enable))
            }
        }
    }
}

/** The readout in its ordinary voice. Present for every nominal phase. */
const val TAG_NOMINAL = "contact-nominal"

/**
 * The readout as a fault.
 *
 * Asserted absent for every disconnected phase and for every resting Pairing:
 * this tag existing is exactly what "renders as an error state" means, so a test
 * can hold the rule rather than trusting a reading of the code. It is attached by
 * the fault branch of [ContactReadout] and of [PairingStatus] and by nothing
 * else.
 */
const val TAG_FAULT = "contact-fault"

const val TAG_FOREGROUND_NOTE = "foreground-only-note"

const val TAG_NOTICE = "notice"

/**
 * The notice that says a Recall Latest fell back to the cache.
 *
 * Its own tag, so a test asserts the *visible statement* rather than the return
 * value that produced it. A silent fallback hands over yesterday's link, and the
 * only way that rule survives is if the thing proving it is on screen.
 */
const val TAG_NOTICE_STALE = "notice-stale-recall"

/** The band that admits the Viewed Pairing is not the one being synced. */
const val TAG_DIVERGED = "pairing-diverged"

/** The band's offer to make the Viewed Pairing the Active one. */
const val TAG_DIVERGED_USE = "pairing-diverged-use"

/**
 * The note that says the Standing Actions notification is not being shown.
 *
 * Attached by exactly one branch, so asserting it *absent* is the assertion that
 * a phone with notifications working says nothing about them.
 */
const val TAG_STANDING_ACTIONS_BLOCKED = "standing-actions-blocked"

/** Its one control: ask the platform for the notification back. */
const val TAG_STANDING_ACTIONS_ENABLE = "standing-actions-enable"
