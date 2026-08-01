import Foundation
import OSLog
import SharepasteCore

/// Where the core's events arrive, and the one place the thread they arrive on
/// is dealt with.
///
/// ``emit(event:)`` is called from the session loop's **own** tokio tasks — the
/// SSE reader, the uploader, the pair poll. Those are worker threads of the
/// core's private runtime, attached to nothing on this side. Two rules follow,
/// and both are load-bearing:
///
/// * **Never block.** The core holds its connection-state lock across this call.
///   Anything that waits here stalls the session. `AsyncStream.Continuation.yield`
///   does not wait: with a bounded buffer it drops rather than suspends.
/// * **Never touch UI state.** This is not the main thread. Handing the event to
///   a stream is what moves it — the consumer is `@MainActor`, so the marshalling
///   happens once, at the consumer, rather than at every call site.
///
/// **It fans out**, which is not decoration. A single `AsyncStream` has a single
/// consumer: a second `for await` over the same stream steals events from the
/// first rather than seeing copies of them. Two things listen here — the state
/// holder, always, and `sendPending` for as long as one drain lasts — so each
/// ``subscribe()`` gets a stream of its own. This is Android's `SharedFlow` with
/// `replay = 0`, and the missing replay is the same deliberate choice: an event
/// is what just happened, and a subscriber that was not there did not miss a
/// fact, it missed a moment. The facts are all readable from the core.
public final class StreamEventSink: EventSink, @unchecked Sendable {

    private let lock = NSLock()
    private var subscribers: [Int: AsyncStream<CoreEvent>.Continuation] = [:]
    private var nextToken = 0

    public init() {}

    /// A stream of everything raised from now on.
    ///
    /// Finishing it — by cancelling the task that iterates — deregisters it. A
    /// drain that times out therefore costs nothing afterwards.
    public func subscribe() -> AsyncStream<CoreEvent> {
        // Deep enough that a burst of backfill during a reconnect is absorbed
        // while the consumer is still being resumed. `bufferingOldest` rather
        // than `bufferingNewest`: the list is built by replaying arrivals in
        // order, so the event to drop is the newest one nobody has caught up to.
        AsyncStream(bufferingPolicy: .bufferingOldest(256)) { continuation in
            let token = lock.withLock {
                let token = nextToken
                nextToken += 1
                subscribers[token] = continuation
                return token
            }
            continuation.onTermination = { [weak self] _ in
                guard let self else { return }
                _ = self.lock.withLock { self.subscribers.removeValue(forKey: token) }
            }
        }
    }

    public func emit(event: CoreEvent) {
        let targets = lock.withLock { Array(subscribers.values) }
        for continuation in targets {
            if case .dropped = continuation.yield(event) {
                // Dropped rather than suspended, deliberately: a stalled consumer
                // must not stall the protocol. Logged because a dropped event is
                // a desynchronised UI and someone has to be able to see that it
                // happened — and logged by *case name*, never by value, because
                // `entryAdded` embeds an Entry and `pairShortcode` is the pairing
                // secret for the next two minutes.
                Logger(subsystem: "com.sharepaste.ios", category: "events")
                    .warning("event buffer full; dropped \(Self.name(of: event), privacy: .public)")
            }
        }
    }

    /// The case name and nothing else. See ``emit(event:)``.
    private static func name(of event: CoreEvent) -> String {
        switch event {
        case .pairingAdded: "pairingAdded"
        case .pairingRemoved: "pairingRemoved"
        case .activePairingChanged: "activePairingChanged"
        case .connectionState: "connectionState"
        case .entryAdded: "entryAdded"
        case .entryDeleted: "entryDeleted"
        case .historyChanged: "historyChanged"
        case .pendingCount: "pendingCount"
        case .contact: "contact"
        case .pairShortcode: "pairShortcode"
        case .pairClaimed: "pairClaimed"
        case .pairExpired: "pairExpired"
        }
    }
}
