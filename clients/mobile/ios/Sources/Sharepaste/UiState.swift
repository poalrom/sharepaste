import Foundation
import SharepasteCore

/// Which screen is in front.
///
/// Not a navigation library, and not a `NavigationStack`. There are three
/// destinations and the choice between them is a fact about the data — a phone
/// with no Pairing has nothing to show — so a graph, a back stack and a route
/// type would all be scaffolding around a `switch`.
///
/// **The left-edge swipe therefore does nothing, and that is spec row 29 rather
/// than an omission.** Android added a `BackHandler` to every screen in `0.5.0`,
/// firing the same action as its on-screen `◂`. The literal port is empty here
/// because iOS has no system Back button; the near-port is a `NavigationStack`,
/// which would make the gesture work and is declined. It would put two sources
/// of truth on screen about which screen is on screen — the stack's path and
/// this value — on the one client with no automated UI defence (spec row 10).
/// Android rejected a back stack for the same reason with 116 instrumented tests
/// to catch the drift; here there are none.
///
/// The `◂` still reaches everywhere Android's back does: Settings to History,
/// the pairing flow to Settings when a Pairing exists. What is lost is the
/// gesture, not a route.
enum Screen {
    /// No Pairing yet, or the person asked to add one.
    case pairing

    /// The Entries of the Viewed Pairing.
    case history

    /// Everything this phone can be told, of which the Pairings are one section.
    ///
    /// Named for the destination rather than for a section of it. Android kept
    /// `Screen.Pairings` after retitling the screen, because renaming a symbol
    /// nobody reads is churn; iOS has nothing to keep, and the moment a screen
    /// named after one of its sections grows a second one, anybody looking for a
    /// switch is sent to a screen that does not exist.
    case settings
}

/// The two reasons a phone cannot scan, kept apart.
///
/// They are not interchangeable and must not share a message: "turn camera
/// access on" is useless advice on a device with no camera, and "this phone has
/// no camera" is a lie when the person simply said no to a permission prompt.
enum CameraProblem {
    /// Someone declined the camera permission, or a policy declines it for them.
    case permissionRefused

    /// There is no camera on this device Sharepaste could use.
    case noCamera
}

/// Which camera problem applies, or `nil` when scanning can go ahead.
///
/// Order matters. Absent hardware wins over a refused permission, because a
/// device with no camera also has no permission granted, and the *useful* thing
/// to say is the one the person can act on.
func cameraProblem(hasCamera: Bool, permissionGranted: Bool) -> CameraProblem? {
    if !hasCamera { return .noCamera }
    if !permissionGranted { return .permissionRefused }
    return nil
}

/// Everything on screen, as one value.
///
/// One immutable snapshot rather than a scatter of `@State` fields, so that a
/// screen renders a state it was handed and cannot invent one, and so the whole
/// surface can be read back without a device in a particular mood.
struct UiState {
    var screen: Screen = .pairing
    var session: SessionPhase = .unpaired
    var pairing = PairingState()

    /// Whether the app is in front, as the scene last reported it.
    ///
    /// On screen indirectly, and not bookkeeping: the same `disconnected` from
    /// the core means "not in contact, we are looking" in the foreground and
    /// "resting, because we put it down" in the background, and those are
    /// different sentences to read. Both are nominal.
    var foreground = false

    /// The cached Entries for the Viewed Pairing, newest first.
    ///
    /// The Preview on each one is the facade's, already normalised to a single
    /// line with its control characters turned to spaces and capped at 80
    /// characters. **It is not re-derived here**, and neither is
    /// `Entry.undecryptable`: an Entry whose plaintext is genuinely empty is
    /// indistinguishable from one this device holds no key for to anything
    /// guessing from an empty Preview.
    var entries: [Entry] = []

    /// The Active Pairing.
    ///
    /// Held here rather than read back out of ``session``, because a phase does
    /// not always have one: ``SessionPhase/looking`` is entered before the
    /// Pairing being resumed is known, and deriving the Pairing from the phase
    /// meant dropping any event that arrived during that window — including the
    /// pending count, which the uploader flushes well before the Relay says it is
    /// online.
    var activeUserId: String?

    /// This device's own Device id on the Viewed Pairing.
    ///
    /// Present so that Origin can be *absent*. An Origin is "the device an Entry
    /// was captured on, as distinct from the device viewing it" (`CONTEXT.md`),
    /// so stamping the phone's own name on the rows it produced itself would be
    /// noise on most of the list. `nil` until the Pairing has been read back,
    /// which shows every row's Origin — the safe direction, since the other one
    /// hides a real one.
    var ownDeviceId: String?

    /// Entries captured here that are not on the Relay yet.
    ///
    /// Surfaced rather than kept as bookkeeping. Sync is foreground-only, so an
    /// Offer made with no connection sits in the queue until the app is next
    /// opened — and a queue nobody can see is a queue nobody knows to come back
    /// for.
    var pending: Int64 = 0

    /// What the last thing the person asked for did, when it needs acting on.
    var notice: Notice?

    /// Every Pairing this phone holds, as the facade last listed them.
    var pairings: [PairingSummary] = []

    /// The Viewed Pairing, when the person has chosen one that is not the Active
    /// one. `nil` means "whichever one this phone is syncing".
    ///
    /// A **transient view choice**: switching it changes nothing about syncing or
    /// capture, and it is forgotten when the app is put down. Held as an override
    /// of ``activeUserId`` rather than as a value of its own precisely so that
    /// "forgotten" is the absence of a value and not a second thing to reset.
    var viewedUserId: String?

    /// The one destructive action waiting to be confirmed, if any.
    var confirming: Confirmation?

    /// Whether a Recall says what it put on the pasteboard. ADR 0009.
    ///
    /// Off means no Recall Receipt at all — the Entry still reaches the
    /// pasteboard, and the six Notices are untouched, because this switch is
    /// about being told and not about being warned. It arrives here from
    /// ``UiPreferences`` rather than from a core event.
    var showRecalled = true

    /// Whether the History Screen's foreground-only band has been closed for
    /// good.
    ///
    /// Not the band's open/closed state, which is view state inside the band:
    /// expanding it is exploration, and only `▴ CLOSE` is acknowledgement.
    var foregroundNoteDismissed = false

    /// The Pairing whose History is on screen. Defaults to the Active one.
    ///
    /// Every read of the list — the rows, the Origin rule, which `entryAdded` is
    /// ours — goes through this rather than through ``activeUserId``, because the
    /// two differ exactly when it matters most.
    var viewedPairing: String? { viewedUserId ?? activeUserId }

    /// Whether the History on screen belongs to a Pairing this phone is not
    /// syncing.
    ///
    /// The condition a band has to state out loud. Without it the list shows one
    /// Pairing, the device syncs another, and nothing on screen admits it.
    var diverged: Bool {
        activeUserId != nil && viewedUserId != nil && viewedUserId != activeUserId
    }

    /// How a Pairing reads in a sentence: its User's name, or its id.
    func nameOf(_ userId: String?) -> String {
        guard let userId else { return "" }
        guard let pairing = pairings.first(where: { $0.userId == userId }) else { return userId }
        return pairing.username ?? pairing.userId
    }

    /// The Pairing the identity band names, and what goes in its User slot.
    ///
    /// `…` until the Relay's `/me` mirror answers, so a 36-character uuid never
    /// reaches the screen on a cold start. Nothing else about the band moves.
    func identityUser(_ userId: String?) -> String {
        guard let userId,
              let pairing = pairings.first(where: { $0.userId == userId }),
              let username = pairing.username,
              !username.isEmpty
        else { return Strings.historyIdentityUnknown }
        return username
    }
}

/// A destructive action that has been asked for and not yet agreed to.
///
/// In ``UiState`` rather than in a view's `@State`, for the same reason
/// everything else here is: the whole surface stays one immutable snapshot, and
/// a confirmation strip is exactly the thing to read before pressing the button
/// that cannot be undone.
///
/// Both name a Pairing, because both are only meaningful about one — and because
/// the naming is the point. Clearing a History that belongs to a Pairing the
/// person is not looking at, without saying which, is how the wrong History gets
/// erased.
enum Confirmation: Equatable {
    /// Erase every Entry of this Pairing, on the Relay and everywhere.
    case clearHistory(userId: String)

    /// Erase this Pairing: its Entries, its key material and its token.
    case forget(userId: String)

    var userId: String {
        switch self {
        case let .clearHistory(userId), let .forget(userId): userId
        }
    }
}

/// The one sentence the app owes the person about something they now have to act
/// on, or at least know.
///
/// **Six variants, and the two that are not here are the point.** A plain Offer
/// and a plain Recall confirm and need nothing back, so they are ``Receipt``s and
/// vanish; every one of these needs something done or known, and a band that
/// persists until it is dismissed is what that difference looks like.
/// ``recalledFromCache`` is the variant that keeps the line honest — it is the
/// plausible seventh Receipt and ADR 0007 says it may never be silent.
enum Notice: Equatable {
    /// An Offered Capture was refused, and the reason has to be readable.
    case offerRefused(reason: SkipReason)

    /// The newest **cached** Entry is on the pasteboard, because the Relay could
    /// not be reached.
    ///
    /// Recall Latest always attempts the round trip; when the round trip fails,
    /// the honest answer is still the best one available, but it may be
    /// yesterday's link and the person is the only one who can tell.
    case recalledFromCache

    /// Nothing is paired, so there is nothing to Offer to or Recall from.
    case unpaired

    /// A Pairing's History was erased, and the sentence names which one.
    case historyCleared(pairing: String)

    /// A Pairing is gone from this phone: its Entries, its key material and its
    /// token. `promoted` is the Pairing the core moved this device onto
    /// afterwards, or `nil` when that was the last one.
    case pairingForgotten(pairing: String, promoted: String?)

    /// It did not work, in the app's words with the core's underneath.
    ///
    /// `detail` is the core's own sentence where the core had one worth
    /// repeating — a refused cleartext Relay names the Relay and the reason, and
    /// no wording here could be that specific.
    case failed(message: String, detail: String? = nil)
}

/// Confirmation that a verb did what was asked, needing nothing back.
///
/// Transient. It replaces itself and it goes away on its own, which is the
/// difference from a ``Notice``: a Receipt says a thing happened, a Notice says
/// something needs doing and waits to be dismissed.
///
/// **``recalled`` is the only variant that may ever carry an Entry's text**, and
/// that is enforced by the shape rather than by a comment: the other variant
/// carries a number. Android has a third, `Aloud`, for a Notice said out loud by
/// a Standing Action that has no band to put it in; iOS has no equivalent and
/// must not grow one — an App Intent reports through its own return value and
/// its own error, and ticket 07 records why that divergence is the platform's
/// rather than a shortcut taken.
enum Receipt: Equatable {
    /// An Offered Capture was taken. `pending` is the queue depth after it.
    ///
    /// It names no Preview, deliberately: the person supplied that content a
    /// second ago, and only a Recall hands back something they did not choose.
    case offered(pending: Int64)

    /// An Entry is on this device's pasteboard, and — when it can be said —
    /// which one.
    ///
    /// **Nullable, and one variant rather than two.** A Preview can genuinely be
    /// missing — an Undecryptable Entry has none, and the read that fetches it
    /// can fail — but "the Recall was confirmed" is the same outcome either way,
    /// and `SHOW WHAT WAS RECALLED` has to silence both. Splitting the two cases
    /// across variants is exactly how one of them escaped that switch on Android:
    /// a guard written against a type has to be able to name the whole of what it
    /// guards.
    case recalled(preview: String?)

    /// The outcome in a word or so, over the sentence.
    var label: String {
        switch self {
        case .offered: Strings.noticeOffered
        case .recalled: Strings.noticeRecalled
        }
    }

    /// The sentence. The one place a Preview is read into words.
    ///
    /// A Recall with no Preview to name says only that something is on the
    /// pasteboard, rather than promising a name and leaving the slot empty.
    var sentence: String {
        switch self {
        case .offered:
            Strings.offerQueued
        case let .recalled(preview):
            if let preview, !preview.trimmingCharacters(in: .whitespaces).isEmpty {
                Strings.receiptRecalled(preview: preview)
            } else {
                Strings.recallDone
            }
        }
    }
}

/// Each refusal an Offer can actually receive, in its own words.
///
/// Three of the seven `SkipReason`s are reachable through an Offer, and each one
/// needs its own sentence because each needs a different thing done about it: put
/// something else on the clipboard, send something smaller, or nothing at all
/// because it is already here.
///
/// The other four describe Watched Capture, which a phone never performs (ADR
/// 0007) — the facade passes their inputs in inert, so they are unreachable by
/// construction. They share one sentence rather than four invented ones: copy
/// that can never be read is copy nobody keeps true. Omitting them altogether is
/// not an option the compiler allows, which is the point of the exhaustive
/// `switch`.
func offerRefusalMessage(_ reason: SkipReason) -> String {
    switch reason {
    case .nonText: Strings.offerRefusedNonText
    case .tooLarge: Strings.offerRefusedTooLarge
    case .duplicate: Strings.offerRefusedDuplicate
    case .disabled, .denyList, .selfWrite, .transient: Strings.offerRefusedUnreachable
    }
}

/// The same refusal in one or two words, for the label above the sentence.
///
/// Not a shortening of the sentence: it names *what to do about it*, which is the
/// only reason the three reachable reasons are three reasons.
func offerRefusalLabel(_ reason: SkipReason) -> String {
    switch reason {
    case .nonText: Strings.noticeNothingToSend
    case .tooLarge: Strings.noticeTooBig
    case .duplicate: Strings.noticeAlreadyHere
    case .disabled, .denyList, .selfWrite, .transient: Strings.noticeRefused
    }
}

/// What the phone can say about its own Contact with the Relay.
///
/// The desktop shows relay health only when it is degraded (ADR 0002). That rule
/// inverts here, because a phone is out of contact almost all of the time: sync
/// is foreground-only (ADR 0007), so "not in contact" is the *nominal* reading
/// and painting it as a fault would mark a perfectly healthy phone permanently
/// broken. Only ``refused`` is a fault — see ``toneOf(_:)``.
enum SessionPhase: Equatable {
    /// Nothing is paired to this phone.
    case unpaired

    /// Foreground, and the session is coming up. Neither good nor bad news yet,
    /// which is why it reads as an activity rather than as a status.
    case looking

    /// Foreground and in Contact.
    case inContact(userId: String)

    /// Foreground, paired, not in Contact. Nominal.
    case outOfContact(userId: String)

    /// Backgrounded, so the session was taken down on purpose.
    case resting(userId: String)

    /// A Pairing this phone holds but is not syncing.
    ///
    /// Distinct from ``resting``, which is the *phone* put down, because the
    /// sentence is different: a Pairing that is merely not the Active one is idle
    /// on a phone that is wide awake, and telling someone "Sharepaste is not
    /// looking while it is closed" about it would be false.
    case notActive(userId: String)

    /// The Relay turned this device's token away.
    ///
    /// The one genuine fault a phone can be in: no amount of waiting or
    /// reconnecting fixes a revoked Pairing, and the person has to pair again.
    case refused(userId: String, detail: String?)

    /// The Pairing a phase is about, where it has one.
    var userId: String? {
        switch self {
        case let .inContact(userId), let .outOfContact(userId), let .resting(userId),
             let .notActive(userId), let .refused(userId, _):
            userId
        case .unpaired, .looking:
            nil
        }
    }
}

/// Whether a phase is ordinary news or something the person has to act on.
///
/// Exhaustive over ``SessionPhase`` on purpose: adding a phase without deciding
/// which of the two it is becomes a compile error. It is the **only** statement
/// of that rule — `Signal` in `Fui.swift` chooses a lamp colour and defers its
/// alert arm to this, rather than re-enumerating which phases are faults.
enum Tone {
    /// Say it in the ordinary voice.
    ///
    /// A nominal phase does get a lit status light — Contact is a permanent
    /// readout — but never an alert colour, never a container, and never a call
    /// to action.
    case nominal

    /// Something is actually wrong and the person has to act.
    case fault
}

func toneOf(_ phase: SessionPhase) -> Tone {
    switch phase {
    case .unpaired, .looking, .inContact, .outOfContact, .resting, .notActive: .nominal
    case .refused: .fault
    }
}

/// What one Pairing's card says about itself.
///
/// A Pairing that is not the Active one holds no session, so whatever the core
/// last read off the wire for it is stale by construction — and it stays stale,
/// because the last Contact reading survives a teardown on purpose. Rendering
/// that reading would put "In contact with the Relay" on a card nothing is
/// connected for. So: **anything that is not the Active Pairing of a phone that
/// is in front is ``SessionPhase/notActive``**, and it is nominal.
///
/// A revoked token is the one exception and is reported either way. No amount of
/// not being connected fixes it, and it is the only thing on that screen a person
/// has to act on.
func pairingPhase(_ pairing: PairingSummary, foreground: Bool) -> SessionPhase {
    if pairing.status == .authFailed { return .refused(userId: pairing.userId, detail: nil) }
    if !pairing.isActive { return .notActive(userId: pairing.userId) }
    if !foreground { return .resting(userId: pairing.userId) }
    switch pairing.status {
    case .online: return .inContact(userId: pairing.userId)
    case .connecting: return .looking
    case .disconnected: return .outOfContact(userId: pairing.userId)
    // Answered above. Repeated rather than swept up by a `default`, so a reading
    // added to the core arrives here as a compile error.
    case .authFailed: return .refused(userId: pairing.userId, detail: nil)
    }
}

/// The pairing flow.
///
/// ``deviceLabel`` starts **empty** and stays empty until someone types
/// something. The desktop's flow hard-codes a default; copying that would put a
/// machine's guess on a person's own device, in a list they have to read later.
/// Pairing is blocked while it is blank, which is what makes the choice theirs
/// rather than a suggestion they can walk past.
///
/// **``code`` is one field with two ways of filling it, and a scan is one of
/// them.** It lives here rather than inside the screen because the camera and the
/// keyboard write to the same place: a scan puts the code it read into the field
/// and stops there. It does *not* pair.
///
/// ``scanned`` says the code in the field came off the camera, which is why the
/// viewfinder is no longer on screen. Emptying the field brings it back: that is
/// the way to scan a second code, and it is the only way, because a control for
/// it would sit beside the field it duplicates.
struct PairingState {
    var deviceLabel = ""
    var code = ""
    var scanned = false
    var camera: CameraProblem?
    var attempt: PairAttempt = .idle

    /// Whether the code is worth sending.
    var canPair: Bool {
        !deviceLabel.trimmingCharacters(in: .whitespaces).isEmpty
            && !code.trimmingCharacters(in: .whitespaces).isEmpty
            && attempt != .working
    }

    /// The flow as it should be arrived at: this phone's name, and nothing else.
    ///
    /// The name outlives one pairing because it names the phone rather than the
    /// Pairing; a code, a scan and a failure do not, and a screen that opened
    /// holding a spent code would offer to send it again. The camera goes too:
    /// whichever of the three states applies is re-read the moment the screen
    /// appears, and a remembered one would be a guess with a head start.
    func restarted() -> PairingState { PairingState(deviceLabel: deviceLabel) }
}

enum PairAttempt: Equatable {
    case idle

    /// A code is in flight. The relay has 120 seconds; this usually takes one.
    case working

    /// It did not work, and this says which of the several ways.
    ///
    /// `detail` carries the core's own sentence when the core had one worth
    /// repeating — `insecureRelay` names the relay and the reason, which no
    /// generic wording here could.
    case failed(message: String, detail: String? = nil)
}
