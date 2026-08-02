import Foundation
import SharepasteCore
import XCTest

/// Backgrounding tears the session down; resuming re-opens it and backfills.
///
/// That pair of sentences is the *entire* sync model (ADR 0007), so it is worth
/// a test against a live relay rather than a stub: no background modes, no push,
/// no processing task, and on this platform that is forced as well as chosen —
/// both are entitlement-gated and a free Personal Team has neither.
///
/// The two edges are driven through the calls the state holder makes, so this is
/// not a parallel implementation of the lifecycle; it is the lifecycle, with a
/// facade of its own so it can reach the cleartext test relay.
///
/// The backfill half is the one that matters most. An Entry offered by the other
/// device *while this phone is backgrounded* has to be there when the phone
/// comes back — and nowhere else, because nothing was listening.
final class SessionLifecycleTest: XCTestCase {

    private var phone: PhoneUnderTest!

    override func setUpWithError() throws {
        phone = try PhoneUnderTest.open(databaseName: "lifecycle-proof.db")
    }

    override func tearDown() async throws {
        await phone.close()
        phone = nil
    }

    func testBackgroundingTearsTheSessionDownAndResumingReOpensItAndBackfills() async throws {
        let other = try await Inviter.shared()
        let userId = try await phone.pair(with: other, label: "lifecycle test phone")

        // --- the first foreground edge ---------------------------------------
        let resumed = try await phone.enterForeground()
        XCTAssertEqual(resumed, userId)
        try await phone.awaitInContact(userId)

        // --- and the edge back down -------------------------------------------
        try await phone.leaveForeground()
        // The core reads `disconnected` from here on: `stopAllSessions` walks
        // every session it stopped to that state before it returns, because a
        // Pairing row rendering `online` for a session that no longer exists was
        // its own bug. What survives the teardown is **Contact** — the last
        // moment this device had a live connection, flushed to the account row
        // on the way out — so the next resume has something to render before the
        // relay answers.
        let state = try await phone.repo.connectionState(userId: userId)
        XCTAssertEqual(state, .disconnected, "a stopped session must not still read online")
        let contact = try await phone.repo.getContact(userId: userId)
        XCTAssertNotNil(
            contact.lastContactAt,
            "Contact is the reading that survives a teardown, and the screen has nothing "
                + "else to show until the relay answers again"
        )

        // --- something happens while the phone is not listening ---------------
        //
        // This is the real proof the stream came down, and it is stronger than
        // any status enum: the other device puts an Entry on the relay and
        // nothing reaches this phone, because nothing here is listening.
        let offered = "offered-while-backgrounded-\(Int(Date().timeIntervalSince1970 * 1000))"
        try await other.offerAndWaitForUpload(offered)
        try await Task.sleep(nanoseconds: SessionLifecycleTest.backgroundWindow)

        let whileResting = try await phone.repo.listHistory(userId: userId)
        XCTAssertFalse(
            whileResting.contains { $0.preview == offered },
            """
            the Entry offered while the session was down arrived anyway; the teardown did not \
            happen. That is the whole of ADR 0007 — nothing syncs while the app is closed.
            """
        )

        // --- and back up again -------------------------------------------------
        _ = try await phone.enterForeground()
        try await phone.awaitInContact(userId)

        let backfilled = try await phone.awaitEntry(
            "the Entry offered while backgrounded must be backfilled",
            userId: userId,
            previewing: offered
        )
        XCTAssertFalse(backfilled.undecryptable, "a decryptable Entry must not be marked")
    }

    /// How long the phone stays "backgrounded" while the other device offers.
    ///
    /// The Entry is already on the relay before the wait starts — that is what
    /// `offerAndWaitForUpload` is for — so this is only the window in which a
    /// session that had *not* come down would have brought it in. Five seconds
    /// is far longer than an SSE frame takes over loopback.
    private static let backgroundWindow: UInt64 = 5_000_000_000
}
