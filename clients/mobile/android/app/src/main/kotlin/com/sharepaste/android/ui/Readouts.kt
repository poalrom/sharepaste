package com.sharepaste.android.ui

import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
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
 * The redesign inverts the desktop's rule rather than copying it: the readout is
 * **permanent chrome**, one band that is always there, so its appearance carries
 * no news of its own and only the words inside it change. Every phase except
 * [SessionPhase.Refused] is a status light in the ordinary voice with no
 * container and no [TAG_FAULT].
 *
 * A revoked Pairing is the one thing a person has to act on, so it is the one
 * thing that looks like it — and the only one that is a sentence rather than a
 * readout, because no amount of waiting fixes a revoked token and the band has
 * to say what to do instead. [onPairAgain] is how; when it is `null` the
 * sentence stands without its control, which is what a readout rendered outside
 * the app's navigation gets.
 */
@Composable
fun ContactReadout(
    phase: SessionPhase,
    modifier: Modifier = Modifier,
    onPairAgain: (() -> Unit)? = null,
) {
    val message = phaseMessage(phase) ?: return

    when (toneOf(phase)) {
        Tone.Nominal -> ChromeBand(
            height = 30.dp,
            modifier = modifier.testTag(TAG_NOMINAL),
            scanlines = true,
        ) {
            StatusLight(signalOf(phase), stringResource(message))
        }

        Tone.Fault -> Column(modifier.fillMaxWidth().testTag(TAG_FAULT)) {
            ChromeBand(height = 30.dp, background = Fui.AlertA16) {
                StatusLight(Signal.Alert, stringResource(message))
            }
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(Fui.AlertA16)
                    .padding(Fui.Gutter),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Text(stringResource(R.string.contact_refused), style = Fui.Prose, color = Fui.TextPrimary)
                if (onPairAgain != null) {
                    FuiButton(
                        text = stringResource(R.string.contact_pair_again),
                        onClick = onPairAgain,
                        accent = Accent.Alert,
                        solid = true,
                        modifier = Modifier.testTag(TAG_PAIR_AGAIN),
                    )
                }
            }
            Hairline(color = Fui.AlertA40)
        }
    }
}

/**
 * The sentence for a phase, in one exhaustive `when`.
 *
 * Shared by the whole-phone readout above and by a single Pairing's card, so
 * there is one set of words for one set of states rather than two that drift.
 * `null` is "say nothing": an unpaired phone is on the pairing flow, where a
 * status line would be noise, and both callers return on it rather than
 * inventing words for a Pairing that does not exist.
 */
@StringRes
private fun phaseMessage(phase: SessionPhase): Int? = when (phase) {
    SessionPhase.Unpaired -> null
    SessionPhase.Looking -> R.string.contact_looking
    is SessionPhase.InContact -> R.string.contact_online
    is SessionPhase.OutOfContact -> R.string.contact_offline
    is SessionPhase.Resting -> R.string.contact_resting
    is SessionPhase.NotActive -> R.string.contact_not_active
    is SessionPhase.Refused -> R.string.contact_refused_short
}

/**
 * Which lamp a phase lights.
 *
 * Three of the four states are nominal and only one is green: being *in* contact
 * is the exceptional state on a phone, and a band that went green whenever
 * nothing was wrong would be grey almost always and read as a warning. Standby
 * is the resting colour, caution is work in progress, and alert is only ever the
 * revoked token.
 *
 * Which phases are faults is [toneOf]'s rule and is asked rather than restated,
 * so a phase cannot be a `Fault` here and a lamp colour there.
 */
private fun signalOf(phase: SessionPhase): Signal {
    if (toneOf(phase) == Tone.Fault) return Signal.Alert
    return when (phase) {
        is SessionPhase.InContact -> Signal.Nominal
        SessionPhase.Looking -> Signal.Caution
        SessionPhase.Unpaired,
        is SessionPhase.OutOfContact,
        is SessionPhase.Resting,
        is SessionPhase.NotActive,
        -> Signal.Standby
        // Answered above. Repeated rather than swept up by an `else`, so a
        // phase added to the core arrives here as a compile error.
        is SessionPhase.Refused -> Signal.Alert
    }
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
    val message = phaseMessage(phase) ?: return

    when (toneOf(phase)) {
        Tone.Nominal -> StatusLight(
            signal = signalOf(phase),
            label = stringResource(message),
            modifier = modifier.testTag(tag),
        )

        Tone.Fault -> Column(
            modifier = modifier
                .fillMaxWidth()
                .background(Fui.AlertA16)
                .padding(10.dp)
                .testTag(TAG_FAULT),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            StatusLight(Signal.Alert, stringResource(message))
            Text(
                text = stringResource(R.string.contact_refused),
                style = Fui.Prose,
                color = Fui.TextPrimary,
                modifier = Modifier.testTag(tag),
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
 * syncing is a thing a person chose to do, not a fault. It is drawn in the
 * emitter's own tint rather than in a warning colour — no [TAG_FAULT], no alert.
 */
@Composable
fun DivergenceBand(
    viewedName: String,
    activeName: String,
    onUseViewed: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier.fillMaxWidth().testTag(TAG_DIVERGED)) {
        Column(
            modifier = Modifier.fillMaxWidth().background(Fui.Active).padding(Fui.Gutter, 12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                text = stringResource(R.string.pairing_diverged, viewedName, activeName),
                style = Fui.Prose,
                color = Fui.TextPrimary,
            )
            FuiButton(
                text = stringResource(R.string.pairing_diverged_use),
                onClick = onUseViewed,
                modifier = Modifier.testTag(TAG_DIVERGED_USE),
            )
        }
        Hairline(color = Fui.Frame)
    }
}

/**
 * What the last thing the person asked for did.
 *
 * One `when` over [Notice], so an outcome added without words for it does not
 * compile. Each carries a label naming the outcome and then the sentence, which
 * is the shape a [Receipt]'s Toast draws too: a Standing Action and a press on
 * this screen are the same operation, and reporting one of them in two idioms
 * would make them look like two.
 *
 * **Six outcomes reach this band, and the two that no longer do are the point.**
 * A plain Offer and a plain Recall confirm and need nothing back, so they are
 * [Receipt]s and go past as a Toast. What is left here all needs something done
 * or known, which is what earns a container that waits to be dismissed — and it
 * is why this band is never the report of a verb that simply worked. Chrome
 * that only ever appears with something in it is chrome nobody has to learn to
 * ignore.
 *
 * [Notice.RecalledFromCache] is the only one that tints its whole band, and that
 * is not decoration: every other notice is a statement about something the
 * person just did, while that one is a warning about the content now on their
 * clipboard — it may be yesterday's link. A refusal is ruled down its left edge
 * instead, in the colour of what to do about it: amber for the two that need
 * something done, inert for `ALREADY HERE`, which is the app working correctly
 * and costs the person nothing.
 *
 * Lives here rather than on one screen because two screens raise notices: a
 * Recall happens on the History and a Pairing is forgotten on the Pairings, and
 * both owe the person the same one sentence in the same place.
 */
@Composable
fun NoticeBanner(notice: Notice, onDismiss: () -> Unit, modifier: Modifier = Modifier) {
    val stale = notice is Notice.RecalledFromCache
    val label = when (notice) {
        is Notice.OfferRefused -> offerRefusalLabel(notice.reason)
        Notice.RecalledFromCache -> R.string.recall_from_cache_badge
        Notice.Unpaired -> R.string.notice_not_paired
        is Notice.HistoryCleared -> R.string.notice_cleared
        is Notice.PairingForgotten -> R.string.notice_forgotten
        is Notice.Failed -> R.string.notice_failed
    }
    val accent = when (notice) {
        Notice.RecalledFromCache -> Accent.Caution
        is Notice.OfferRefused -> offerRefusalAccent(notice.reason)
        is Notice.Failed -> Accent.Caution

        Notice.Unpaired,
        is Notice.HistoryCleared,
        is Notice.PairingForgotten,
        -> Accent.Emitter
    }
    val text = when (notice) {
        is Notice.OfferRefused -> stringResource(offerRefusalMessage(notice.reason))
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
    // A refusal is ruled down its left edge in the colour of what to do about
    // it; an outcome that simply happened is not. Only the stale Recall tints
    // its whole band, because it is the only notice about *what is now on the
    // clipboard* rather than about what the app just did.
    val ruled = notice is Notice.OfferRefused || notice is Notice.Failed
    Row(
        modifier = modifier
            .fillMaxWidth()
            .height(IntrinsicSize.Min)
            .background(if (stale) Fui.AmberA16 else Fui.Band)
            .testTag(if (stale) TAG_NOTICE_STALE else TAG_NOTICE),
    ) {
        if (ruled) Box(Modifier.width(2.dp).fillMaxHeight().background(accent.ink))
        Column(
            modifier = Modifier.weight(1f).padding(Fui.Gutter, 12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            FuiBadge(text = stringResource(label), accent = accent, solid = stale)
            Text(
                text = text,
                style = Fui.Prose,
                color = if (stale) Fui.TextPrimary else Fui.TextBody,
            )
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                FuiButton(
                    text = stringResource(R.string.notice_dismiss),
                    onClick = onDismiss,
                    accent = accent,
                    height = Fui.TargetSmall,
                )
            }
        }
    }
    Hairline(color = if (stale) Fui.AmberA40 else Fui.Hairline)
}

/**
 * The one surprising thing about how this app works, pinned where it cannot
 * scroll away.
 *
 * Sync is foreground only, so something copied on the laptop does not reach the
 * phone until Sharepaste is opened. Left unsaid, that reads as a bug — a person
 * copies a link, picks up their phone, and the History is empty.
 *
 * **It used to be the first item in the list, which meant it was the first thing
 * to leave.** Now it is chrome: one clipped line above the Entries, with the
 * verbatim sentence and the four things that are *not* happening one tap behind
 * [TAG_FOREGROUND_WHY]. A band that is simply there says nothing by appearing,
 * which is what lets it be permanent without becoming a warning. It stays until
 * it is acknowledged, and never merely until it is scrolled past.
 *
 * **This band has two states and they are owned in two different places, which
 * is the argument rather than an accident.** Open/closed is `rememberSaveable`:
 * it changes nothing about the phone, survives a rotation, and putting it in
 * the snapshot would mean the state holder owned a fact about a disclosure
 * triangle. Dismissed goes out through [onDismiss] to the preference store,
 * because it is a decision about what this phone shows from now on, and the
 * caller stops composing the band at all.
 *
 * Which is why only `▴ CLOSE` dismisses. The whole band is the tap target and
 * not the chip in it, so the first tap can do nothing but open: a thumb that
 * brushes chrome it has not read must not thereby delete the app's most
 * important disclosure. The second tap is on a control that says what it does,
 * taken by somebody with the sentence in front of them — and it does not lose
 * the note either, which is what makes a permanent dismissal an honest offer
 * rather than a trap.
 */
@Composable
fun ForegroundOnlyNote(onDismiss: () -> Unit, modifier: Modifier = Modifier) {
    var open by rememberSaveable { mutableStateOf(false) }

    Column(modifier.fillMaxWidth().testTag(TAG_FOREGROUND_NOTE)) {
        // The whole band is the control, not the chip inside it. A 22dp chip in
        // a 30dp band is half of `Fui.TargetSmall`, and the band's argument —
        // that the fact is one tap from anywhere — rests on that tap landing.
        // `FuiButton` is a `Box` and a `clickable`, so nothing hands it
        // Material's minimum interactive size; the band has to be the size.
        ChromeBand(
            height = Fui.TargetSmall,
            background = Fui.Recess,
            modifier = Modifier
                .clickable(
                    // Named only while it is shut. `foreground_only_why_action`
                    // is a sentence about opening the band, and reading it out
                    // over a tap that now closes the band for good would promise
                    // the wrong thing. There is no key for the second action, so
                    // the open band leaves the naming to the `▴ CLOSE` its own
                    // content already reads out.
                    onClickLabel = if (open) null else stringResource(R.string.foreground_only_why_action),
                    role = Role.Button,
                    // Expanding is exploration; closing is acknowledgement. Only
                    // the second of the two is remembered.
                    onClick = {
                        if (open) onDismiss()
                        open = !open
                    },
                )
                .testTag(TAG_FOREGROUND_WHY),
        ) {
            Text("⌾", style = Fui.Micro, color = Fui.Amber400)
            Text(
                text = stringResource(R.string.foreground_only_pinned),
                style = Fui.Micro,
                color = Fui.Amber400,
                maxLines = 1,
                modifier = Modifier.weight(1f).padding(start = 8.dp),
            )
            // The affordance, not the target. Bordered so it reads as pressable,
            // and inert so there is one clickable node in this band rather than
            // two with different sizes.
            Text(
                text = stringResource(
                    if (open) R.string.foreground_only_close else R.string.foreground_only_why,
                ),
                style = Fui.Micro,
                color = Fui.TextEmitter,
                modifier = Modifier.border(1.dp, Fui.Frame).padding(horizontal = 6.dp, vertical = 3.dp),
            )
        }
        if (open) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(Fui.Band)
                    .padding(Fui.Gutter),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(
                    text = stringResource(R.string.foreground_only_note),
                    style = Fui.Prose,
                    color = Fui.TextBody,
                )
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    FuiTag(stringResource(R.string.foreground_only_tag_sync))
                    FuiTag(stringResource(R.string.foreground_only_tag_notification))
                    FuiTag(stringResource(R.string.foreground_only_tag_watching))
                    FuiTag(stringResource(R.string.foreground_only_tag_counterparty))
                }
            }
            Hairline()
        }
    }
}

/**
 * Why the Standing Actions notification is not there.
 *
 * Shown only when the platform is refusing to display it. Deliberately in the
 * **ordinary** voice with no [TAG_FAULT] and no alert colour: nothing is broken.
 * `POST_NOTIFICATIONS` is a runtime grant from API 33 and implicit below it, and
 * on any version a person can switch notifications off in Settings — all of
 * which are choices they are entitled to make. What they are not entitled to is
 * a feature that vanishes without a word, because a notification that never
 * appears looks exactly like one that was never built, and
 * `NotificationManager.notify` reports nothing at all when the permission is
 * denied.
 *
 * The sentence says the two verbs still work from this screen, because that is
 * the part a person cannot see for themselves and the part that decides whether
 * a denial has broken the app.
 */
@Composable
fun StandingActionsBlockedNote(onEnable: () -> Unit, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .background(Fui.Band)
            .padding(Fui.Gutter)
            .testTag(TAG_STANDING_ACTIONS_BLOCKED),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(
            text = stringResource(R.string.standing_actions_blocked_heading),
            style = Fui.Micro,
            color = Fui.TextMuted,
        )
        Text(
            text = stringResource(R.string.standing_actions_blocked),
            style = Fui.Prose,
            color = Fui.TextBody,
        )
        FuiButton(
            text = stringResource(R.string.standing_actions_enable),
            onClick = onEnable,
            height = Fui.TargetSmall,
            modifier = Modifier.testTag(TAG_STANDING_ACTIONS_ENABLE),
        )
    }
}

/**
 * A section heading on a screen that has no cards to head.
 *
 * The emitter's label voice: small, tracked, and never competing with the
 * sentence it introduces.
 */
@Composable
fun SectionHeading(text: String, modifier: Modifier = Modifier) {
    Text(text, style = Fui.Micro, color = Fui.TextEmitter, modifier = modifier)
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

/** The one control a revoked Pairing offers: start over. */
const val TAG_PAIR_AGAIN = "contact-pair-again"

const val TAG_FOREGROUND_NOTE = "foreground-only-note"

/**
 * The disclosure that opens the pinned fact in full.
 *
 * Its own tag because the band's whole argument is that the sentence is
 * *reachable without scrolling* — one tap, from chrome, on any screenful of
 * Entries. A test presses this rather than scrolling to a list item, which is
 * the difference the redesign made.
 */
const val TAG_FOREGROUND_WHY = "foreground-only-why"

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
