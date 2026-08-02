import Foundation
import SharepasteCore
import SharepasteKit
import XCTest

/// The round trip that justifies the client at all.
///
/// Copy on the laptop, open the phone, see it and Recall it onto the phone's
/// pasteboard; Offer from the phone, see it on the laptop. Against the live
/// relay with a second real device on the other end of the Pairing — a stub on
/// either side would prove nothing about the bytes.
///
/// Everything is driven through the shipped ``SharepasteRepository``, and the
/// pasteboard assertions read the ``TestPasteboard`` the phone handed it.
/// `UIPasteboard` is not reachable from a host-less test bundle at all; the
/// reasoning, and the hang that established it, are on that type. What is under
/// test is unchanged by the substitution: the Clipboard trait is a foreign
/// trait precisely so that the shell decides what a pasteboard is, and the
/// crossing being asserted is the core reaching a Swift object.
///
/// **A Recall's write is the core's, not the shell's.** Nothing in
/// ``SharepasteRepository`` writes the pasteboard on a Recall —
/// `recall_latest` calls `write_clipboard` inside the core
/// (`clients/core/src/facade.rs`), so the assertion below that the plaintext
/// arrived at the Clipboard trait is evidence about the *core's* behaviour.
/// That matters beyond this test: ticket 07 says neither App Intent touches the
/// pasteboard, and on this path one of them does.
///
/// **What does not port.** On Android these tests now assert a Receipt and the
/// Preview it names. A Receipt is a Toast — it is UI, and spec row 10 buys none
/// — so what is asserted here is the facade half beneath it: that the Recall
/// handed back the Entry only the relay had, that it was not served from the
/// cache, and that the plaintext reached the pasteboard. The Preview the Receipt
/// would have shown is checked too, through
/// ``SharepasteRepository/previewOf(userId:entryId:)``, because that method
/// exists for the Receipt and would otherwise have no test at all.
///
/// Each test pairs its own phone into the run's one inviting device, over a
/// database of its own, so nothing here depends on the order XCTest picks.
final class RoundTripTest: XCTestCase {

    private var phone: PhoneUnderTest!

    override func tearDown() async throws {
        await phone?.close()
        phone = nil
    }

    /// Something copied on the other device reaches the phone on a resume, and
    /// Recalling it puts it on this phone's pasteboard.
    ///
    /// The Entry is put on the relay while the phone is closed, which is the
    /// shape of the real thing: sync is foreground-only, so the first resume is
    /// what brings it in.
    func testAnEntryFromAnotherDeviceArrivesOnResumeAndRecallsOntoThisPasteboard() async throws {
        let other = try await Inviter.shared()
        let userId = try await openPhone("backfill-proof.db", label: "round-trip test phone")

        let copied = "https://example.invalid/from-the-laptop-\(Self.stamp)"
        try await other.offerAndWaitForUpload(copied)

        _ = try await phone.enterForeground()
        try await phone.awaitInContact(userId)
        let arrived = try await phone.awaitEntry(
            "the other device's Entry must be backfilled",
            userId: userId,
            previewing: copied
        )
        XCTAssertFalse(arrived.undecryptable, "a decryptable Entry must not be marked")

        let ownDeviceId = try await phone.repo.listPairings()
            .first { $0.userId == userId }?.deviceId
        XCTAssertNotNil(
            ownDeviceId,
            "this phone's own Device id has to be known, or every row claims an Origin"
        )
        XCTAssertNotEqual(
            arrived.deviceId,
            ownDeviceId,
            "this Entry came from the other device, so it has an Origin to show"
        )

        // Overwritten first, so a pass cannot be an accident of whatever the
        // pasteboard already happened to hold.
        phone.pasteboard.put("something else entirely")
        try await phone.repo.recall(userId: userId, entryId: arrived.id)

        XCTAssertEqual(
            phone.pasteboard.written,
            [copied],
            "the Recall must hand that Entry's plaintext to the pasteboard, exactly once"
        )
        let preview = await phone.repo.previewOf(userId: userId, entryId: arrived.id)
        XCTAssertEqual(
            preview,
            copied,
            "the Receipt names the Entry by its Preview, and this is the read behind it"
        )
    }

    /// Something Offered on the phone reaches the relay and the other device can
    /// read it.
    ///
    /// Read back with the other device's own `recallLatest`, which always
    /// performs the round trip — so the assertion is about what the relay holds
    /// now, not about anything either device had cached.
    func testAnEntryOfferedHereIsReadableByTheOtherDevice() async throws {
        let other = try await Inviter.shared()
        let userId = try await openPhone("offer-proof.db", label: "offer test phone")

        _ = try await phone.enterForeground()
        try await phone.awaitInContact(userId)

        let offered = "offered-from-the-phone-\(Self.stamp)"
        phone.pasteboard.put(offered)
        let attempt = try await phone.repo.offerPasteboard()
        guard case .settled(let offeredTo, let outcome) = attempt else {
            return XCTFail("a paired phone must not report itself unpaired: \(attempt)")
        }
        XCTAssertEqual(offeredTo, userId, "an Offer goes to the Active Pairing")
        guard case .queued = outcome else {
            return XCTFail("the Offer must be taken, and this one was \(outcome)")
        }

        let seen = await other.awaitNewestOnRelay(offered)
        XCTAssertTrue(seen, "the phone's Offer never reached the relay")
    }

    /// **Recall Latest fetches. It does not read the cache.**
    ///
    /// Proven the only way it can be: the phone is put down, the other device
    /// puts an Entry on the relay that this phone's cache has *never* held, and
    /// Recall Latest is asked with no session running. If it read the cache it
    /// would hand over the older Entry; it hands over the new one.
    ///
    /// The second half is `fromCache`, which is the fact ADR 0007 says may never
    /// be silent. A round trip that succeeded must report `false` — an
    /// authoritative answer reported as stale is a phone crying wolf about the
    /// one thing it has to be believed on.
    func testRecallLatestFetchesAnEntryThisPhoneHasNeverCached() async throws {
        let other = try await Inviter.shared()
        let userId = try await openPhone("recall-latest-proof.db", label: "recall latest phone")

        let older = "older-entry-\(Self.stamp)"
        try await other.offerAndWaitForUpload(older)
        _ = try await phone.enterForeground()
        try await phone.awaitInContact(userId)
        _ = try await phone.awaitEntry(
            "the older Entry must be cached first",
            userId: userId,
            previewing: older
        )

        // Put the phone down. Nothing syncs now, which is the entire sync model.
        try await phone.leaveForeground()

        let newer = "newer-entry-the-cache-has-never-seen-\(Self.stamp)"
        try await other.offerAndWaitForUpload(newer)
        try await Task.sleep(nanoseconds: Self.settle)

        let cached = try await phone.repo.listHistory(userId: userId)
        XCTAssertFalse(
            cached.contains { $0.preview == newer },
            "the newer Entry reached this phone's cache; with a live session this test would "
                + "prove nothing about fetching"
        )
        XCTAssertEqual(cached.first?.preview, older, "a cache read would hand over this one")

        phone.pasteboard.put("something else entirely")
        let attempt = try await phone.repo.recallLatestOnActivePairing()
        guard case .done(let recalledFor, let entryId, _, let fromCache) = attempt else {
            return XCTFail("a paired phone must not report itself unpaired: \(attempt)")
        }
        XCTAssertEqual(recalledFor, userId)
        XCTAssertFalse(
            fromCache,
            "the relay was reachable, so this answer is authoritative and must not be reported "
                + "as stale"
        )
        XCTAssertEqual(
            phone.pasteboard.written.last,
            newer,
            "Recall Latest read the cache instead of fetching"
        )
        let preview = await phone.repo.previewOf(userId: userId, entryId: entryId)
        XCTAssertEqual(
            preview,
            newer,
            "the Receipt must name the Entry only the relay had; a cache read would have named "
                + "the older one"
        )
    }

    /// An Offer is honoured with capture disabled.
    ///
    /// `captureEnabled` governs Watched Capture, which a phone never performs.
    /// Every Entry a phone produces is an Offered Capture — the person handed
    /// the content over — and refusing that because of a desktop's watcher
    /// setting would be indefensible.
    ///
    /// The setting is written **before** the phone exists, through a facade of
    /// this test's own that is released before the repository opens. The
    /// chokepoint exposes no `updateSettings` — nothing on this phone's screens
    /// can change a desktop's watcher, which is the right surface — and two
    /// facades open on one SQLite file is not something to arrange on purpose.
    func testAnOfferIsHonouredWithCaptureDisabled() async throws {
        let database = "capture-disabled-proof.db"
        let directory = try freshDatabase(named: database)
        try autoreleasepool {
            let core = try Sharepaste.open(
                dbPath: directory.appendingPathComponent(database).path,
                keychain: InMemoryKeychain(),
                clipboard: NoClipboard(),
                events: SilentSink(),
                requireHttps: false
            )
            // Only `captureEnabled` is patched. `autostart`, `hotkey` and
            // `updateCheckEnabled` are desktop concerns carried on the same row,
            // and a phone that named them would clear them.
            let settings = try core.updateSettings(patch: SettingsPatch(captureEnabled: false))
            XCTAssertFalse(
                settings.captureEnabled,
                "capture must really be off for this to prove anything"
            )
        }

        let other = try await Inviter.shared()
        phone = try PhoneUnderTest.open(databaseName: database, fresh: false)
        let userId = try await phone.pair(with: other, label: "capture disabled phone")

        _ = try await phone.enterForeground()
        try await phone.awaitInContact(userId)

        let offered = "offered-with-capture-disabled-\(Self.stamp)"
        let attempt = try await phone.repo.offerText(offered)
        guard case .settled(_, let outcome) = attempt, case .queued = outcome else {
            return XCTFail("the Offer must be taken with capture disabled: \(attempt)")
        }

        let seen = await other.awaitNewestOnRelay(offered)
        XCTAssertTrue(seen, "an Offer made with capture disabled must still reach the relay")
    }

    /// A phone of this test's own, paired into the run's inviting device and
    /// left resting.
    private func openPhone(_ database: String, label: String) async throws -> String {
        let inviter = try await Inviter.shared()
        phone = try PhoneUnderTest.open(databaseName: database)
        return try await phone.pair(with: inviter, label: label)
    }

    /// How long the phone is left closed while the other device offers.
    ///
    /// The Entry is on the relay before the wait starts, so this is only the
    /// window in which a session that had not come down would have brought it
    /// in.
    private static let settle: UInt64 = 3_000_000_000

    /// Distinct content per test, so one run's Entries cannot be mistaken for
    /// another's on a relay this suite shares with itself.
    private static var stamp: Int {
        Int(Date().timeIntervalSince1970 * 1000)
    }
}
