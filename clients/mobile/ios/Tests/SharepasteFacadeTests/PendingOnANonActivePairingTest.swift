import Foundation
import SharepasteCore
import XCTest

/// A queue on a Pairing this phone has **switched away from**.
///
/// The Pairing's own card is the single place it is visible at all: the
/// History's count belongs to the Active Pairing, so the moment the device moves
/// on, Entries that were captured and never uploaded become invisible everywhere
/// else — kept, not sent, and not mentioned. What the facade has to get right is
/// that the count stays attached to the Pairing that owns it rather than to
/// whichever one is Active, and that is what is asserted here; the card that
/// draws it is a screen's business.
///
/// Manufacturing a queue takes a Pairing whose relay can be taken away, which
/// means a ``RelayProxy`` and a Pairing made through it. A short code carries the
/// *inviting* device's `server_url` inside its payload, so the inviter is the one
/// claimed through the proxy — it costs a single-use invite of its own — and the
/// phone then pairs by code exactly as it always does.
final class PendingOnANonActivePairingTest: XCTestCase {

    private var proxy: RelayProxy!
    private var phone: PhoneUnderTest!
    private var queued: String!
    private var movedTo: String!

    override func setUp() async throws {
        proxy = try RelayProxy.inFrontOfTheTestRelay()
        let behindTheProxy = try await Inviter.against(
            relay: proxy.url,
            label: "the inviting device behind the proxy"
        )
        phone = try PhoneUnderTest.open(databaseName: "pending-elsewhere-proof.db")
        queued = try await phone.pair(
            with: behindTheProxy,
            label: "the Pairing that keeps a queue"
        )
        movedTo = try await phone.pair(
            with: try await Inviter.shared(),
            label: "the Pairing this phone moves to"
        )
        try await phone.makeActive(queued)
    }

    override func tearDown() async throws {
        // The Pairing made through the proxy is deliberately **not** forgotten:
        // forgetting reaches the relay, the relay is behind a port this test has
        // just closed, and a teardown that hung for a timeout would charge every
        // run for the tidiness. The relay's database is a CI artefact that goes
        // with the runner.
        try? await phone.repo.stopAllSessions()
        try? await phone.repo.forgetPairing(userId: movedTo)
        phone = nil
        proxy.close()
        proxy = nil
    }

    func testAQueueOnAPairingThePhoneSwitchedAwayFromStaysWithThatPairing() async throws {
        _ = try await phone.enterForeground()
        try await phone.awaitInContact(queued)

        proxy.close()
        XCTAssertTrue(
            proxy.isUnreachable,
            "the proxy is still accepting connections, so nothing here is offline"
        )

        let stranded = "captured-and-never-sent-\(Int(Date().timeIntervalSince1970 * 1000))"
        let attempt = try await phone.repo.offerText(stranded)
        guard case .settled(let offeredTo, let outcome) = attempt else {
            return XCTFail("a paired phone must not report itself unpaired: \(attempt)")
        }
        XCTAssertEqual(offeredTo, queued)
        guard case .queued(let pending) = outcome else {
            return XCTFail("an Offer made offline is still taken, and this one was \(outcome)")
        }
        XCTAssertEqual(pending, 1, "the Entry is kept, and the queue is one deep")

        // Now move the device on. The Entry is still here, still un-uploaded,
        // and from this point the History's own count is about a different
        // Pairing.
        try await phone.makeActive(movedTo)
        _ = try await phone.enterForeground()

        await awaitCondition("the queue must stay with the Pairing that owns it") {
            let pairings = (try? await self.phone.repo.listPairings()) ?? []
            return pairings.first { $0.userId == self.queued }?.pending == 1
        }

        let pairings = try await phone.repo.listPairings()
        let left = try XCTUnwrap(pairings.first { $0.userId == queued })
        let active = try XCTUnwrap(pairings.first { $0.userId == movedTo })
        XCTAssertFalse(left.isActive, "the Pairing left behind is not the Active one")
        XCTAssertTrue(active.isActive)
        XCTAssertEqual(
            active.pending,
            0,
            "the Active Pairing has nothing queued, which is why the History says nothing "
                + "about the Entry that is stranded"
        )
        // And it still is not a fault. A Pairing holding a queue it cannot send
        // is resting, not broken — `disconnected` is what a Pairing no session
        // has ever run for reads too.
        XCTAssertEqual(left.status, .disconnected)
    }
}
