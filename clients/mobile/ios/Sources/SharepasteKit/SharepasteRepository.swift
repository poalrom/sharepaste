import Dispatch
import Foundation
import SharepasteCore

/// What became of an Offered Capture, including the one outcome the core has no
/// word for.
///
/// The core is asked about a Pairing it has been given the id of, so "nothing is
/// paired on this device" is not a failure it can report. It is not a failure at
/// all — it is the ordinary state of a fresh install — and it has to be a value,
/// because the callers that need it least are the ones with nowhere to put an
/// error: an App Intent's answer is its return value, and a thrown error there
/// reads to Shortcuts as a broken action rather than as a phone with no Pairing.
public enum OfferAttempt: Sendable, Equatable {
    case unpaired
    case settled(userId: String, outcome: OfferOutcome)
}

/// What became of a Recall Latest. See ``OfferAttempt`` for why `unpaired` is a
/// value.
public enum RecallAttempt: Sendable, Equatable {
    case unpaired

    /// The Entry is on the pasteboard.
    ///
    /// `fromCache` is the fact ADR 0007 says may never be silent: the round trip
    /// was attempted and failed, so what was handed over is only as new as this
    /// device already was. Carried as a field rather than as a second case
    /// because every caller has to answer it, and a case a caller can forget to
    /// match is a case that gets swept into a default.
    case done(userId: String, entryId: Int64, createdAt: Int64, fromCache: Bool)
}

/// The only thing in this application that touches the core.
///
/// The FFI boundary is blocking: every call runs the operation to completion on
/// the core's runtime and returns a plain value. So **no call may happen on the
/// main thread**, and the way that rule is kept is by having exactly one place
/// where calls happen at all — ``run(_:)`` below, which is `private`. A screen
/// that reached past this class for the `Sharepaste` object would have broken
/// the rule, and there is nothing at runtime that would say so.
///
/// The hop is to a **dedicated serial queue** rather than to the cooperative
/// thread pool an `actor` would use. Both are off the main thread, which is the
/// stated requirement, but the pool has one thread per core and Swift's runtime
/// assumes work on it makes forward progress. `recallLatest` performs a relay
/// round trip inside a blocking call; parking a pool thread on it for seconds at
/// a time is how a concurrency runtime is starved. A queue of our own has one
/// thread, cannot starve anything else, and serialises every crossing — which
/// the boundary wants anyway.
///
/// **Opening is a blocking call too** — it creates a SQLite connection, runs the
/// migrations and stands up a tokio runtime — so ``init`` returns immediately
/// with the facade still opening on that queue, and every method awaits it. A
/// caller on the main thread therefore never has to be a coroutine just to hold
/// one of these.
///
/// Events do not come back through here — they arrive on ``events``, because
/// they are raised by the core's own tasks rather than in reply to a call.
public final class SharepasteRepository: Sendable {

    /// The database file, inside the app's own container.
    public static let databaseName = "sharepaste.db"

    /// How long ``sendPending(userId:timeout:)`` holds a session open waiting
    /// for the upload.
    ///
    /// Long enough for a session to come up and one small POST to complete over
    /// a working connection; short enough that a phone with no route does not
    /// hold an App Intent open while somebody waits for nothing. Exceeding it is
    /// the ordinary offline outcome and not an error.
    public static let sendTimeout: Duration = .seconds(10)

    /// Everything the core raises, already off the thread that raised it.
    ///
    /// A stream of its own per call, because two things listen: the state holder
    /// for the life of the app, and ``sendPending(userId:timeout:)`` for the
    /// length of one drain.
    public func events() -> AsyncStream<CoreEvent> { sink.subscribe() }

    private let sink: StreamEventSink
    private let clipboard: Clipboard
    private let queue = DispatchQueue(label: "com.sharepaste.ios.ffi")
    private let opening: Task<Sharepaste, Error>
    private let held = HeldSessions()

    /// Open the core over this application's private storage.
    ///
    /// The path is handed *in*: the core never asks the OS where data lives.
    /// `directory` is the app's own container, which is what puts the cache
    /// behind the platform's Data Protection without a plaintext-at-rest toggle
    /// of our own — see ``LeakageControls`` for the half of that which is not
    /// free.
    ///
    /// `requireHttps` is the transport policy, and it is a parameter rather than
    /// a constant because the answer belongs to whoever is shipping. The app
    /// passes `TransportPolicy.requireHttps`, which is `true`; a facade test
    /// reaching a cleartext test relay passes `false` and says so at the call.
    /// Nothing on this platform constrains the core's Rust HTTP client — App
    /// Transport Security governs `URLSession` and reaches no further — so this
    /// flag is the only real enforcement.
    public init(
        directory: URL,
        requireHttps: Bool,
        databaseName: String = SharepasteRepository.databaseName,
        keychain: Keychain? = nil
    ) {
        let sink = StreamEventSink()
        // One pasteboard, handed both to the core and to `offerPasteboard`: a
        // Recall writes through the same object an Offer reads through, so there
        // is one set of rules about what this platform calls text.
        let pasteboard = IosPasteboard()
        let secrets = keychain ?? IosKeychain()
        let path = directory.appendingPathComponent(databaseName).path
        let queue = self.queue

        self.sink = sink
        self.clipboard = pasteboard
        self.opening = Task {
            try await withCheckedThrowingContinuation { continuation in
                queue.async {
                    continuation.resume(with: Result {
                        try Sharepaste.open(
                            dbPath: path,
                            keychain: secrets,
                            clipboard: pasteboard,
                            events: sink,
                            requireHttps: requireHttps
                        )
                    })
                }
            }
        }
    }

    // -- pairings and sessions ------------------------------------------------

    public func listPairings() async throws -> [PairingSummary] {
        try await run { try $0.listPairings() }
    }

    public func pairWithCode(code: String, deviceLabel: String) async throws -> PairedDevice {
        try await run { try $0.pairWithCode(code: code, deviceLabel: deviceLabel) }
    }

    public func forgetPairing(userId: String) async throws {
        try await run { try $0.forgetPairing(userId: userId) }
    }

    public func setActivePairing(userId: String) async throws {
        try await run { try $0.setActivePairing(userId: userId) }
    }

    public func activePairing() async throws -> String? {
        try await run { $0.activePairing() }
    }

    public func resumeActivePairing() async throws -> String? {
        try await run { try $0.resumeActivePairing() }
    }

    public func startSession(userId: String) async throws {
        try await run { try $0.startSession(userId: userId) }
        // Recorded after the call, so a start that raised — no route to the
        // relay, a pairing that would not unlock — is not a session this process
        // believes it holds.
        await held.insert(userId)
    }

    public func stopSession(userId: String) async throws {
        // Forgotten before the call rather than after it: the record says "this
        // process asked for a session and has not given it up", and asking is
        // exactly what it has just stopped doing.
        await held.remove(userId)
        try await run { $0.stopSession(userId: userId) }
    }

    public func stopAllSessions() async throws {
        await held.removeAll()
        try await run { $0.stopAllSessions() }
    }

    public func connectionState(userId: String) async throws -> ConnectionState {
        try await run { $0.connectionState(userId: userId) }
    }

    // -- history and clipboard ------------------------------------------------

    public func listHistory(
        userId: String,
        beforeId: Int64? = nil,
        limit: Int64 = 50
    ) async throws -> [Entry] {
        try await run { try $0.listHistory(userId: userId, beforeId: beforeId, limit: limit) }
    }

    /// The full plaintext of one Entry, with no pasteboard involvement.
    ///
    /// The screen never calls this: it has no reader pane and no search, and a
    /// row shows the Preview the core built. The Recall intent does, because its
    /// return value **is** the Entry — Shortcuts is what puts it on the
    /// pasteboard (ADR 0007), so the text has to cross this boundary once.
    ///
    /// `nil` covers both "no such Entry" and "this device cannot decrypt it";
    /// the Entry's `undecryptable` flag is what tells them apart.
    public func readEntry(userId: String, entryId: Int64) async throws -> String? {
        try await run { try $0.readEntry(userId: userId, entryId: entryId) }
    }

    public func recall(userId: String, entryId: Int64) async throws {
        try await run { try $0.recall(userId: userId, entryId: entryId) }
    }

    public func offer(userId: String, text: String) async throws -> OfferOutcome {
        try await run { try $0.offer(userId: userId, text: text) }
    }

    public func deleteEntry(userId: String, entryId: Int64) async throws {
        try await run { try $0.deleteEntry(userId: userId, entryId: entryId) }
    }

    public func clearHistory(userId: String) async throws {
        try await run { try $0.clearHistory(userId: userId) }
    }

    public func getContact(userId: String) async throws -> Contact {
        try await run { try $0.getContact(userId: userId) }
    }

    // -- the verbs, which assume no screen is open -----------------------------

    /// Offered Capture of whatever is on this device's pasteboard.
    ///
    /// The whole operation behind one call on the Active Pairing, because that is
    /// the granularity every caller wants and the only granularity some of them
    /// can express: an App Intent renders nothing and holds no state, so anything
    /// it had to look up first would be a second copy of this method.
    ///
    /// A pasteboard with nothing text-like on it is offered as the **empty
    /// string**, which the core's one capture filter answers with
    /// `SkipReason.nonText`. Deciding "there is no text here" a second time up
    /// here would be a second filter to keep in step with the first, and the
    /// first is the one with the tests.
    public func offerPasteboard() async throws -> OfferAttempt {
        let pasteboard = clipboard
        return try await run { core in
            // `try?` over a `String?`-returning throwing call gives `String??`;
            // both layers mean the same thing here — nothing text-like on the
            // pasteboard — and the core's one capture filter is what turns the
            // empty string into `SkipReason.nonText`.
            let text = ((try? pasteboard.readText()) ?? nil) ?? ""
            return try Self.offer(on: core, text: text)
        }
    }

    /// Offered Capture of text that arrived from somewhere other than the
    /// pasteboard — which, on iOS, is every Offer that did not come off a
    /// button.
    ///
    /// The Offer intent takes its text as a parameter, because *Shortcuts touches
    /// the pasteboard, never us* (ADR 0007). So this is the intent's entry point
    /// and ``offerPasteboard()`` is the screen's, and they share a body so the
    /// two cannot drift.
    public func offerText(_ text: String) async throws -> OfferAttempt {
        try await run { try Self.offer(on: $0, text: text) }
    }

    /// Recall Latest, on the Active Pairing.
    ///
    /// It **always** fetches — see the facade, which never short-circuits to the
    /// cache — and the fetch failing is not a failure of the operation: the
    /// newest cached Entry is still the best answer available. Which one it was
    /// comes back in `fromCache`, and saying so is the caller's obligation rather
    /// than an option.
    public func recallLatestOnActivePairing() async throws -> RecallAttempt {
        try await run { core in
            guard let userId = Self.theActivePairing(core) else { return .unpaired }
            let recalled = try core.recallLatest(userId: userId)
            return .done(
                userId: userId,
                entryId: recalled.entryId,
                createdAt: recalled.createdAt,
                fromCache: recalled.source == .cache
            )
        }
    }

    /// The Preview of one Entry, for a Recall that has to say what it handed
    /// over.
    ///
    /// `RecallAttempt.done` carries no plaintext and must not start to: the core
    /// has already put the text on the pasteboard, and a secret nothing needs is
    /// a secret something logs. A Preview is a different thing — the facade's own
    /// one-line rendering, already normalised and capped, and the same string a
    /// History row shows.
    ///
    /// **Answers `nil` rather than throwing, and that is the contract.** The
    /// Entry is on the pasteboard by the time anyone asks; a failed read is a
    /// Receipt with less to say, never a Recall reported as a failure.
    public func previewOf(userId: String, entryId: Int64) async -> String? {
        let entries = try? await listHistory(userId: userId)
        return entries?.first { $0.id == entryId }?.preview
    }

    /// Bring a session up, wait for the pending queue to drain, and put it back
    /// down.
    ///
    /// **What this is for.** `offer` enqueues and nudges the uploader, and the
    /// uploader lives on a session. A screen open in front of somebody always has
    /// one; an App Intent does not. Without this, "Offer without opening the app"
    /// would produce an Entry that sits in a queue until the app *is* opened,
    /// which is not the feature: a person offers something on their phone in
    /// order to use it on their laptop a moment later.
    ///
    /// **Why this is not the background work ADR 0007 forbids.** The rule is that
    /// nothing runs while nobody is looking. This runs because somebody ran a
    /// shortcut; `timeout` bounds the wait; and the session is brought back down
    /// before the intent returns.
    ///
    /// **A session an open screen is already holding is left alone**, neither
    /// started nor stopped. An intent can be run while the app is in front, and
    /// `startSession` cancels whichever session it replaces — so doing both would
    /// leave a visible History receiving no Entries with no foreground edge
    /// coming to restore it.
    ///
    /// Answers whether the queue emptied. Not emptying is the ordinary offline
    /// outcome and not an error — the Entry is kept, the pending count is on the
    /// History Screen, and the next foreground sends it.
    @discardableResult
    public func sendPending(
        userId: String,
        timeout: Duration = SharepasteRepository.sendTimeout
    ) async -> Bool {
        let ours = await !held.contains(userId)
        if ours {
            guard (try? await startSession(userId: userId)) != nil else { return false }
        }
        defer {
            // The teardown is not the caller's to revoke: an intent cancelled
            // mid-drain must still put down the session it brought up, or sync
            // outlives the press that authorised it, which is the thing ADR 0007
            // forbids arrived at by accident.
            if ours {
                Task { try? await self.stopSession(userId: userId) }
            }
        }

        // The subscription is established **first**, and the read closes the gap
        // an event cannot: a session an open screen holds can empty the queue
        // before anything here is listening, and the count that would have ended
        // the wait was then emitted with nobody to hear it. Anything emptied
        // before the read is in the read; anything emptied after it arrives as an
        // event.
        let stream = sink.subscribe()
        if let pending = try? await pendingOn(userId: userId), pending == 0 { return true }

        return await withTaskGroup(of: Bool.self) { group in
            group.addTask {
                for await event in stream {
                    if case let .pendingCount(eventUserId, count) = event,
                       eventUserId == userId, count == 0 {
                        return true
                    }
                }
                return false
            }
            group.addTask {
                try? await Task.sleep(for: timeout)
                return false
            }
            let drained = await group.next() ?? false
            group.cancelAll()
            return drained
        }
    }

    private func pendingOn(userId: String) async throws -> Int64? {
        try await run { core in
            try core.listPairings().first { $0.userId == userId }?.pending
        }
    }

    /// Resolve the Active Pairing and offer `text` to it.
    ///
    /// Static, and taking the core, so both callers do their work inside one
    /// ``run(_:)`` block: the pairing lookup and the Offer are then a single trip
    /// across the boundary.
    private static func offer(on core: Sharepaste, text: String) throws -> OfferAttempt {
        guard let userId = theActivePairing(core) else { return .unpaired }
        return .settled(userId: userId, outcome: try core.offer(userId: userId, text: text))
    }

    /// The Active Pairing, in a process that may never have resumed one.
    ///
    /// **`activePairing()` is an in-memory read, and a cold process has nothing
    /// in memory.** The core latches the Active Pairing when
    /// `resumeActivePairing()` loads it from storage, which is what the
    /// foreground edge does. An App Intent has no foreground edge: its process
    /// may have been created by the intent, it opens the facade and asks
    /// immediately, and `activePairing()` answers `nil` for a phone that is
    /// perfectly well paired.
    ///
    /// A failed resume is not an error to raise. It means there is no Pairing to
    /// resume, or the keychain would not open — and "unpaired" is the honest
    /// answer to both from a surface with nowhere to put an error.
    private static func theActivePairing(_ core: Sharepaste) -> String? {
        core.activePairing() ?? (try? core.resumeActivePairing()) ?? nil
    }

    /// The one place an FFI call happens.
    ///
    /// Everything above is a one-line wrapper around this, which is the point:
    /// the boundary's rule is "not on the main thread", and a rule with one place
    /// to check is a rule that holds.
    private func run<T: Sendable>(_ call: @escaping @Sendable (Sharepaste) throws -> T) async throws -> T {
        let core = try await opening.value
        return try await withCheckedThrowingContinuation { continuation in
            queue.async {
                continuation.resume(with: Result { try call(core) })
            }
        }
    }
}

/// The Pairings this process is holding a session for.
///
/// Not a second opinion about core state — a record of **this shell's own
/// acts**, which is a different fact and the one ``SharepasteRepository/sendPending(userId:timeout:)``
/// needs: an intent has to put down the session it brought up and has to leave
/// alone the one an open screen is using. Asking the core would not answer it
/// anyway: `connectionState` reads `disconnected` for a Pairing no session has
/// ever run for, for one whose session was stopped, and for one merely out of
/// contact.
private actor HeldSessions {
    private var userIds: Set<String> = []

    func insert(_ userId: String) { userIds.insert(userId) }
    func remove(_ userId: String) { userIds.remove(userId) }
    func removeAll() { userIds.removeAll() }
    func contains(_ userId: String) -> Bool { userIds.contains(userId) }
}
