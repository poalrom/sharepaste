import Foundation
import SharepasteCore
import XCTest

/// Events raised by the session loop's **own** tasks arrive in Swift.
///
/// This is the awkward crossing, and the reason it gets a live relay rather than
/// a stub. The events under test are not replies to a call: the SSE reader, the
/// uploader and the contact stamp all run on worker threads of the core's
/// private tokio runtime, attached to no Swift executor and to no run loop. If
/// the generated binding could not reach Swift from there, every test that only
/// ever calls *into* the core would still pass and the app would be deaf.
///
/// The relay is the one the CI job starts on the runner with `npm start`. From
/// the simulator that is the host's own loopback — there is no emulator alias to
/// special-case, which is the one place this arrangement is simpler than
/// Android's.
final class SessionEventsTest: XCTestCase {

    private var sink: RecordingSink!
    private var core: Sharepaste!
    private var pairedUserId: String?

    override func setUpWithError() throws {
        Suite.assertRelayIsReachable()
        let directory = try freshDatabase(named: SessionEventsTest.database)
        sink = RecordingSink()
        core = try Sharepaste.open(
            dbPath: directory.appendingPathComponent(SessionEventsTest.database).path,
            keychain: InMemoryKeychain(),
            clipboard: NoClipboard(),
            events: sink,
            // The test relay is plain HTTP, so this suite says so. The shipped
            // app passes `true` — see `TransportPolicyTest`.
            requireHttps: false
        )
    }

    override func tearDown() {
        core?.stopAllSessions()
        if let pairedUserId { try? core?.forgetPairing(userId: pairedUserId) }
        core = nil
    }

    func testALiveSessionRaisesEventsOnItsOwnThreads() async throws {
        // The calling thread is read here, before anything is started, because
        // half the criterion is that neither event came back on it.
        let caller = RecordingSink.currentThread()

        let paired = try core.pairWithInvite(
            serverUrl: Suite.relayURL,
            token: try Suite.nextInvite(claiming: "SessionEventsTest's own facade"),
            deviceLabel: "simulator under test"
        )
        pairedUserId = paired.userId

        // Brings the session up: the SSE reader, the uploader and the contact
        // stamp all go onto the core's runtime here.
        try core.setActivePairing(userId: paired.userId)
        try core.startSession(userId: paired.userId)

        let online = await sink.first {
            if case .connectionState(_, let state, _) = $0 { return state == .online }
            return false
        }
        let onlineEvent = try XCTUnwrap(
            online,
            "no connectionState(online) reached Swift in \(Int(Suite.timeout))s; "
                + "events seen: \(sink.names)"
        )

        // Confirmed, not asserted on: `connectionState` is a live reading and a
        // session that reached `online` can be back in `connecting` a moment
        // later after a reconnect. The criterion is the event above and the
        // thread it arrived on; this only says the session is genuinely up.
        await awaitCondition("the core agrees the session is up") {
            self.core.connectionState(userId: paired.userId) == .online
        }

        // Hand the protocol some text; the uploader task takes it from here and
        // reports the queue depth back through the same sink.
        let outcome = try core.offer(
            userId: paired.userId,
            text: "instrumented-\(Int(Date().timeIntervalSince1970 * 1000))"
        )
        guard case .queued = outcome else {
            return XCTFail("an Offer must be honoured, and this one was \(outcome)")
        }

        let drained = await sink.first {
            if case .pendingCount(_, let count) = $0 { return count == 0 }
            return false
        }
        let drainedEvent = try XCTUnwrap(
            drained,
            "the uploader never reported an empty queue; events seen: \(sink.names)"
        )

        // The point of the whole test. Neither event came back on the thread
        // that called in, and neither arrived on the main thread: they were
        // raised by the SSE reader and the uploader, on threads of the core's
        // own runtime, with nothing on this side having attached them to
        // anything.
        for (what, received) in [
            ("connectionState(online)", onlineEvent),
            ("pendingCount(0)", drainedEvent),
        ] {
            XCTAssertNotEqual(
                received.thread,
                caller,
                "\(what) arrived on the calling thread, which proves nothing about the session loop"
            )
            XCTAssertFalse(
                received.thread.hasPrefix("main("),
                "\(what) arrived on the main thread"
            )
        }
    }

    private static let database = "session-proof.db"
}
