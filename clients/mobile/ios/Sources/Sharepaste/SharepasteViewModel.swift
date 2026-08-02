import Foundation
import SharepasteCore
import SharepasteKit
import SwiftUI

/// The one state holder, and the whole of the sync model.
///
/// Two responsibilities, and they are the same responsibility seen from two
/// sides: it turns what the person does into calls on ``SharepasteRepository``,
/// and it turns what the core raises into ``UiState``. Nothing else in the app
/// talks to the repository, and no view talks to the core.
///
/// **Sync is foreground only** (ADR 0007), and on iOS that is forced as well as
/// chosen: Background Modes is entitlement-gated alongside push and a free
/// Personal Team has neither. ``onEnterForeground()`` resumes the Active Pairing
/// and brings its session up; ``onLeaveForeground()`` takes every session down.
/// There is nothing in between. The honest consequence is on screen, in
/// ``Strings/foregroundOnlyNote``, rather than buried here.
///
/// `@MainActor` in its entirety, which is where the marshalling ADR 0006's
/// threading note demands actually happens: the core raises events on its own
/// tokio worker threads, ``StreamEventSink`` hands them to a stream, and the one
/// consumer of that stream is this type — so the hop onto the main thread occurs
/// once, here, instead of at every call site.
@MainActor
final class SharepasteViewModel: ObservableObject {

    @Published private(set) var state = UiState()

    /// The Receipts, as they happen, and deliberately not part of ``state``.
    ///
    /// A Receipt is transient: put in the snapshot it would be re-shown by every
    /// render that followed and would need a second action to clear itself again,
    /// which is the shape ``Notice`` has precisely because a Notice *is*
    /// something on screen. Held as one value with a stamp so that two identical
    /// Recalls in a row are two Receipts rather than one that never changed.
    @Published private(set) var receipt: TimestampedReceipt?

    struct TimestampedReceipt: Equatable {
        let receipt: Receipt
        let at: Date
    }

    private let repo: SharepasteRepository
    private let preferences: UiPreferences
    private var pump: Task<Void, Never>?

    init(repo: SharepasteRepository, preferences: UiPreferences) {
        self.repo = repo
        self.preferences = preferences
        readPreferences()
        pump = Task { [weak self] in
            guard let events = self?.repo.events() else { return }
            for await event in events {
                guard let self else { return }
                self.onCoreEvent(event)
            }
        }
    }

    deinit { pump?.cancel() }

    // -- the two lifecycle edges, which are the entire sync model ---------------

    /// The app came to the front: pick the Active Pairing back up and bring it
    /// online.
    ///
    /// A phone with no Pairing lands on the pairing flow, which is the only thing
    /// it can usefully do. A failure to resume is *not* an error screen — being
    /// out of contact is the nominal case, and a phone that cannot reach the
    /// Relay right now is a phone that will try again next time it is opened.
    func onEnterForeground() {
        // Recorded before the work, not inside it, for the same reason
        // `onLeaveForeground` does: the flag is what tells a disconnected session
        // apart from a resting one, and a window where the app is in front but
        // does not think so would read as "resting" on screen.
        state.foreground = true
        state.session = .looking
        // The preferences are re-read here as well as at construction: the person
        // may have been in Settings, and there is no notification for a
        // `UserDefaults` value this process itself wrote through a different
        // instance.
        readPreferences()

        Task {
            let userId = try? await repo.resumeActivePairing()
            guard let userId else {
                state.screen = .pairing
                state.session = .unpaired
                state.activeUserId = nil
                return
            }
            state.screen = .history
            state.activeUserId = userId
            // Before the session, because it needs no network and everything it
            // reads is what the list is rendered *with*: this device's own Device
            // id, without which every row claims an Origin; the depth of the
            // pending queue; and the Pairings themselves, without which a
            // divergence band has no names to put in its sentence.
            await refreshPairings()
            // The cached Entries too, and **before** the session rather than
            // after it. Reading afterwards is a race with the session's own
            // stream: `startSession` returns as soon as its tasks are on the
            // core's runtime, so an `entryAdded` can arrive between this read
            // hitting the database and the state being written — and the write
            // then replaces a list that already had the new Entry in it with one
            // that never will.
            await refreshHistory(userId)
            do {
                // Opens the SSE session and backfills every Entry that arrived
                // while this phone was closed. Returns as soon as the tasks are
                // on the core's runtime; the Relay is reported through events.
                //
                // This is also the only thing that flushes the pending queue: the
                // uploader lives on the session, so an Offer made with no
                // connection goes out on the next foreground and never in the
                // background.
                try await repo.startSession(userId: userId)
            } catch {
                state.session = sessionPhase(for: userId, error: error)
            }
        }
    }

    /// The app went away: take every session down.
    func onLeaveForeground() {
        // The Viewed Pairing goes with it. It is a transient view choice —
        // `CONTEXT.md`: "forgotten when the window closes" — and a phone's
        // equivalent of closing the window is being put down.
        let wasDiverged = state.diverged
        state.foreground = false
        state.viewedUserId = nil
        state.confirming = nil
        // The rows belonged to the Pairing being stopped looking at, so they
        // would otherwise be read as the Active Pairing's on the way back in.
        if wasDiverged { state.entries = [] }

        Task {
            try? await repo.stopAllSessions()
            // `looking` carries no user id, because `onEnterForeground` enters it
            // before it knows which Pairing it is resuming. A phone put down
            // while it was still looking would otherwise stay on screen claiming
            // to be checking for new Entries with nothing left running to check.
            if let resting = state.session.userId ?? state.activeUserId {
                state.session = .resting(userId: resting)
            }
        }
    }

    // -- pairing ----------------------------------------------------------------

    func setDeviceLabel(_ label: String) { state.pairing.deviceLabel = label }

    func setCameraProblem(_ problem: CameraProblem?) { state.pairing.camera = problem }

    /// The code field, as somebody types in it.
    ///
    /// Emptying it clears ``PairingState/scanned``, which is what puts the
    /// viewfinder back on screen. A field left holding a scanned code keeps the
    /// camera stood down, including while it is being edited — resurrecting a
    /// preview under a cursor because a character was deleted would be worse than
    /// either state.
    func setPairingCode(_ code: String) {
        state.pairing.code = code
        state.pairing.scanned = state.pairing.scanned && !code.isEmpty
    }

    /// A code the camera read. It fills the field and stands the viewfinder down.
    ///
    /// **It does not pair**, and that is the point rather than an omission. A
    /// scan is the first thing a person does on this screen — the square is the
    /// only part of it that looks like an instruction — and it arrives before the
    /// name the Pairing has to carry. Pairing on it would spend a code with a
    /// two-minute life on a message asking for the name.
    ///
    /// The scanner fires on every frame a code stays in view, so the first one
    /// wins: ``PairingState/scanned`` is the gate that turns a stream of
    /// identical decodes into one field. It also clears a failure, because the
    /// code that failed is no longer the code in the field.
    func codeScanned(_ code: String) {
        guard !state.pairing.scanned else { return }
        state.pairing.code = code
        state.pairing.scanned = true
        state.pairing.attempt = .idle
    }

    /// Take the failure back, and the code with it.
    ///
    /// The whole of ``PairingState/restarted()`` rather than the attempt alone,
    /// and that is the fix rather than a tidy-up: clearing the attempt on its own
    /// leaves the spent code in the field, so `canPair` goes true again and the
    /// control resends a code the Relay has already expired — and it leaves
    /// `scanned` latched, so the viewfinder stays stood down and there is no way
    /// to read a fresh one. The Device Label survives, because it names the phone
    /// rather than the attempt. Android shipped the one-field version and had to
    /// fix it; Swift does not inherit that bug unless somebody writes it.
    func dismissPairFailure() { state.pairing = state.pairing.restarted() }

    /// Pair with the code in the field, however it got there.
    ///
    /// The code goes to the core exactly as it arrived: the core's `decode`
    /// already strips whitespace and dashes and upper-cases, so the desktop's
    /// compact QR payload and a code someone typed in groups of four are both
    /// simply codes.
    ///
    /// Ignored while an attempt is already in flight. The two other refusals — no
    /// code, no name — are what ``PairingState/canPair`` disables the button over,
    /// and they are re-checked here because a state holder that trusts a screen to
    /// have disabled something is a state holder with a hole in it.
    func pairWithCode() {
        let pairing = state.pairing
        guard pairing.attempt != .working else { return }
        guard !pairing.code.trimmingCharacters(in: .whitespaces).isEmpty else { return }
        guard !pairing.deviceLabel.trimmingCharacters(in: .whitespaces).isEmpty else {
            state.pairing.attempt = .failed(message: Strings.pairNeedsAName)
            return
        }
        state.pairing.attempt = .working
        Task {
            do {
                let paired = try await repo.pairWithCode(
                    code: pairing.code,
                    deviceLabel: pairing.deviceLabel.trimmingCharacters(in: .whitespaces)
                )
                try await repo.setActivePairing(userId: paired.userId)
                state.screen = .history
                state.session = .looking
                state.activeUserId = paired.userId
                // A Pairing just added is the one to look at, whatever was being
                // looked at before.
                state.viewedUserId = nil
                // The flow is spent: nothing here should be able to offer this
                // code a second time.
                state.pairing = state.pairing.restarted()
                await refreshPairings()
                // Before the session, for the reason `onEnterForeground` spells
                // out: a read that lands after it can overwrite an Entry the
                // stream has already delivered.
                await refreshHistory(paired.userId)
                try await repo.startSession(userId: paired.userId)
            } catch {
                state.pairing.attempt = pairFailure(for: error)
            }
        }
    }

    // -- History, Offer and Recall ----------------------------------------------

    /// Offered Capture of whatever is on the pasteboard.
    ///
    /// Every Entry a phone produces is an Offered Capture: the person hands the
    /// content over, so the device never sees a pasteboard it was not shown. It
    /// is honoured whether or not capture is enabled — `capture_enabled` governs
    /// Watched Capture, which a phone never performs, and refusing content
    /// someone just chose to share would be indefensible.
    ///
    /// Expect iOS to show its paste banner. That is the platform telling the
    /// truth about what just happened and must not be engineered around.
    func offerPasteboard() {
        Task {
            do {
                switch try await repo.offerPasteboard() {
                case .unpaired:
                    raise(.unpaired)
                case let .settled(_, outcome):
                    switch outcome {
                    case let .queued(pending):
                        // The Offer's own count is more current than any event:
                        // the enqueue has already happened by the time it is
                        // returned, and `pendingCount` is on its way through the
                        // sink behind it.
                        state.notice = nil
                        state.pending = pending
                        show(.offered(pending: pending))
                    case let .rejected(reason):
                        raise(.offerRefused(reason: reason))
                    }
                }
            } catch {
                raise(.failed(message: Strings.offerFailed, detail: explain(error)))
            }
        }
    }

    /// Recall Latest: the newest Entry onto this device's pasteboard.
    ///
    /// It always fetches, and when the fetch fails the newest **cached** Entry is
    /// handed over instead with ``Notice/recalledFromCache`` to say so. That one
    /// stays a Notice and not a Receipt: it is the outcome ADR 0007 says may
    /// never be silent, and a band that waits to be dismissed is what "never
    /// silent" costs.
    func recallLatest() {
        Task {
            do {
                switch try await repo.recallLatestOnActivePairing() {
                case .unpaired:
                    raise(.unpaired)
                case let .done(userId, entryId, _, fromCache):
                    if fromCache {
                        raise(.recalledFromCache)
                    } else {
                        await confirmRecall(preview: await previewOf(userId, entryId))
                    }
                }
            } catch {
                raise(recallFailure(for: error))
            }
        }
    }

    /// Recall one chosen Entry.
    ///
    /// Takes the Entry's own `userId` rather than the Active Pairing's, because
    /// the Viewed Pairing may not be the Active one and a row must recall from
    /// the History it is a row of.
    func recall(_ entry: Entry) {
        Task {
            do {
                try await repo.recall(userId: entry.userId, entryId: entry.id)
            } catch {
                raise(recallFailure(for: error))
                return
            }
            await confirmRecall(preview: entry.preview)
        }
    }

    /// Delete one Entry, on the Relay and here.
    ///
    /// Offered for an Undecryptable Entry as much as for any other: ciphertext
    /// this device holds no key for is exactly the thing a person most wants
    /// gone, and it is the one row where deleting is all they can do with it.
    func deleteEntry(_ entry: Entry) {
        Task {
            do {
                try await repo.deleteEntry(userId: entry.userId, entryId: entry.id)
            } catch {
                raise(.failed(message: Strings.deleteFailed, detail: explain(error)))
            }
        }
    }

    func dismissNotice() { state.notice = nil }

    /// Put one Notice in the band, replacing whatever was there.
    private func raise(_ notice: Notice) { state.notice = notice }

    private func show(_ receipt: Receipt) {
        self.receipt = TimestampedReceipt(receipt: receipt, at: Date())
    }

    /// Say what was recalled, unless the person has asked not to be told.
    ///
    /// The Receipt is suppressed **whole**, not merely stripped of its Preview:
    /// `SHOW WHAT WAS RECALLED` off means the Entry reaches the pasteboard and
    /// Sharepaste says nothing, which is the switch's own sentence. Only this
    /// confirmation goes quiet — a stale Recall, a refusal and a failure are
    /// Notices and are not the switch's to silence.
    ///
    /// The band is cleared either way. Without that, a `MAY BE STALE` from the
    /// Recall before this one would still be on screen describing what is no
    /// longer on the pasteboard.
    private func confirmRecall(preview: String?) async {
        state.notice = nil
        guard state.showRecalled else { return }
        show(.recalled(preview: preview))
    }

    /// The Preview of the Entry a Recall Latest just handed over.
    ///
    /// In memory first, because the recalled Entry is the newest one and the
    /// newest one is at the head of a list already in hand. The read is the
    /// fallback for the case the list is not: a Recall Latest acts on the
    /// **Active** Pairing, and the list belongs to the Viewed one.
    private func previewOf(_ userId: String, _ entryId: Int64) async -> String? {
        if let known = state.entries.first(where: { $0.id == entryId })?.preview { return known }
        return await repo.previewOf(userId: userId, entryId: entryId)
    }

    // -- what this phone has been told about its own chrome ----------------------

    /// Whether a Recall says what it put on the pasteboard.
    ///
    /// Written to the store and then read back, rather than assigned to
    /// ``UiState`` directly: ``readPreferences()`` is the only writer of either
    /// preference field, so the switch on screen shows what was persisted rather
    /// than what was pressed.
    func setShowRecalled(_ show: Bool) {
        preferences.setShowRecalled(show)
        readPreferences()
    }

    /// Close the foreground-only band for good.
    ///
    /// Only `▴ CLOSE` reaches this. Expanding the band is exploration and must
    /// not dismiss it — the whole strip is the tap target, so a stray tap would
    /// otherwise delete the app's most important disclosure.
    func dismissForegroundNote() {
        preferences.dismissForegroundNote()
        readPreferences()
    }

    private func readPreferences() {
        let values = preferences.values
        state.showRecalled = values.showRecalled
        state.foregroundNoteDismissed = values.foregroundNoteDismissed
    }

    // -- Pairings: the Viewed one, the Active one, and the two erasures ----------

    /// Show the Settings Screen, and read the Pairings back before they render.
    func openSettings() {
        state.screen = .settings
        state.confirming = nil
        Task { await refreshPairings() }
    }

    func openHistory() {
        state.screen = .history
        state.confirming = nil
    }

    /// Show the pairing flow, from the top.
    ///
    /// The flow is restarted rather than resumed. Its state outlives the screen —
    /// the code field is ``PairingState``'s, so that a scan can fill it — and a
    /// second visit that opened holding the code from an abandoned first one
    /// would offer to send a code minted minutes ago for somebody else's slot.
    func openAddPairing() {
        state.screen = .pairing
        state.confirming = nil
        state.pairing = state.pairing.restarted()
    }

    /// Look at another Pairing's History.
    ///
    /// **This changes nothing about syncing or capture.** No session is started
    /// or stopped, no setting is written, nothing is persisted: the Viewed
    /// Pairing is an override held in memory and dropped when the app is put
    /// down. Offer and Recall Latest continue to act on the Active Pairing, which
    /// is what the divergence band is on screen to admit.
    func viewPairing(_ userId: String) {
        state.viewedUserId = userId
        state.confirming = nil
        // Emptied rather than left: the rows on screen belong to the other
        // Pairing, and a list that changes ownership one repaint late is a list
        // that briefly attributes Entries to the wrong User.
        state.entries = []
        state.ownDeviceId = state.pairings.first { $0.userId == userId }?.deviceId
        Task { await refreshHistory(userId) }
    }

    /// Sync this Pairing from now on.
    ///
    /// The persistent half of the pair of distinctions: the core writes the
    /// choice to settings, so it survives a restart, and brings the new Pairing's
    /// session up. The one it replaces is stopped **here** rather than left
    /// running — `set_active_pairing` starts the new session but has no opinion
    /// about the old one, and a phone quietly holding two live SSE streams is not
    /// what "exactly one is active at a time" means.
    func activatePairing(_ userId: String) {
        Task {
            let previous = state.activeUserId
            do {
                if let previous, previous != userId {
                    try await repo.stopSession(userId: previous)
                }
                try await repo.setActivePairing(userId: userId)
            } catch {
                state.notice = .failed(message: Strings.pairingUseFailed, detail: explain(error))
                return
            }
            state.activeUserId = userId
            // If the Pairing just made Active is the one being viewed, the two
            // now agree and the override has nothing left to say.
            if state.viewedUserId == userId { state.viewedUserId = nil }
            state.confirming = nil
            await refreshPairings()
            if let viewed = state.viewedPairing { await refreshHistory(viewed) }
        }
    }

    /// Ask for a destructive action, or take the question back with `nil`.
    func confirm(_ confirmation: Confirmation?) { state.confirming = confirmation }

    /// Erase every Entry of one Pairing, on the Relay and on every device.
    ///
    /// Takes the Pairing explicitly rather than assuming the Active or the Viewed
    /// one. A phone may hold several, the button is on a card, and the card is
    /// what the person is looking at when they press it.
    func clearHistory(_ userId: String) {
        state.confirming = nil
        Task {
            let name = state.nameOf(userId)
            do {
                try await repo.clearHistory(userId: userId)
            } catch {
                state.notice = .failed(message: Strings.clearHistoryFailed, detail: explain(error))
                return
            }
            state.notice = .historyCleared(pairing: name)
            if userId == state.viewedPairing { state.entries = [] }
            await refreshPairings()
        }
    }

    /// Forget a Pairing: its Entries, its key material and its token.
    ///
    /// The facade does all of it, including promoting another Pairing to Active
    /// and bringing its session up. What is read back afterwards is the core's
    /// answer rather than a guess made here — which Pairing was promoted is the
    /// core's decision, and a shell that assumed one would be wrong the first
    /// time the rule changed.
    func forgetPairing(_ userId: String) {
        state.confirming = nil
        Task {
            let name = state.nameOf(userId)
            do {
                try await repo.forgetPairing(userId: userId)
            } catch {
                state.notice = .failed(message: Strings.forgetFailed, detail: explain(error))
                return
            }
            let active = try? await repo.activePairing()
            state.activeUserId = active ?? nil
            if state.viewedUserId == userId { state.viewedUserId = nil }
            state.pairings.removeAll { $0.userId == userId }
            await refreshPairings()

            let viewed = state.viewedPairing
            let promoted = state.nameOf(active ?? nil)
            state.notice = .pairingForgotten(
                pairing: name,
                promoted: promoted.isEmpty ? nil : promoted
            )
            if viewed == nil {
                // Nothing left to be paired to: the pairing flow is the only
                // screen a phone in that state can usefully be on.
                state.screen = .pairing
                state.session = .unpaired
                state.entries = []
            }
            state.ownDeviceId = state.pairings.first { $0.userId == viewed }?.deviceId
            if let viewed { await refreshHistory(viewed) }
        }
    }

    // -- the core's own events ---------------------------------------------------

    private func onCoreEvent(_ event: CoreEvent) {
        switch event {
        case let .connectionState(userId, connection, lastError):
            state.session = phase(for: userId, state: connection, lastError: lastError)
            // The card for that Pairing reads the same event. Only the Active
            // Pairing has a session raising them, which is exactly why
            // `pairingPhase` refuses to render a stale reading for any other
            // card.
            state.pairings = state.pairings.map {
                var pairing = $0
                if pairing.userId == userId { pairing.status = connection }
                return pairing
            }

        // Gated on the **Viewed** Pairing, not the Active one. A live session
        // belongs to the Active Pairing and goes on delivering while someone is
        // looking at another Pairing's History; ungated, its Entries would appear
        // in a list they are not part of and be attributed to the wrong User.
        case let .entryAdded(userId, entry):
            guard userId == state.viewedPairing else { break }
            // Newest first, and de-duplicated by id: the backfill and the live
            // SSE stream can both deliver the same Entry across a reconnect.
            guard !state.entries.contains(where: { $0.id == entry.id }) else { break }
            state.entries.insert(entry, at: 0)
            // The one thing that scrolls the list, and the reason it is announced
            // here rather than derived from the list changing: an arrival is a
            // *new head with the old head still under it*, and deleting the
            // newest row or switching the Viewed Pairing both change the head as
            // well. Neither is somewhere to drag a reader mid-read.
            arrived = entry.id

        case let .entryDeleted(userId, entryId):
            guard userId == state.viewedPairing else { break }
            state.entries.removeAll { $0.id == entryId }

        case let .historyChanged(userId):
            guard userId == state.viewedPairing else { break }
            Task { await refreshHistory(userId) }

        // The queue's depth, from the uploader that just changed it. The
        // screen-wide count is matched against the Active Pairing and not against
        // the phase's user id: the uploader drains the queue long before the SSE
        // reader reports online, so a phase-based gate drops the one count that
        // matters while the phone is still `looking`. The card takes it for
        // whichever Pairing it belongs to, because a queue on a Pairing this
        // device has switched away from is visible nowhere else.
        case let .pendingCount(userId, count):
            if userId == state.activeUserId { state.pending = count }
            state.pairings = state.pairings.map {
                var pairing = $0
                if pairing.userId == userId { pairing.pending = count }
                return pairing
            }

        // The core moves the Active Pairing itself when one is forgotten, so this
        // is not merely an echo of `setActivePairing`.
        case let .activePairingChanged(userId):
            state.activeUserId = userId
            Task { await refreshPairings() }

        case let .pairingRemoved(userId):
            state.pairings.removeAll { $0.userId == userId }

        case .pairingAdded:
            Task { await refreshPairings() }

        // `contact` is read through `SessionPhase` rather than through the stamp.
        // The rest belong to a pairing flow this phone never runs — it is the
        // claimer, so it reads a shortcode rather than minting one. Named rather
        // than caught by a `default` so that adding a variant to the core is a
        // compile error here instead of an event silently dropped on the floor.
        case .contact, .pairShortcode, .pairClaimed, .pairExpired:
            break
        }
    }

    /// The id of the Entry that just arrived, for the list to scroll to.
    ///
    /// `@Published` and set only from ``onCoreEvent(_:)``'s `entryAdded` arm, so
    /// that the scroll is caused by an arrival and by nothing else. A view that
    /// watched `entries.first` instead would also scroll when the newest row was
    /// deleted and when the Viewed Pairing changed, and neither is somewhere to
    /// drag a reader mid-read.
    @Published private(set) var arrived: Int64?

    /// Every Pairing this phone holds, and the two facts the History is rendered
    /// with.
    ///
    /// Read from `listPairings`, which needs no network, so a phone with no route
    /// to the Relay still renders its rows' Origins correctly. A failure leaves
    /// the previous values: a missing `ownDeviceId` shows an Origin that need not
    /// be shown, which is better than hiding one that must be.
    private func refreshPairings() async {
        guard let pairings = try? await repo.listPairings() else { return }
        state.pairings = pairings
        state.ownDeviceId =
            pairings.first { $0.userId == state.viewedPairing }?.deviceId ?? state.ownDeviceId
        state.pending = pairings.first { $0.userId == state.activeUserId }?.pending ?? state.pending
    }

    /// The Viewed Pairing's cached Entries.
    ///
    /// The write is guarded on the Pairing still being the viewed one. A read
    /// started for one Pairing can land after the person has switched to another
    /// — `viewPairing` starts one, and so does every `historyChanged` — and
    /// putting one Pairing's Entries into the list of another is the one mistake
    /// on this screen a person cannot spot.
    private func refreshHistory(_ userId: String) async {
        guard let entries = try? await repo.listHistory(userId: userId) else { return }
        guard userId == state.viewedPairing else { return }
        state.entries = entries
    }

    /// The core's connection reading, in the phone's terms.
    ///
    /// The foreground flag decides more than the wording. Leaving the foreground
    /// takes every session down, but a `connectionState` frame raised a moment
    /// earlier can still be travelling through the sink when it does — and
    /// rendering that frame would put a phone that has just hung up on the Relay
    /// on screen claiming to be in contact with it. With no session there is
    /// nothing to be in contact through, so the honest reading is resting.
    ///
    /// A revoked Pairing is the exception, and it is not a wording exception: no
    /// amount of waiting or reconnecting fixes it, so it is news whether the app
    /// is in front or not.
    private func phase(
        for userId: String,
        state connection: ConnectionState,
        lastError: String?
    ) -> SessionPhase {
        if connection == .authFailed { return .refused(userId: userId, detail: lastError) }
        if !state.foreground { return .resting(userId: userId) }
        switch connection {
        case .online: return .inContact(userId: userId)
        case .connecting: return .looking
        case .disconnected: return .outOfContact(userId: userId)
        // Answered above. Repeated rather than swept up, so a reading added to
        // the core arrives here as a compile error.
        case .authFailed: return .refused(userId: userId, detail: lastError)
        }
    }

    /// The phase a failed `startSession` leaves the phone in.
    private func sessionPhase(for userId: String, error: Error) -> SessionPhase {
        if case let AppError.Auth(detail) = error { return .refused(userId: userId, detail: detail) }
        // Everything else — no route to the Relay, a refused cleartext relay, a
        // keychain that would not open — leaves the phone out of contact, which
        // is where a phone spends most of its life anyway.
        return .outOfContact(userId: userId)
    }

    /// Which of the pairing failures this was.
    ///
    /// Each one gets its own sentence. A single "pairing failed" would be true of
    /// all of them and useful for none: an expired code needs a fresh code, a
    /// cleartext relay needs a certificate, and a code that is not a code needs
    /// retyping. `insecureRelay` is the one that shows the core's own words,
    /// because the core names the relay and the reason and no wording here could.
    private func pairFailure(for error: Error) -> PairAttempt {
        switch error {
        case AppError.PairExpired:
            .failed(message: Strings.pairCodeExpired)
        case let AppError.InsecureRelay(detail):
            .failed(message: Strings.pairInsecureRelay, detail: detail)
        case AppError.BadInput:
            .failed(message: Strings.pairNotACode)
        case AppError.Auth:
            .failed(message: Strings.pairRefused)
        // The Relay answers a claim for a slot it has already expired with a 404,
        // so this is the same news as `PairExpired` and gets the same sentence.
        case AppError.NotFound:
            .failed(message: Strings.pairCodeExpired)
        case AppError.Network:
            .failed(message: Strings.pairUnreachable)
        default:
            .failed(message: Strings.pairFailed)
        }
    }

    /// Which of the Recall failures this was.
    ///
    /// `NotFound` is the one worth telling apart, and it covers the two ways a
    /// Recall has nothing to hand over: an Undecryptable Entry, whose cached
    /// plaintext is NULL, and a History with nothing in it at all. Both mean "no
    /// plaintext here", which is why the *list* is where an Undecryptable Entry
    /// is marked and why its Recall control is refused before it is pressed.
    private func recallFailure(for error: Error) -> Notice {
        switch error {
        case AppError.NotFound:
            .failed(message: Strings.recallNothingToRecall)
        case let AppError.InsecureRelay(detail):
            .failed(message: Strings.recallFailed, detail: detail)
        default:
            .failed(message: Strings.recallFailed, detail: explain(error))
        }
    }
}

/// The core's own words, whichever variant carried them.
///
/// UniFFI gives each variant its own `detail` field rather than one on a shared
/// parent, so reading the detail generically costs a `switch`. The `switch` is
/// worth having: it is exhaustive, so a variant added to the core arrives here as
/// a compile error rather than as a blank sentence.
func explain(_ error: Error) -> String? {
    guard let error = error as? AppError else { return nil }
    switch error {
    case let .Network(detail), let .Auth(detail), let .NotFound(detail),
         let .BadInput(detail), let .Storage(detail), let .Crypto(detail),
         let .PairExpired(detail), let .Keychain(detail), let .Update(detail),
         let .InsecureRelay(detail):
        return detail
    }
}
