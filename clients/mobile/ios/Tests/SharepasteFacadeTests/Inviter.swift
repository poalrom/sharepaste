import Foundation
import SharepasteCore

/// The device on the other end of the pairing.
///
/// A short code is minted by a device that is *already* paired, so proving this
/// phone can claim one needs a second device to exist. This is that device: it
/// claims an invite, mints codes, and can offer an Entry while the phone under
/// test is "backgrounded".
///
/// In memory, because none of it needs to survive the run — but a real facade
/// against the real relay, because a stub inviter would prove nothing about the
/// bytes in a code.
///
/// **Its own serial queue**, for the reason ``SharepasteRepository`` has one:
/// every call across the boundary blocks until the operation completes, and
/// parking a cooperative thread on a relay round trip is how Swift's concurrency
/// runtime is starved. A test awaiting this device and the phone at the same
/// time would otherwise be one thread short of making progress on either.
final class Inviter: @unchecked Sendable {

    let userId: String

    private let core: Sharepaste
    private let queue = DispatchQueue(label: "com.sharepaste.ios.tests.inviter")

    private init(core: Sharepaste, userId: String) {
        self.core = core
        self.userId = userId
    }

    /// A fresh short code, in the **compact** form the desktop's QR carries:
    /// whitespace and dashes stripped, upper case.
    ///
    /// The core hands back the grouped-for-reading form, which is what a person
    /// types. The desktop's pairing pane strips it before encoding the square,
    /// so that is the string a scan actually produces — and the string this
    /// returns.
    ///
    /// Deliberately never logged. For the next two minutes it *is* the pairing
    /// secret.
    func freshCompactCode() async throws -> String {
        try await run { core in
            try core.pairStart(userId: self.userId).code
                .filter { !$0.isWhitespace && $0 != "-" }
                .uppercased()
        }
    }

    /// Put an Entry on the relay, as the other device would.
    func offer(_ text: String) async throws {
        try await run { core in
            try core.startSession(userId: self.userId)
            _ = try core.offer(userId: self.userId, text: text)
        }
    }

    /// The newest Entry this User has on the relay, decrypted by *this* device.
    ///
    /// `recallLatest` is what reads it, because `recallLatest` always performs
    /// the round trip: the answer is what the relay holds now, not what this
    /// facade happened to have cached. That is what makes it usable as the
    /// assertion for "the phone's Offer reached the relay and another device can
    /// read it" — one call covers both halves, and a stale cache cannot fake it.
    func newestOnRelay() async throws -> String {
        try await run { core in try core.recallLatest(userId: self.userId).text }
    }

    /// Offer, and do not return until the relay actually has it.
    ///
    /// The uploader is asynchronous, so an Offer that has only been *queued*
    /// proves nothing about what another device can see. Every test that hands
    /// this device something for the phone to find needs the Entry to be on the
    /// relay before the phone is allowed to look, or the phone finding nothing
    /// would look like a broken backfill.
    ///
    /// Hands back the relay's id for it, which is the only handle on an Entry
    /// whose Preview a test cannot predict.
    @discardableResult
    func offerAndWaitForUpload(_ text: String) async throws -> Int64 {
        try await offer(text)
        let deadline = Date().addingTimeInterval(Inviter.uploadTimeout)
        while Date() < deadline {
            // Only `entryId` is read out of it. `Recalled` carries the
            // plaintext, and the core deliberately gives its Rust equivalent no
            // `Debug`, so the rule that it is never printed has to be kept by
            // hand on this side.
            let newest = try? await run { core in try core.recallLatest(userId: self.userId) }
            if let newest, newest.text == text { return newest.entryId }
            try? await Task.sleep(nanoseconds: 200_000_000)
        }
        throw InviterFailure.neverUploaded(seconds: Int(Inviter.uploadTimeout))
    }

    /// Wait until the newest Entry on the relay is `expected`.
    ///
    /// The read-back half of "the phone's Offer reached the relay": polled
    /// rather than read once, because an Offer is queued the moment the facade
    /// answers and reaches the relay only after the uploader has sent it.
    func awaitNewestOnRelay(_ expected: String) async -> Bool {
        let deadline = Date().addingTimeInterval(Suite.timeout)
        while Date() < deadline {
            if let newest = try? await newestOnRelay(), newest == expected { return true }
            try? await Task.sleep(nanoseconds: 250_000_000)
        }
        return false
    }

    private func run<T: Sendable>(_ call: @escaping @Sendable (Sharepaste) throws -> T) async throws -> T {
        let core = self.core
        return try await withCheckedThrowingContinuation { continuation in
            queue.async {
                continuation.resume(with: Result { try call(core) })
            }
        }
    }

    /// How long the other device's uploader is given.
    ///
    /// Generous: it covers a session coming up, an encrypt and a POST over
    /// loopback, and a slow one here would fail a test about the *phone* for a
    /// reason that has nothing to do with the phone.
    private static let uploadTimeout: TimeInterval = 30

    /// The run's one inviting device, claimed on first use.
    ///
    /// Never released: it dies with the test process, and keeping its session
    /// live between tests is what lets one test offer an Entry that another
    /// test's phone backfills.
    static func shared() async throws -> Inviter {
        try await Pool.shared.first(label: "inviting side of the test")
    }

    /// A **second** User, for the tests that need this phone to hold two
    /// Pairings at once.
    ///
    /// A Pairing binds this machine to one User on one Relay, so two Pairings
    /// means two Users: pairing twice against ``shared()`` would give one
    /// Pairing two Devices, which is a different thing entirely and proves
    /// nothing about an Active Pairing.
    ///
    /// One per run, for the same reason the first is. A User can mint any number
    /// of short codes, so every test that wants a second Pairing can claim one
    /// from this device for free — where a second *inviter* per test would cost
    /// a single-use invite token each time.
    static func second() async throws -> Inviter {
        try await Pool.shared.second(label: "the second User in the test")
    }

    /// An inviting device on a Relay address of the caller's choosing, claimed
    /// now and memoised nowhere.
    ///
    /// The address is a parameter for one test only: a short code carries the
    /// inviting device's `server_url` inside its payload, so the only way to
    /// give the phone a Pairing whose Relay can be taken away is to claim the
    /// *inviter* through the proxy and let the phone pair by code with it. It
    /// costs a single-use invite of its own.
    static func against(relay: String, label: String) async throws -> Inviter {
        try await claim(label: label, relay: relay)
    }

    fileprivate static func claim(
        label: String,
        relay: String = Suite.relayURL
    ) async throws -> Inviter {
        let queue = DispatchQueue(label: "com.sharepaste.ios.tests.inviter.claim")
        let invite = try Suite.nextInvite(claiming: label)

        // A plain TCP connect before the protocol is asked to do anything.
        // Without it a relay that is simply not running looks exactly like a
        // broken FFI boundary: the failure surfaces from inside
        // `pairWithInvite` as a transport error and says nothing about which of
        // the two it was.
        let (host, port) = Tcp.split(url: relay)
        guard Tcp.canConnect(host: host, port: port, timeout: 5) else {
            throw InviterFailure.relayUnreachable(relay)
        }
        return try await withCheckedThrowingContinuation { continuation in
            queue.async {
                continuation.resume(with: Result {
                    let core = try Sharepaste.openInMemory(
                        keychain: TestKeychain(),
                        clipboard: NoClipboard(),
                        events: SilentSink(),
                        // The test relay is plain HTTP. `TransportPolicyTest`
                        // is what proves the app itself passes `true`.
                        requireHttps: false
                    )
                    let paired = try core.pairWithInvite(
                        serverUrl: relay,
                        token: invite,
                        deviceLabel: label
                    )
                    try core.setActivePairing(userId: paired.userId)
                    return Inviter(core: core, userId: paired.userId)
                })
            }
        }
    }
}

enum InviterFailure: Error, CustomStringConvertible {
    case neverUploaded(seconds: Int)
    case relayUnreachable(String)

    /// The run asked for more single-use invites than the job minted.
    ///
    /// It names the claim that ran out, because that is the one thing the CI log
    /// cannot work out for itself: the pool is process-wide and the order XCTest
    /// runs classes in is not fixed, so "an invite was wanted and there was
    /// none" without a name sends the reader to the wrong test.
    case outOfInvites(claiming: String, taken: Int)

    var description: String {
        switch self {
        case .neverUploaded(let seconds):
            "the other device's Entry never reached the relay in \(seconds)s"
        case .relayUnreachable(let relay):
            "the relay at \(relay) is not reachable from this simulator"
        case .outOfInvites(let label, let taken):
            """
            out of invite tokens while claiming "\(label)"; \(taken) had already been \
            taken. Each is single-use. The CI job mints them with \
            `node server/dist/index.js user create <name>` and passes them comma-separated \
            as TEST_RUNNER_SHAREPASTE_INVITES — raise the count there rather than sharing \
            one between two claims.
            """
        }
    }
}

/// The two inviting devices, claimed at most once each.
///
/// The claim is memoised as a `Task` rather than as an `Inviter?`, and that is
/// not belt and braces: an actor suspends at every `await`, so two tests asking
/// for the shared inviter at the same moment would both see `nil`, both claim,
/// and the second would burn an invite token the run does not have. Holding the
/// in-flight task means the second caller awaits the first one's claim.
private actor Pool {

    static let shared = Pool()

    private var firstClaim: Task<Inviter, Error>?
    private var secondClaim: Task<Inviter, Error>?

    func first(label: String) async throws -> Inviter {
        if let firstClaim { return try await firstClaim.value }
        let claim = Task { try await Inviter.claim(label: label) }
        firstClaim = claim
        return try await claim.value
    }

    func second(label: String) async throws -> Inviter {
        if let secondClaim { return try await secondClaim.value }
        let claim = Task { try await Inviter.claim(label: label) }
        secondClaim = claim
        return try await claim.value
    }
}
