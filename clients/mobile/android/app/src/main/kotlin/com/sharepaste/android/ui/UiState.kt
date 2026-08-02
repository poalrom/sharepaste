package com.sharepaste.android.ui

import androidx.annotation.StringRes
import com.sharepaste.android.R
import com.sharepaste.android.scan.CameraProblem
import com.sharepaste.core.ConnectionState
import com.sharepaste.core.Entry
import com.sharepaste.core.PairingSummary
import com.sharepaste.core.SkipReason

/**
 * Everything on screen, as one value.
 *
 * One immutable snapshot rather than a scatter of `mutableStateOf` fields, so
 * that a screen renders a state it was handed and cannot invent one, and so the
 * whole surface can be asserted in a test without a device in a particular mood.
 * Tickets 11 and 12 add fields here — the Viewed Pairing, the Standing Actions —
 * rather than growing a second state holder beside this one.
 */
data class UiState(
    val screen: Screen = Screen.Pairing,
    val session: SessionPhase = SessionPhase.Unpaired,
    val pairing: PairingState = PairingState(),
    /**
     * Whether the app is in front, as the activity last reported it.
     *
     * On screen indirectly, and not bookkeeping: the same `DISCONNECTED` from
     * the core means "not in contact, we are looking" in the foreground and
     * "resting, because we put it down" in the background, and those are
     * different sentences to read. Both are nominal.
     */
    val foreground: Boolean = false,
    /**
     * The cached Entries for the Active Pairing, newest first.
     *
     * The Preview on each one is the facade's, already normalised to a single
     * line with its control characters turned to spaces and capped at 80
     * characters. **It is not re-derived here**, and neither is
     * [Entry.undecryptable]: the desktop inferred that flag from an empty
     * Preview in four places and ticket 06 spent its time removing them, because
     * an Entry whose plaintext is genuinely empty is indistinguishable from one
     * this device holds no key for to anything guessing.
     */
    val entries: List<Entry> = emptyList(),
    /**
     * The Active Pairing this screen is showing.
     *
     * Held here rather than read back out of [session], because a phase does not
     * always have one: [SessionPhase.Looking] is entered before the Pairing being
     * resumed is known. Deriving the Pairing from the phase meant dropping any
     * event that arrived during that window — and the uploader flushes the pending
     * queue well before the Relay says it is online, so the count that mattered
     * most was exactly the one that went missing.
     */
    val activeUserId: String? = null,
    /**
     * This device's own Device id on the Active Pairing.
     *
     * Present so that Origin can be *absent*. An Origin is "the device an Entry
     * was captured on, as distinct from the device viewing it" (CONTEXT.md), so
     * stamping the phone's own name on the rows it produced itself would be
     * noise on most of the list. `null` until the Pairing has been read back,
     * which shows every row's Origin — the safe direction, since the other one
     * hides a real one.
     */
    val ownDeviceId: String? = null,
    /**
     * Entries captured here that are not on the Relay yet.
     *
     * Surfaced rather than kept as bookkeeping. Sync is foreground-only (ADR
     * 0007), so an Offer made with no connection sits in the queue until the app
     * is next opened — and a queue nobody can see is a queue nobody knows to
     * come back for.
     */
    val pending: Long = 0,
    /** What the last thing the person asked for did. Nothing, before they ask. */
    val notice: Notice? = null,
    /**
     * Every Pairing this phone holds, as the facade last listed them.
     *
     * The whole list rather than the Active one alone, because a phone may hold
     * several and each carries a fact no other surface shows: its own
     * [PairingSummary.pending] queue. A Pairing the device has switched away from
     * still holds Entries it never uploaded, and this list is the single place
     * they are visible at all.
     */
    val pairings: List<PairingSummary> = emptyList(),
    /**
     * The Viewed Pairing, when the person has chosen one that is not the Active
     * one. `null` means "whichever one this phone is syncing".
     *
     * A **transient view choice**: switching it changes nothing about syncing or
     * capture, and it is forgotten when the app is put down (CONTEXT.md — the
     * desktop's rule is "forgotten when the window closes", and a phone's
     * equivalent of closing the window is `onStop`). Held as an override of
     * [activeUserId] rather than as a value of its own precisely so that
     * "forgotten" is the absence of a value and not a second thing to reset;
     * nothing persists it, and there is no settings row it could leak into.
     */
    val viewedUserId: String? = null,
    /** The one destructive action waiting to be confirmed, if any. */
    val confirming: Confirmation? = null,
    /**
     * Whether the platform is refusing to show the Standing Actions
     * notification.
     *
     * On screen, not bookkeeping — and it is the one fact about this feature
     * that a person cannot discover for themselves. A notification that is
     * simply absent looks exactly like a feature that was never built, and
     * `NotificationManager.notify` reports nothing when the permission is
     * denied. So the app says it: the two verbs are unreachable from outside,
     * everything on this screen still works, and here is how to get them back.
     *
     * Covers both variants across the supported floor with one flag, because
     * both have one consequence. `POST_NOTIFICATIONS` is a runtime grant from
     * API 33 and implicit below it — but on any version a person can switch
     * this app's notifications off in Settings, and the app must not be broken
     * by either.
     */
    val standingActionsBlocked: Boolean = false,
    /**
     * Whether a Recall says what it put on the clipboard.
     *
     * The phone's first real preference, and the only one it has that the
     * desktop does not. Off means no Recall [Receipt] at all — the Entry still
     * reaches the clipboard, and the six [Notice]s are untouched, because this
     * switch is about being told and not about being warned.
     *
     * Persisted, so it arrives here from
     * [com.sharepaste.android.platform.UiPreferences] rather than from a core
     * event. It is in the snapshot for the ordinary reason everything is: the
     * Settings Screen draws a switch from it.
     */
    val showRecalled: Boolean = true,
    /**
     * Whether the History Screen's foreground-only band has been closed for
     * good.
     *
     * Not the band's open/closed state, which is a `rememberSaveable` inside the
     * band: expanding it is exploration, and only `▴ CLOSE` is acknowledgement.
     * Dismissing does not lose the disclosure — it is on the Settings Screen at
     * full length, which is the whole reason a dismissal is allowed to persist.
     */
    val foregroundNoteDismissed: Boolean = false,
    /**
     * What has been typed into the Filter, verbatim.
     *
     * Held here and not in the band's own `remember` for the reason everything
     * else is: it survives a rotation with the rest of the snapshot. It is
     * cleared on a Viewed Pairing switch and when the app goes to the back —
     * the desktop's rule (`store/ui.ts`), ported — because a needle left over
     * from the last visit is a needle silently hiding rows.
     *
     * Untrimmed, because it is what the person typed and the field draws it
     * back. Trimming happens once, in [shown], where the needle is made.
     */
    val filter: String = "",
    /**
     * The last real scan's answer, and the needle it answered.
     *
     * Written only by [SharepasteViewModel]'s filtering job and read only
     * through [shown], which is the reason it can be a frame behind without
     * anything on screen disagreeing with itself.
     */
    val scanned: Filtered = Filtered(),
) {
    /**
     * The Pairing whose History is on screen. Defaults to the Active one.
     *
     * Every read of the list — the rows, the Origin rule, which `EntryAdded` is
     * ours — goes through this rather than through [activeUserId], because the
     * two differ exactly when it matters most.
     */
    val viewedPairing: String? get() = viewedUserId ?: activeUserId

    /**
     * Whether the History on screen belongs to a Pairing this phone is not
     * syncing.
     *
     * The condition a band has to state out loud. Without it the list shows one
     * Pairing, the device syncs another, and nothing on screen admits it.
     */
    val diverged: Boolean get() = activeUserId != null && viewedUserId != null && viewedUserId != activeUserId

    /** How a Pairing reads in a sentence: its User's name, or its id. */
    fun nameOf(userId: String?): String {
        if (userId == null) return ""
        return pairings.firstOrNull { it.userId == userId }?.let { it.username ?: it.userId } ?: userId
    }

    /**
     * The rows on screen, and the needle that left them.
     *
     * The screen reads this and never [entries]: the list, the `n/m` count and
     * the empty-state branch all resolve from one value that carries the query
     * it answers, so no frame can show one needle's rows under another's count.
     *
     * **Two of the three answers need no scan, and they are the two that
     * matter.** A blank needle is every Entry and an empty History is no
     * Entries, so both are known here without reading a single `plaintext` —
     * and every path that could otherwise mislead goes through one of them.
     * Switching the Viewed Pairing and going to the back clear the needle;
     * clearing a History and forgetting the last Pairing empty the list. So no
     * frame can put one Pairing's rows under another's name, and this holds by
     * construction rather than by every future edit to [entries] remembering
     * there is a second field to keep up.
     *
     * The third answer is a real scan, and that is the one case this hands back
     * [scanned] instead: computing it here would run it on whichever thread read
     * the property, which is the main one, and a hundred Entries is 6.4 MiB at
     * the 64 KB cap. [SharepasteViewModel] runs it on `Dispatchers.Default` and
     * writes it back. Between the keystroke and the answer this is one frame
     * stale — and says so, because [Filtered.needle] is the needle it answered
     * rather than the one in the field.
     */
    val shown: Filtered
        get() {
            val needle = filter.trim()
            return if (needle.isEmpty() || entries.isEmpty()) filtered(needle, entries) else scanned
        }
}

/**
 * The Entries a needle left, and the needle that left them.
 *
 * One pair rather than two fields, because a count that disagrees with the rows
 * it counts is the whole failure mode of filtering off the main thread. An empty
 * [needle] means no Filter is on: [entries] is then the whole History, and
 * `NO MATCHES` is not a thing that can be said about it.
 */
data class Filtered(val needle: String = "", val entries: List<Entry> = emptyList())

/**
 * The one predicate, so a phone cannot answer a needle two ways.
 *
 * Matches the whole [Entry.plaintext] and not the [Entry.preview]: the Preview
 * is capped at 80 characters, which truncates exactly the long Entries a Filter
 * exists to tell apart. The needle arrives trimmed and the haystack does not —
 * leading whitespace is part of what was copied, and a trailing space nobody can
 * see must not empty the list.
 *
 * `ignoreCase` rather than lowercasing both sides: `lowercase()` is
 * locale-sensitive, and on a Turkish locale it turns `ID` into `ıd` and stops it
 * matching `id`.
 *
 * An **Undecryptable** Entry carries no plaintext and so matches nothing, which
 * is the truth about it rather than an omission.
 */
fun filtered(needle: String, entries: List<Entry>): Filtered =
    if (needle.isEmpty()) {
        Filtered(needle, entries)
    } else {
        Filtered(needle, entries.filter { it.plaintext?.contains(needle, ignoreCase = true) == true })
    }

/**
 * A destructive action that has been asked for and not yet agreed to.
 *
 * In [UiState] rather than in a composable's `remember`, for the same reason
 * everything else here is: the whole surface stays one immutable snapshot, and a
 * confirmation strip is exactly the thing a test needs to read before it presses
 * the button that cannot be undone.
 *
 * Both name a Pairing, because both are only meaningful about one — and because
 * the naming is the point. Clearing a History that belongs to a Pairing the
 * person is not looking at, without saying which, is how the wrong History gets
 * erased.
 */
sealed interface Confirmation {

    val userId: String

    /** Erase every Entry of this Pairing, on the Relay and everywhere. */
    data class ClearHistory(override val userId: String) : Confirmation

    /** Erase this Pairing: its Entries, its key material and its token. */
    data class Forget(override val userId: String) : Confirmation
}

/**
 * The one sentence the app owes the person about something they now have to act
 * on, or at least know.
 *
 * A value in [UiState] rather than a snackbar raised from a coroutine, for the
 * same reason the rest of this is: what is on screen stays one immutable
 * snapshot, so every sentence can be asserted without a device in a particular
 * mood. The sealed hierarchy is what makes rendering exhaustive — an outcome
 * added without words for it will not compile.
 *
 * **Five variants, and the three that left are the point.** A plain Offer and a
 * plain Recall confirm and need nothing back, so they are [Receipt]s and reach
 * the person as a Toast; every one of these needs something done or known, and
 * a band that persists until it is dismissed is what that difference looks like.
 * The third was `RecalledFromCache`, and it left for a different reason: the
 * verb that raised it stopped fetching (ADR 0010), so there is no round trip
 * left to fall back from. The Standing Actions' Recall still fetches and still
 * says so, on a surface with no band — see `StandingActionActivity`.
 */
sealed interface Notice {

    /**
     * An Offered Capture was refused, and the reason has to be readable.
     *
     * The reason is the core's, from the one capture filter both clients share —
     * see [offerRefusalMessage] for the words and for which reasons can arrive.
     */
    data class OfferRefused(val reason: SkipReason) : Notice

    /** Nothing is paired, so there is nothing to Offer to or Recall from. */
    data object Unpaired : Notice

    /**
     * A Pairing's History was erased, and the sentence names which one.
     *
     * A phone may hold several Pairings and only one of them is on screen, so a
     * bare "cleared" leaves the person to work out what they just lost.
     */
    data class HistoryCleared(val pairing: String) : Notice

    /**
     * A Pairing is gone from this phone: its Entries, its key material and its
     * token.
     *
     * [promoted] is the Pairing the core moved this device onto afterwards, or
     * `null` when that was the last one. Reported rather than left to be
     * noticed: forgetting one Pairing silently changing what the phone syncs is
     * precisely the surprise this ticket exists to remove.
     */
    data class PairingForgotten(val pairing: String, val promoted: String?) : Notice

    /**
     * It did not work, in the app's words with the core's underneath.
     *
     * [detail] is the core's own sentence where the core had one worth
     * repeating — a refused cleartext Relay names the Relay and the reason, and
     * no wording here could be that specific.
     */
    data class Failed(@param:StringRes val message: Int, val detail: String? = null) : Notice
}

/**
 * Each refusal an Offer can actually receive, in its own words.
 *
 * Two of the six [SkipReason]s are reachable through an Offer, and each one
 * needs its own sentence because each needs a different thing done about it:
 * put something else on the clipboard, or send something smaller. A repeat
 * copy was the third until ADR 0012 stopped calling it a refusal — it is a Use
 * of the Entry this device already holds, and it draws [Receipt.Recognised]
 * instead of anything here.
 *
 * The other four describe Watched Capture, which a phone never performs (ADR
 * 0007) — the facade passes their inputs in inert, so they are unreachable by
 * construction. They share one sentence rather than four invented ones: copy
 * that can never be read is copy nobody keeps true, and it is deliberately
 * worded as the surprise it would be. Omitting them altogether is not an option
 * the compiler allows, which is the point of the exhaustive `when`.
 */
@StringRes
fun offerRefusalMessage(reason: SkipReason): Int = when (reason) {
    SkipReason.NON_TEXT -> R.string.offer_refused_non_text
    SkipReason.TOO_LARGE -> R.string.offer_refused_too_large

    SkipReason.DISABLED,
    SkipReason.DENY_LIST,
    SkipReason.SELF_WRITE,
    SkipReason.TRANSIENT,
    -> R.string.offer_refused_unreachable
}

/**
 * The same refusal in one or two words, for the label above the sentence.
 *
 * Not a shortening of the sentence: it names *what to do about it*, which is the
 * only reason the two reachable reasons are two reasons.
 */
@StringRes
fun offerRefusalLabel(reason: SkipReason): Int = when (reason) {
    SkipReason.NON_TEXT -> R.string.notice_nothing_to_send
    SkipReason.TOO_LARGE -> R.string.notice_too_big

    SkipReason.DISABLED,
    SkipReason.DENY_LIST,
    SkipReason.SELF_WRITE,
    SkipReason.TRANSIENT,
    -> R.string.notice_refused
}

/**
 * Which screen is in front.
 *
 * Not a navigation library. There are two destinations and the choice between
 * them is a fact about the data — a phone with no Pairing has nothing to show —
 * so a graph, a back stack and a route DSL would all be scaffolding around a
 * `when`. Ticket 11's Pairings screen is a third entry here.
 */
enum class Screen {
    /** No Pairing yet, or the person asked to add one. */
    Pairing,

    /** The Entries of the Viewed Pairing. */
    History,

    /** Every Pairing this phone holds, and the settings a phone actually has. */
    Pairings,
}

/**
 * What the phone can say about its own Contact with the Relay.
 *
 * The desktop shows relay health only when it is degraded (ADR 0002). That rule
 * inverts here, because a phone is out of contact almost all of the time: sync
 * is foreground-only (ADR 0007), so "not in contact" is the *nominal* reading
 * and painting it as a fault would mark a perfectly healthy phone permanently
 * broken. Only [Refused] is a fault — see [toneOf].
 */
sealed interface SessionPhase {

    /** Nothing is paired to this phone. */
    data object Unpaired : SessionPhase

    /**
     * Foreground, and the session is coming up. Neither good nor bad news yet,
     * which is why it reads as an activity rather than as a status.
     */
    data object Looking : SessionPhase

    /** Foreground and in Contact. */
    data class InContact(val userId: String) : SessionPhase

    /** Foreground, paired, not in Contact. Nominal. */
    data class OutOfContact(val userId: String) : SessionPhase

    /**
     * Backgrounded, so the session was taken down on purpose.
     *
     * A Pairing that is merely not active and disconnected is resting, not
     * faulty — this is the state `onStop` leaves behind, and it is nominal.
     */
    data class Resting(val userId: String) : SessionPhase

    /**
     * A Pairing this phone holds but is not syncing.
     *
     * Distinct from [Resting], which is the *phone* put down, because the
     * sentence is different: a Pairing that is merely not the Active one is idle
     * on a phone that is wide awake, and telling someone "Sharepaste is not
     * looking while it is closed" about it would be false. Both are nominal, and
     * that is the whole point — a Pairing nobody asked to connect is resting,
     * not faulty, and must not read as an error.
     */
    data class NotActive(val userId: String) : SessionPhase

    /**
     * The Relay turned this device's token away.
     *
     * The one genuine fault a phone can be in: no amount of waiting or
     * reconnecting fixes a revoked Pairing, and the person has to pair again.
     */
    data class Refused(val userId: String, val detail: String?) : SessionPhase
}

/**
 * Whether a phase is ordinary news or something the person has to act on.
 *
 * Exhaustive over [SessionPhase] on purpose: adding a phase without deciding
 * which of the two it is becomes a compile error, which is the only way this
 * rule survives the tickets built on top of it. It is the **only** statement of
 * that rule — `Signal` in `Fui.kt` chooses a lamp colour and defers its alert
 * arm to this, rather than re-enumerating which phases are faults.
 */
enum class Tone {
    /**
     * Say it in the ordinary voice.
     *
     * A nominal phase does get a lit status light — the redesign made Contact a
     * permanent readout — but never an alert colour, never a container, and
     * never a call to action. Three of the phases here light `Signal.Standby`
     * and only being *in contact* lights `Signal.Nominal`, so the two words
     * called Nominal are not the same set.
     */
    Nominal,

    /** Something is actually wrong and the person has to act. */
    Fault,
}

fun toneOf(phase: SessionPhase): Tone = when (phase) {
    SessionPhase.Unpaired,
    SessionPhase.Looking,
    is SessionPhase.InContact,
    is SessionPhase.OutOfContact,
    is SessionPhase.Resting,
    is SessionPhase.NotActive,
    -> Tone.Nominal

    is SessionPhase.Refused -> Tone.Fault
}

/**
 * What one Pairing's card says about itself.
 *
 * A Pairing that is not the Active one holds no session, so whatever the core
 * last read off the wire for it is stale by construction — and it stays stale,
 * because ticket 05 keeps the last Contact reading across a teardown on purpose.
 * Rendering that reading would put "In contact with the Relay" on a card nothing
 * is connected for, which is the same mistake in a different place as the late
 * `ConnectionState` frame ticket 10 fixed. So: **anything that is not the Active
 * Pairing of a phone that is in front is [SessionPhase.NotActive]**, and it is
 * nominal.
 *
 * A revoked token is the one exception and is reported either way. No amount of
 * not being connected fixes it, and it is the only thing on this screen a person
 * has to act on.
 */
fun pairingPhase(pairing: PairingSummary, foreground: Boolean): SessionPhase {
    if (pairing.status == ConnectionState.AUTH_FAILED) {
        return SessionPhase.Refused(pairing.userId, null)
    }
    if (!pairing.isActive) return SessionPhase.NotActive(pairing.userId)
    if (!foreground) return SessionPhase.Resting(pairing.userId)
    return when (pairing.status) {
        ConnectionState.ONLINE -> SessionPhase.InContact(pairing.userId)
        ConnectionState.CONNECTING -> SessionPhase.Looking
        ConnectionState.DISCONNECTED -> SessionPhase.OutOfContact(pairing.userId)
        // Answered above. Repeated rather than swept up by an `else`, so a
        // reading added to the core arrives here as a compile error.
        ConnectionState.AUTH_FAILED -> SessionPhase.Refused(pairing.userId, null)
    }
}

/**
 * The pairing flow.
 *
 * [deviceLabel] starts **empty** and stays empty until someone types something.
 * The desktop's flow hard-codes a default; copying that would put a machine's
 * guess on a person's own device, in a list they have to read later. Pairing is
 * blocked while it is blank, which is what makes the choice theirs rather than a
 * suggestion they can walk past.
 *
 * **[code] is one field with two ways of filling it, and a scan is one of them.**
 * It lives here rather than inside the screen because the camera and the keyboard
 * write to the same place: a scan puts the code it read into the field and stops
 * there. It does *not* pair. Somebody who opens this screen scans before they
 * read anything — the square is the only thing on it that looks like an
 * instruction — and a scan that paired would fail on the empty name and throw the
 * code away, which is a two-minute code spent on a message.
 *
 * [scanned] says the code in the field came off the camera, which is why the
 * viewfinder is no longer on screen. Emptying the field brings it back: that is
 * the way to scan a second code, and it is the only way, because a control for it
 * would sit beside the field it duplicates.
 */
data class PairingState(
    val deviceLabel: String = "",
    val code: String = "",
    val scanned: Boolean = false,
    val camera: CameraProblem? = null,
    val attempt: PairAttempt = PairAttempt.Idle,
) {
    /** Whether the code is worth sending. Ticket 09's one gate on the label. */
    val canPair: Boolean
        get() = deviceLabel.isNotBlank() && code.isNotBlank() && attempt !is PairAttempt.Working

    /**
     * The flow as it should be arrived at: this phone's name, and nothing else.
     *
     * The name outlives one pairing because it names the phone rather than the
     * Pairing; a code, a scan and a failure do not, and a screen that opened
     * holding a spent code would offer to send it again. The camera goes too:
     * whichever of the three states applies is re-read the moment the screen is
     * composed, and a remembered one would be a guess with a head start.
     */
    fun restarted() = PairingState(deviceLabel = deviceLabel)
}

sealed interface PairAttempt {

    data object Idle : PairAttempt

    /** A code is in flight. The relay has 120 seconds; this usually takes one. */
    data object Working : PairAttempt

    /**
     * It did not work, and this says which of the several ways.
     *
     * [detail] carries the core's own sentence when the core had one worth
     * repeating — `InsecureRelay` names the relay and the reason, which no
     * generic wording here could. Everything else reads better in the app's
     * voice than in the protocol's.
     */
    data class Failed(val message: Int, val detail: String? = null) : PairAttempt
}
