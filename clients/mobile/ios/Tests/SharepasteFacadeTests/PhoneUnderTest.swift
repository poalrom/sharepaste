import Foundation
import SharepasteCore
import SharepasteKit
import XCTest

/// The phone under test: the app's own chokepoint over a database this test
/// owns.
///
/// Assembled from the production ``SharepasteRepository`` over a production
/// core, because a criterion proven against a hand-rolled facade is a criterion
/// not proven at all. The two platform objects it would otherwise reach for are
/// substituted, and for one reason: this bundle has no host application, so
/// `UIPasteboard` never answers and Keychain Services refuses with
/// `errSecMissingEntitlement`. The diagnosis is at the top of `Suite.swift`.
///
/// Android's equivalent also carries the state holder and the real screens. This
/// one stops at the repository, and the two lifecycle edges below are the calls
/// the state holder makes rather than the state holder itself: spec row 10 buys
/// the facade half of that suite and no UI, so a `SharepasteViewModel` here
/// would be a dependency on the app module for the sake of reading a flag off a
/// screen nothing is looking at.
final class PhoneUnderTest {

    let repo: SharepasteRepository

    /// This phone's pasteboard, which every Offer reads and every Recall writes.
    ///
    /// Handed to the repository rather than left to default, because
    /// `UIPasteboard` is not a pasteboard a host-less test bundle has — the
    /// reasoning is on ``TestPasteboard``. It is the same object the core was
    /// given, so what a Recall wrote is readable here without asking the core
    /// anything.
    let pasteboard: TestPasteboard

    /// This phone's keychain: the two secrets a Pairing is made of, as the core
    /// filed them.
    ///
    /// Injected through the seam ticket 02 already put on the initialiser, for
    /// the reason on ``TestKeychain`` — Keychain Services answers
    /// `errSecMissingEntitlement` in this process. It is the same object the
    /// core was given, so `<user>:key` and `<user>:token` are readable here
    /// exactly as the core wrote them.
    let keychain: TestKeychain

    /// Every Pairing this phone joined here, in the order it joined them.
    private(set) var pairedUserIds: [String] = []

    /// The Pairing this phone joined most recently, once it has one.
    var userId: String? { pairedUserIds.last }

    private init(
        repo: SharepasteRepository,
        pasteboard: TestPasteboard,
        keychain: TestKeychain
    ) {
        self.repo = repo
        self.pasteboard = pasteboard
        self.keychain = keychain
    }

    /// A phone with an empty database, in the app's own container.
    ///
    /// `requireHttps: false`, because the test relay is plain HTTP and there is
    /// no publicly trusted certificate to put in front of it on a runner. That
    /// concession is safe only while `TransportPolicyTest` proves the app itself
    /// does not make it — leave that test alone.
    ///
    /// `fresh: false` keeps whatever is already in the file, which is what a
    /// test that had to write a Settings row before the phone existed needs —
    /// two facades open on one SQLite file at once is not something to arrange
    /// on purpose.
    static func open(
        databaseName: String,
        fresh: Bool = true
    ) throws -> PhoneUnderTest {
        let directory = try fresh
            ? freshDatabase(named: databaseName)
            : existingDatabase(named: databaseName)
        let pasteboard = TestPasteboard()
        let keychain = TestKeychain()
        return PhoneUnderTest(
            repo: SharepasteRepository(
                directory: directory,
                requireHttps: false,
                databaseName: databaseName,
                keychain: keychain,
                clipboard: pasteboard
            ),
            pasteboard: pasteboard,
            keychain: keychain
        )
    }

    /// Pair with a short code the other device minted, exactly as a scan does.
    @discardableResult
    func pair(with inviter: Inviter, label: String) async throws -> String {
        let code = try await inviter.freshCompactCode()
        let paired = try await repo.pairWithCode(code: code, deviceLabel: label)
        try await activate(paired.userId)
        return paired.userId
    }

    // There is deliberately no `pairWithInvite` here. `SharepasteRepository`
    // exposes none: an invite is redeemed on a desktop, and this phone's only
    // way into a Pairing is a short code (spec row 11). A test that needs the
    // Relay to be somewhere it can take away therefore does not reach for one —
    // it pairs by code with an ``Inviter`` that was itself claimed through the
    // proxy, because a short code *carries* the inviting device's `server_url`
    // inside its payload. See `PendingOnANonActivePairingTest`.

    /// Make it the Active Pairing, then put the phone back down.
    ///
    /// Pairing brings a session up on its own — `pairWithCode` ends in
    /// `activate_and_sync` and `setActivePairing` activates a session — which is
    /// right for a real phone and wrong for a test that has not reached its
    /// first foreground edge yet. Without this, an Entry a test puts on the
    /// Relay "while the phone is closed" arrives over a live stream instead, and
    /// the resume it was meant to prove proves nothing. Every test here drives
    /// the two edges explicitly, so this is the state they all start from.
    private func activate(_ userId: String) async throws {
        pairedUserIds.append(userId)
        try await repo.setActivePairing(userId: userId)
        try await repo.stopAllSessions()
    }

    /// Make one of the Pairings already held the Active one, with no session
    /// left running.
    func makeActive(_ userId: String) async throws {
        try await repo.setActivePairing(userId: userId)
        try await repo.stopAllSessions()
    }

    // -- the two lifecycle edges, as the state holder drives them --------------

    /// What `onEnterForeground` does to the facade: pick the Active Pairing back
    /// up, then bring its session up.
    @discardableResult
    func enterForeground() async throws -> String? {
        guard let userId = try await repo.resumeActivePairing() else { return nil }
        try await repo.startSession(userId: userId)
        return userId
    }

    /// What `onLeaveForeground` does: every session down, and nothing else.
    /// Sync is foreground-only and this is the whole of it (ADR 0007).
    func leaveForeground() async throws {
        try await repo.stopAllSessions()
    }

    // -- waiting ---------------------------------------------------------------

    /// Wait until the core reports a live connection for `userId`.
    ///
    /// Polled rather than read once: a session that reached `online` can be back
    /// in `connecting` a moment later after a reconnect, and the criterion is
    /// that it got there at all.
    func awaitInContact(
        _ userId: String,
        file: StaticString = #filePath,
        line: UInt = #line
    ) async throws {
        var last: ConnectionState = .disconnected
        await awaitCondition("a session for \(userId) must reach the relay", file: file, line: line) {
            last = (try? await self.repo.connectionState(userId: userId)) ?? .disconnected
            return last == .online
        }
    }

    /// Wait for an Entry with this Preview in one Pairing's cache, and hand it
    /// back.
    ///
    /// The cache is asked directly rather than through anything that renders it:
    /// the Pairing being written to is not always the one a screen would be
    /// showing, and on this target there is no screen at all.
    @discardableResult
    func awaitEntry(
        _ what: String,
        userId: String,
        previewing preview: String,
        file: StaticString = #filePath,
        line: UInt = #line
    ) async throws -> Entry {
        try await awaitValue(what, file: file, line: line) {
            try? await self.repo.listHistory(userId: userId).first { $0.preview == preview }
        }
    }

    /// Release the facade, and by default every Pairing this phone made.
    ///
    /// Forgetting is on by default so a run does not leave Users and Entries on
    /// the relay behind it. Sessions come down first: the facade is destroyed
    /// when the last reference to it goes, and destroying it with a session's
    /// own task mid-call is how a suite acquires a failure that lands in
    /// whichever test happens to be running.
    func close(forgetPairings: Bool = true) async {
        try? await repo.stopAllSessions()
        if forgetPairings {
            for userId in pairedUserIds {
                try? await repo.forgetPairing(userId: userId)
            }
        }
    }
}
