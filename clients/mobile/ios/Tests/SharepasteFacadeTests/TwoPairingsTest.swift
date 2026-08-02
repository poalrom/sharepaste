import Foundation
import SharepasteCore
import SharepasteKit
import XCTest

/// A phone holding two Pairings at once, against the live relay.
///
/// Two Pairings means two **Users** — a Pairing is "the local record binding
/// this machine to one user on one relay" (CONTEXT.md), so pairing twice against
/// the same inviting device would give one Pairing two Devices and prove
/// nothing. ``Inviter/second()`` is the second User, claimed once for the whole
/// run.
///
/// **The Viewed Pairing does not appear here.** On the desktop and on Android it
/// is the other half of this distinction: what is on screen, changing nothing,
/// forgotten when the window closes. Nothing writes it, no core call answers it,
/// and there is no screen on this target — so what is testable at the facade is
/// the half that matters anyway: what the phone syncs and captures to is the
/// **Active** Pairing and moves only when something moves it.
final class TwoPairingsTest: XCTestCase {

    private var phone: PhoneUnderTest!
    private var synced: String!
    private var held: String!

    override func setUp() async throws {
        phone = try PhoneUnderTest.open(databaseName: "two-pairings-proof.db")
        synced = try await phone.pair(
            with: try await Inviter.shared(),
            label: "the Pairing this phone syncs"
        )
        held = try await phone.pair(
            with: try await Inviter.second(),
            label: "the Pairing it merely holds"
        )
        // Pairing makes the newest one Active, so the one this suite calls the
        // Active Pairing has to be chosen back.
        try await phone.makeActive(synced)
    }

    override func tearDown() async throws {
        await phone?.close()
        phone = nil
    }

    /// Capture follows the Active Pairing, and the Pairing nothing syncs
    /// receives nothing.
    ///
    /// Two claims, each asserted against something a mistake could not fake: an
    /// Offer lands in the Active Pairing's History and nowhere else, and an
    /// Entry put on the relay for the *other* User never reaches this phone at
    /// all while that Pairing is not the Active one.
    func testWhatIsSyncedAndCapturedFollowsTheActivePairingOnly() async throws {
        // Both Entries go onto the relay **before** this phone is opened, so the
        // backfill is what has to find them. An Entry offered after the session
        // is up would depend on the SSE stream being subscribed already, and the
        // session reports online a beat before it subscribes. A phone that is
        // closed has no such window.
        let delivered = "reaches-the-active-pairing-\(Self.stamp)"
        try await Inviter.shared().offerAndWaitForUpload(delivered)
        let unheard = "never-reaches-the-pairing-nobody-syncs-\(Self.stamp)"
        try await Inviter.second().offerAndWaitForUpload(unheard)

        _ = try await phone.enterForeground()
        try await phone.awaitInContact(synced)

        // The positive control: the Active Pairing does receive.
        _ = try await phone.awaitEntry(
            "the Active Pairing's Entry must arrive",
            userId: synced,
            previewing: delivered
        )

        let stillActive = try await phone.repo.activePairing()
        XCTAssertEqual(
            stillActive,
            synced,
            "nothing in this test asked for the Active Pairing to move"
        )
        let onTheOther = try await phone.repo.listHistory(userId: held)
        XCTAssertFalse(
            onTheOther.contains { $0.preview == unheard },
            "the Pairing nothing syncs must hold none of the Entries put on the relay for it"
        )
        let states = try await phone.repo.listPairings()
        XCTAssertEqual(states.count, 2)
        XCTAssertEqual(states.first { $0.userId == synced }?.isActive, true)
        XCTAssertEqual(states.first { $0.userId == held }?.isActive, false)

        // Capture, too, goes to the Active Pairing and to nothing else.
        let offered = "offered-while-two-are-held-\(Self.stamp)"
        let attempt = try await phone.repo.offerText(offered)
        guard case .settled(let offeredTo, _) = attempt else {
            return XCTFail("a paired phone must not report itself unpaired: \(attempt)")
        }
        XCTAssertEqual(offeredTo, synced)

        // Polled, not read once: an Offer is queued the moment the facade
        // answers and reaches this cache only after the uploader has sent it and
        // the session's own stream has brought it back.
        _ = try await phone.awaitEntry(
            "the Offer must reach the Active Pairing's cache",
            userId: synced,
            previewing: offered
        )
        let heldHistory = try await phone.repo.listHistory(userId: held)
        XCTAssertFalse(
            heldHistory.contains { $0.preview == offered },
            "and must not go to the Pairing that merely happens to be held"
        )
    }

    /// Forgetting a Pairing takes its Entries, its **key material** and its
    /// **token**, and promotes another to Active.
    ///
    /// The keychain is asserted directly, before and after: a row disappearing
    /// from a list is not the claim. ``IosKeychain`` is the app's real Keychain
    /// Services store and `<user>:key` / `<user>:token` are the accounts the core
    /// writes, so this reads exactly what the facade wrote.
    func testForgettingAPairingTakesItsEntriesKeyAndTokenAndPromotesAnother() async throws {
        _ = try await phone.enterForeground()
        try await phone.awaitInContact(synced)

        let doomed = "erased-with-its-pairing-\(Self.stamp)"
        _ = try await phone.repo.offerText(doomed)
        _ = try await phone.awaitEntry(
            "something for the Pairing to lose",
            userId: synced,
            previewing: doomed
        )

        let keychain = IosKeychain()
        XCTAssertNotNil(
            try keychain.get(account: "\(synced!):key"),
            "no key to erase means this test proves nothing"
        )
        XCTAssertNotNil(try keychain.get(account: "\(synced!):token"), "no token to erase either")

        try await phone.repo.forgetPairing(userId: synced)

        XCTAssertNil(
            try keychain.get(account: "\(synced!):key"),
            "the key material must be gone from the keychain"
        )
        XCTAssertNil(try keychain.get(account: "\(synced!):token"), "and so must the token")
        let promoted = try await phone.repo.activePairing()
        XCTAssertEqual(
            promoted,
            held,
            "the core must have promoted the remaining Pairing to Active"
        )
        let remaining = try await phone.repo.listPairings()
        XCTAssertEqual(remaining.count, 1)
        XCTAssertEqual(remaining.first?.userId, held)

        // `listHistory` is a query over Entries rather than a lookup of the
        // Pairing, so a forgotten one answers with an empty History rather than
        // a `NotFound` — measured, not assumed. Empty is the claim that matters:
        // the cached Entries went with the key that could read them.
        let leftBehind = try await phone.repo.listHistory(userId: synced)
        XCTAssertTrue(
            leftBehind.isEmpty,
            "the forgotten Pairing still holds \(leftBehind.count) Entries"
        )
    }

    /// Clearing a History erases that Pairing's Entries and **leaves the other
    /// Pairing's alone.**
    ///
    /// The confirmation that names the Pairing is a screen's business; this is
    /// the other half of the same claim, which is that the name is not
    /// decorative — the erase really is scoped to the Pairing that was named.
    func testClearingOnePairingsHistoryLeavesTheOthersEntriesAlone() async throws {
        _ = try await phone.enterForeground()
        try await phone.awaitInContact(synced)

        let cleared = "cleared-from-the-active-pairing-\(Self.stamp)"
        _ = try await phone.repo.offerText(cleared)
        _ = try await phone.awaitEntry(
            "something to clear",
            userId: synced,
            previewing: cleared
        )

        // And an Entry on the other Pairing, brought in by making it Active for
        // as long as it takes to sync one. Put down between switches, so the
        // sessions do not overlap: `makeActive` stops them all, and the next
        // resume brings up exactly the one the core now calls Active.
        let kept = "kept-on-the-other-pairing-\(Self.stamp)"
        try await Inviter.second().offerAndWaitForUpload(kept)
        try await phone.leaveForeground()
        try await phone.makeActive(held)
        _ = try await phone.enterForeground()
        try await phone.awaitInContact(held)
        _ = try await phone.awaitEntry(
            "the other Pairing's Entry must be cached first",
            userId: held,
            previewing: kept
        )
        try await phone.leaveForeground()
        try await phone.makeActive(synced)
        _ = try await phone.enterForeground()
        try await phone.awaitInContact(synced)

        try await phone.repo.clearHistory(userId: synced)

        let syncedHistory = try await phone.repo.listHistory(userId: synced)
        XCTAssertFalse(
            syncedHistory.contains { $0.preview == cleared },
            "the named Pairing's Entries must be gone"
        )
        let heldHistory = try await phone.repo.listHistory(userId: held)
        XCTAssertTrue(
            heldHistory.contains { $0.preview == kept },
            "and the other Pairing's must not be touched"
        )
    }

    private static var stamp: Int {
        Int(Date().timeIntervalSince1970 * 1000)
    }
}
