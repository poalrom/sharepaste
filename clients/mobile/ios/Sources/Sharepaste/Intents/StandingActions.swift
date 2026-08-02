import AppIntents
import Foundation
import SharepasteCore
import SharepasteKit

// The two verbs a phone performs without its surface being opened, in the only
// shape iOS allows without leaking.
//
// **They are building blocks, not shortcuts.** There is deliberately no
// `AppShortcutsProvider` anywhere in this target, and its absence is a decision
// rather than an oversight. Zero-setup App Shortcuts would make both verbs
// reachable the instant the app installs, and they are declined for two
// independent reasons. Offer cannot take the clipboard as an open-ended
// parameter in an App Shortcut phrase, so it would not work. Recall would work
// and is worse: its returned string lands in a result snippet, so a background
// invocation would *display a decrypted Entry* — precisely the exposure ADR 0007
// exists to prevent. Do not add one because it would be convenient.
//
// **Neither intent READS the pasteboard, and one of them writes it.** Offer
// takes its text as a parameter and Recall returns a string, so the person
// assembles *Get Clipboard* → Offer and Recall → *Copy to Clipboard*. The read
// half of ADR 0007 therefore holds outright: this app never sees a pasteboard
// it was not handed.
//
// The write half does not, and ticket 07's *"neither touches the pasteboard —
// Shortcuts does, never us"* is false as shipped. `recall_latest` in the core
// puts the plaintext on the clipboard itself before it returns
// (`clients/core/src/facade.rs:858`), because that is what the verb means on the
// two shells that already existed. There is no shell-side way to decline it: the
// facade exposes no fetch-without-writing, and `read_entry` alone would skip the
// round trip that makes a Recall a Recall rather than a cache read.
//
// **Recorded as a finding rather than worked around.** The spec's Out of scope
// says: *"Any core change beyond a Swift bindgen invocation. If iOS needs a core
// change, that is a finding, not a task."* This is that. The finding is written
// up on ticket 07 and the fix is a core one — `recall_latest` splitting the
// fetch from the hand-over, or taking the clipboard write as a flag the way
// `open` already takes `require_https`.
//
// What was **not** done is leave the app's own words claiming the property it
// does not have. `Strings.shortcutsBody` and `Strings.shortcutsRecallNote` say
// what actually happens, because a privacy claim a person reads on the Settings
// Screen is worth more than a privacy claim in a ticket.
//
// **Neither intent opens the app.** `openAppWhenRun` stays `false` and nothing
// here returns `.continueInApp`. The point of a Standing Action is that it shows
// nothing and picks nothing. If one could not do its work without foregrounding,
// that would be a finding to record rather than a flag to set.
//
// **No intent draws a Receipt, and that is a knowing divergence.** Android
// folded its Standing Actions into the Receipt type in `0.5.0` so that a
// Standing Action and an in-app press are one operation reported one way — a
// Toast, which is the only surface a closed Android phone has. An iOS app
// running as an App Intent has no such surface: what it reports is its return
// value and its dialog, and Shortcuts decides what becomes of them. So the
// glossary's *"the same whether the app was open or closed"* holds in intent and
// not in mechanism here. The intents must not reach for a substitute: a
// notification would need the entitlement ADR 0007 refuses, and a result snippet
// is the leak `AppShortcutsProvider` was declined to avoid. Spec row 17 — the
// reason the shapes differ is the platform's, not a shortcut taken.

/// What an intent says when it cannot do what was asked.
///
/// Each case is something the person can act on, which is the bar ticket 07
/// sets: an intent that hits a refusal must surface it through its own error or
/// dialog rather than returning as though nothing happened. A single opaque
/// failure would be true of all of them and useful for none.
enum StandingActionError: Error, CustomLocalizedStringResourceConvertible {
    /// No Active Pairing. The ordinary state of a fresh install, and not a fault
    /// the core can report — it is asked about a Pairing it has been given the id
    /// of, so "nothing is paired" is a value there rather than an error.
    case unpaired

    /// Nothing to hand back: an empty History, or an Entry this phone holds no
    /// plaintext for.
    case nothingToRecall

    /// The facade did not answer inside the budget.
    ///
    /// An App Intent has a short execution window, and the facade opens a
    /// database and may reach the relay. What a timeout reports is decided here
    /// rather than left to be whatever the system does, which is a silent kill.
    case timedOut

    /// Anything else, in the app's words with the core's underneath where the
    /// core had one worth repeating.
    case failed(String)

    var localizedStringResource: LocalizedStringResource {
        switch self {
        case .unpaired: "\(Strings.actionUnpaired)"
        case .nothingToRecall: "\(Strings.recallNothingToRecall)"
        case .timedOut: "\(Strings.standingActionTimedOut)"
        case let .failed(detail): "\(detail)"
        }
    }
}

/// How long an intent gives the facade before it reports a timeout.
///
/// The point is not to beat the system's own budget, which is shorter and
/// undocumented, but to make the failure *ours*: a timeout that reports a
/// sentence beats a shortcut the system kills with nothing said. Every FFI call
/// is blocking and the chokepoint serialises them, so an intent arriving while a
/// screen is mid-Recall waits behind it — that wait is real, and this bounds it.
private let intentBudget: Duration = .seconds(12)

/// Run `work`, or report ``StandingActionError/timedOut``.
func withIntentBudget<T: Sendable>(
    _ work: @escaping @Sendable () async throws -> T
) async throws -> T {
    try await withThrowingTaskGroup(of: T.self) { group in
        group.addTask { try await work() }
        group.addTask {
            try await Task.sleep(for: intentBudget)
            throw StandingActionError.timedOut
        }
        guard let first = try await group.next() else { throw StandingActionError.timedOut }
        group.cancelAll()
        return first
    }
}

/// Hand text to the protocol, without opening the app.
///
/// The text is a parameter because *Shortcuts touches the pasteboard, never us*.
/// The person wires *Get Clipboard* into it; this app never reads a pasteboard
/// it was not handed.
struct OfferIntent: AppIntent {
    // Computed rather than stored. `AppIntent` declares these as `static var`
    // requirements, and a stored `static var` is nonisolated global mutable
    // state under Swift 6 — an error, not a warning. Nothing here varies, so a
    // getter costs nothing and says so.
    static var title: LocalizedStringResource { "Offer what I copied" }

    static var description: IntentDescription {
        // One literal, not two concatenated: `IntentDescription` takes a
        // `LocalizedStringResource`, and `+` over two string literals produces a
        // `String`, which is not one.
        IntentDescription(
            "Hands text to Sharepaste, which encrypts it and sends it to your Relay so your other devices can recall it. Chain the Shortcuts action Get Clipboard into this one.",
            categoryName: "Sharepaste"
        )
    }

    /// Never. See the note at the top of this file.
    static var openAppWhenRun: Bool { false }

    @Parameter(
        title: "Text",
        description: "What to offer. Usually the output of the Get Clipboard action.",
        inputOptions: String.IntentInputOptions(multiline: true)
    )
    var text: String

    static var parameterSummary: some ParameterSummary {
        Summary("Offer \(\.$text) to Sharepaste")
    }

    func perform() async throws -> some IntentResult & ProvidesDialog {
        let repository = AppGraph.shared.repository
        let offered = text
        let attempt = try await withIntentBudget { try await repository.offerText(offered) }

        switch attempt {
        case .unpaired:
            // Thrown rather than returned. A shortcut that "succeeded" with
            // nothing sent is the silent failure this whole file exists to avoid.
            throw StandingActionError.unpaired

        case let .settled(userId, outcome):
            switch outcome {
            case let .queued(pending):
                // The Entry is enqueued, and the uploader lives on a session an
                // intent does not have. Bringing one up, waiting for the queue
                // and putting it back down is what `sendPending` is for — and it
                // is why this is not the background work ADR 0007 forbids:
                // somebody ran a shortcut, the wait is bounded, and the session
                // comes down before the intent returns.
                let drained = await repository.sendPending(userId: userId)
                // `pendingCount` is the screen's sentence and carries no number,
                // because the History draws the depth beside it as a readout. A
                // dialog has no readout beside it, so the intent needs the one
                // that says the figure out loud — otherwise a shortcut that left
                // a queue behind reports the same words as one that did not.
                let said = drained ? Strings.offerQueued : Strings.offerQueuedPending(pending)
                return .result(dialog: IntentDialog(stringLiteral: said))

            case let .rejected(reason):
                // A refusal is a Notice on the History Screen and has to be one
                // here too. `offerRefusalMessage` is the same wording the band
                // uses, so the two paths cannot report one refusal in two idioms.
                throw StandingActionError.failed(offerRefusalMessage(reason))
            }
        }
    }
}

/// Fetch the newest Entry and hand it back, without opening the app.
///
/// Returns a string. Shortcuts decides what becomes of it; the person wires
/// *Copy to Clipboard* onto the end. This app never writes a pasteboard unasked.
struct RecallLatestIntent: AppIntent {
    // Computed, for the reason `OfferIntent` states.
    static var title: LocalizedStringResource { "Recall the latest Entry" }

    static var description: IntentDescription {
        IntentDescription(
            "Fetches the newest Entry from your Relay and hands back its text. Chain the Shortcuts action Copy to Clipboard onto this one.",
            categoryName: "Sharepaste"
        )
    }

    static var openAppWhenRun: Bool { false }

    func perform() async throws -> some IntentResult & ReturnsValue<String> & ProvidesDialog {
        let repository = AppGraph.shared.repository
        let attempt = try await withIntentBudget {
            try await repository.recallLatestOnActivePairing()
        }

        guard case let .done(userId, entryId, _, fromCache) = attempt else {
            throw StandingActionError.unpaired
        }

        // The plaintext is read back by id rather than carried on the attempt:
        // `RecallAttempt` deliberately holds none, because a secret nothing needs
        // is a secret something logs. Here it *is* needed — the return value is
        // the whole of the verb — so this is the one call that asks for it.
        let text = try await withIntentBudget {
            try await repository.readEntry(userId: userId, entryId: entryId)
        }
        guard let text else { throw StandingActionError.nothingToRecall }

        // `RecalledFromCache` may never be silent (ADR 0007), and "silent"
        // includes a shortcut that quietly copies yesterday's link. The dialog is
        // the intent's own surface for saying so, and it carries a fixed sentence
        // — never the Entry, which is what the return value is for.
        //
        // The value is still returned. Handing back nothing would be a worse
        // answer than handing back the best one available and saying what it is,
        // which is the judgement the History Screen's band already makes.
        let said = fromCache ? Strings.recallFromCache : Strings.recallDone
        return .result(value: text, dialog: IntentDialog(stringLiteral: said))
    }
}
