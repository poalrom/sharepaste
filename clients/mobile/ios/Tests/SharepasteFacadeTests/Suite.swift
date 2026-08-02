import Foundation
import SharepasteCore
import SharepasteKit
import XCTest

// The whole of this client's automated defence, and a deliberately short list.
//
// Spec row 10: the facade classes only, on the CI simulator. What is here is a
// port of the protocol-level half of Android's instrumented suite — the classes
// that exercise the facade rather than the UI — against a real relay started
// natively on the runner. Ported by what each class *defends*, not by its shape:
// several of them are one Compose test and one facade test wearing the same
// name, and only the second half crosses.
//
// ─────────────────────────────────────────────────────────────────────────────
// THE HOST-LESS BUNDLE, which is the one fact this suite is shaped around.
// ─────────────────────────────────────────────────────────────────────────────
//
// A SwiftPM test target has no host application. `xcodebuild` therefore runs
// this bundle in the Xcode unit-test runner: a process with no app, no bundle
// identifier of its own, no entitlements, and nothing on screen. **It has no
// application identity**, and every platform service that authorises by
// application identity refuses it or waits forever for a user who is not there.
//
// Three of those were found the hard way, one per CI run, before the pattern was
// named:
//
//  1. **The pasteboard.** Since iOS 16 a read raises the system paste-permission
//     prompt. With no app to present it and nobody to tap it the call does not
//     fail — it never returns. One run sat on `UIPasteboard.general` for
//     thirty-six minutes. See ``TestPasteboard``.
//  2. **The keychain.** `SecItemAdd` and `SecItemDelete` answer `OSStatus
//     -34018`, `errSecMissingEntitlement`: an item needs a keychain access
//     group, an access group comes from the application-identifier entitlement,
//     and this process has none. See ``TestKeychain``.
//  3. **The environment.** `TEST_RUNNER_`-prefixed build settings are the
//     documented way to put a value into a test process's environment, and they
//     are delivered to the *host app* that a host-less bundle does not have. The
//     tokens simply never arrived. See `Resources/harness.json` and ``Suite``.
//
// The rule that falls out of it, and the one worth carrying to the next
// platform service somebody reaches for here: **anything that authorises by
// application identity has to be injected, not called.** Every one of these is
// reachable through a foreign trait the core already declares — Clipboard,
// Keychain — which is not a coincidence: those traits exist because the shell
// decides what a pasteboard and a keychain *are*, and a test process is a shell
// for which the answer is "not the platform's".
//
// What that costs is written down rather than absorbed: the platform
// implementations — `IosPasteboard`'s refusal to coerce an image, and the whole
// of `IosKeychain` — are exercised by nothing automated, and are device
// acceptance in tickets 04 to 06. Android's own suite carries the same shape of
// hole from the other side, and says so in its `FacadeSurfaceTest`: the
// clipboard is readable only by the focused app or the default IME, and an
// instrumentation run is neither.
//
// **Deliberately excluded, so that the omissions read as decisions.**
//
//  * Every Compose UI test — `BackNavigationTest`, `TryAgainTest`,
//    `HistoryListTest`, `PairingsScreenTest`, `QrPairingTest`, and the rest.
//    Porting them means rebuilding `TestRelay`, `PhoneUnderTest` and `Evidence`
//    in Swift for the flakiest runner available, on a client with one user. The
//    retry reset, the back stack and the row wording are proved by hand on the
//    device instead, which is what tickets 03 to 06 carry as acceptance.
//  * `StandingActionsNotificationTest`, `ShareTargetTest` and
//    `NotificationsDeniedTest`. Android-only surfaces with nothing here to test:
//    this phone posts no notification (ADR 0007), has no Share Extension and can
//    have none on a free team (spec row 15), and therefore has no permission to
//    be refused.
//  * The **Receipt** half of the classes that are on this list.
//    `RoundTripTest` above all now asserts a Receipt and its Preview where it
//    used to assert a `Notice`; a Receipt is a Toast, which is UI. The facade
//    half beneath it — that a Recall served over the network hands back the
//    Entry only the relay had — is what these tests are on this list for, and
//    what is asserted here.
//  * The **Viewed Pairing**, in `TwoPairingsTest`. It is a state-holder concept
//    with no core state behind it: nothing writes it, and the facade cannot be
//    asked what it is. The half that is testable here is that what the core
//    syncs and captures to does not move with it.
//
// Nothing in here is a snapshot test and nothing in here drives a screen.

/// Values every test in the suite shares, and the two things the runner has to
/// be told.
///
/// **Both arrive as a file in the bundle, not as an environment variable.** The
/// documented route into a test process's environment is a `TEST_RUNNER_`-
/// prefixed build setting, and it does not arrive in a bundle with no host
/// application. That was measured rather than guessed: two CI runs where the
/// job's own step held all twenty-four minted tokens and the suite read an
/// empty list. It was invisible for a whole run because the relay address has a
/// correct default and the invite list has none, so only the invites ever
/// reported it.
///
/// `Resources/harness.json` is written by the CI job before the bundle is
/// built, and the copy in the tree carries the shape and an empty invite list.
/// The environment is still read first, because that is the one route that
/// works when somebody runs this from a shell with tokens of their own.
enum Suite {

    /// How long anything that has to cross the network is given.
    ///
    /// Generous, and the same 60 seconds Android's `PhoneUnderTest` uses, for
    /// the same reason: a suite that talks to a real relay must not fail a test
    /// about the phone because the relay was slow. Every wait that uses it
    /// reports what it was waiting for when it expires.
    static let timeout: TimeInterval = 60

    /// The wall clock XCTest itself gives one test before it kills it.
    ///
    /// Every wait in this suite is bounded, and that was not enough: a blocking
    /// call inside the FFI boundary is not interruptible by anything on this
    /// side, so one `UIPasteboard` read with nobody to answer its permission
    /// prompt held a job for thirty-six minutes until a human cancelled it. A
    /// test that cannot fail on its own has to be failed from outside.
    ///
    /// `xcodebuild` supplies that from outside with `-test-timeouts-enabled`,
    /// and this is the value the job passes as the default allowance. It is
    /// stated here as well because ``SlowTestCase`` raises it, and a number a
    /// test overrides should be readable beside the override.
    static let executionTimeAllowance: TimeInterval = 120

    /// The relay these tests pair against.
    ///
    /// Plain HTTP, which is why every facade opened here passes
    /// `requireHttps: false` and says so at the call. `TransportPolicyTest` is
    /// what keeps that a concession rather than the configuration — leave it
    /// alone.
    ///
    /// `127.0.0.1` and not an emulator alias: the simulator shares the host's
    /// network stack, so the host's loopback is the loopback. This is the one
    /// place the iOS arrangement is simpler than Android's rather than harder.
    static let relayURL: String = {
        ProcessInfo.processInfo.environment["SHAREPASTE_RELAY_URL"] ?? harness.relayURL
    }()

    /// The invite tokens the run was given, handed out one at a time.
    ///
    /// An invite is **single use** — the relay answers a second claim with `409
    /// Conflict` — so every claim spends one. The job mints far more than a run
    /// needs; see the reasoning at that step, and at
    /// ``InviterFailure/outOfInvites(claiming:taken:)`` for why running out must
    /// not be a crash.
    static func nextInvite(claiming label: String) throws -> String {
        try invites.next(claiming: label)
    }

    private static let invites = Invites(
        ProcessInfo.processInfo.environment["SHAREPASTE_INVITES"].map {
            $0.split(separator: ",").map(String.init)
        } ?? harness.invites
    )

    /// What the job baked into the bundle.
    ///
    /// A missing or unreadable file is not survivable and does not pretend to
    /// be: every claiming test would fail one after another with a message
    /// about invites, and the cause would be a resource that never got copied.
    /// This says so once, at the top, in the terms that would fix it.
    private static let harness: Harness = {
        guard let url = Bundle.module.url(
            forResource: "harness",
            withExtension: "json",
            subdirectory: "Resources"
        ) else {
            fatalError(
                """
                Resources/harness.json is not in the test bundle. It is declared in \
                Package.swift as `.copy("Resources")`; if that declaration is gone, so is \
                every invite token and the relay address with it.
                """
            )
        }
        do {
            return try JSONDecoder().decode(Harness.self, from: Data(contentsOf: url))
        } catch {
            fatalError("Resources/harness.json is in the bundle and does not parse: \(error)")
        }
    }()

    /// A plain TCP connect before the protocol is asked to do anything.
    ///
    /// Without it, a relay that is simply not running looks exactly like a
    /// broken FFI boundary: the failure surfaces from deep inside
    /// `pairWithInvite` as a transport error and says nothing about which of the
    /// two it was.
    static func assertRelayIsReachable(
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        let (host, port) = Tcp.split(url: relayURL)
        XCTAssertTrue(
            Tcp.canConnect(host: host, port: port, timeout: 5),
            """
            the relay at \(relayURL) is not reachable from this simulator. Start it on the \
            host with `npm --prefix server start`, or set SHAREPASTE_RELAY_URL.
            """,
            file: file,
            line: line
        )
    }
}

/// `Resources/harness.json`, as the job writes it.
private struct Harness: Decodable {
    let relayURL: String
    let invites: [String]
}

/// The run's invite tokens.
///
/// A class with a lock rather than a `static var`: the suite is one process,
/// XCTest may run classes in any order, and two tests taking the same token
/// would fail as a `409` from the relay several seconds later and look like a
/// pairing bug.
private final class Invites: @unchecked Sendable {

    private let lock = NSLock()
    private var remaining: [String]
    private var taken = 0

    init(_ tokens: [String]) {
        remaining = tokens
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
    }

    /// The next unused token, or an error naming the claim that ran out.
    ///
    /// **Thrown, never `fatalError`.** A crash takes the whole test process
    /// with it, and `xcodebuild` responds to a crashed bundle by relaunching it
    /// and re-running what has not finished — which claims the *same* tokens
    /// again from a pool that is already empty. One exhausted run becomes
    /// several, and the report names a signal rather than a cause. A thrown
    /// error fails one test with a sentence.
    func next(claiming label: String) throws -> String {
        lock.lock()
        defer { lock.unlock() }
        guard !remaining.isEmpty else {
            throw InviterFailure.outOfInvites(claiming: label, taken: taken)
        }
        taken += 1
        return remaining.removeFirst()
    }
}

/// The one piece of socket work the suite does for itself.
///
/// `URLSession` is not usable for any of it: App Transport Security refuses
/// cleartext, the test runner's `Info.plist` is generated by `xcodebuild` and
/// carries no exception, and the relay these tests talk to is `http://`. None of
/// that reaches the core — its Rust HTTP client is on raw sockets and never
/// consults ATS, which is exactly the asymmetry `TransportPolicyTest` exists to
/// keep honest — but it does mean a reachability probe here has to be a socket.
enum Tcp {

    /// `http://127.0.0.1:8443` as its host and port.
    static func split(url: String) -> (host: String, port: UInt16) {
        let authority = url.components(separatedBy: "://").last ?? url
        let hostPort = authority.components(separatedBy: "/").first ?? authority
        let parts = hostPort.components(separatedBy: ":")
        let host = parts.first ?? "127.0.0.1"
        let port = parts.count > 1 ? UInt16(parts[1]) ?? 80 : 80
        return (host, port)
    }

    /// Whether something is listening, within `timeout`.
    ///
    /// A blocking connect on a socket of its own, closed either way. The
    /// timeout is set with `SO_SNDTIMEO` because a refused connection on
    /// loopback answers instantly and an unroutable address would otherwise
    /// hang for the platform's own two minutes.
    static func canConnect(host: String, port: UInt16, timeout: TimeInterval) -> Bool {
        let fd = socket(AF_INET, SOCK_STREAM, 0)
        guard fd >= 0 else { return false }
        defer { Darwin.close(fd) }

        var deadline = timeval(
            tv_sec: Int(timeout),
            tv_usec: Int32((timeout - Double(Int(timeout))) * 1_000_000)
        )
        setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &deadline, socklen_t(MemoryLayout<timeval>.size))

        var address = sockaddr_in()
        address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
        address.sin_family = sa_family_t(AF_INET)
        address.sin_port = port.bigEndian
        address.sin_addr.s_addr = inet_addr(host)

        let connected = withUnsafePointer(to: &address) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddress in
                Darwin.connect(fd, sockaddress, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        return connected == 0
    }
}

/// Poll until `condition` holds, or fail saying what never happened.
///
/// Almost every assertion in this suite has to be preceded by one of these:
/// what is being waited for arrives from the SSE reader, the uploader, or
/// another device entirely. The message matters — a bare timeout in a suite that
/// talks to a real relay is indistinguishable between "the phone is wrong" and
/// "the relay was slow".
func awaitCondition(
    _ what: String,
    timeout: TimeInterval = Suite.timeout,
    interval: TimeInterval = 0.2,
    file: StaticString = #filePath,
    line: UInt = #line,
    _ condition: () async throws -> Bool
) async rethrows {
    let deadline = Date().addingTimeInterval(timeout)
    while Date() < deadline {
        if try await condition() { return }
        try? await Task.sleep(nanoseconds: UInt64(interval * 1_000_000_000))
    }
    XCTFail("\(what): never happened in \(Int(timeout))s", file: file, line: line)
}

/// The same, for a value rather than a condition, with the value handed back.
func awaitValue<T>(
    _ what: String,
    timeout: TimeInterval = Suite.timeout,
    interval: TimeInterval = 0.2,
    file: StaticString = #filePath,
    line: UInt = #line,
    _ produce: () async throws -> T?
) async throws -> T {
    let deadline = Date().addingTimeInterval(timeout)
    while Date() < deadline {
        if let value = try await produce() { return value }
        try? await Task.sleep(nanoseconds: UInt64(interval * 1_000_000_000))
    }
    XCTFail("\(what): never arrived in \(Int(timeout))s", file: file, line: line)
    throw SuiteFailure.timedOut(what)
}

enum SuiteFailure: Error {
    case timedOut(String)
}

/// A directory this test's database has to itself, inside the app container the
/// shipped app uses.
///
/// `AppContainer.databaseDirectory()` and not a temporary directory, because
/// that call is itself part of what these tests cover: it is where the backup
/// exclusion and the file-protection class are applied, and a suite that wrote
/// somewhere else would prove the facade over a directory the app never uses.
///
/// **A directory per test, removed whole.** The first version deleted three
/// named files — the database and SQLite's `-wal` and `-shm` — and the two
/// sidecars exist only while a connection is open in WAL mode, so on every run
/// where the facade had closed cleanly the removal was of something that was not
/// there. Removing a directory that may not exist is one `try?` instead of
/// three, and it takes with it whatever SQLite invented that this list did not
/// know about. Set-up must not be able to fail a test that would have passed.
///
/// It also ends a hazard that had nothing to do with the sidecars: every test
/// was writing into one directory, so a name collision between two classes was
/// a shared database and a test order nobody had chosen.
func freshDatabase(named name: String) throws -> URL {
    let directory = try AppContainer.databaseDirectory()
        .appendingPathComponent(name, isDirectory: true)
    try? FileManager.default.removeItem(at: directory)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    return directory
}

/// The same directory, without emptying it.
///
/// For the one test that has to write a Settings row through a facade of its
/// own before the phone opens over the same file.
func existingDatabase(named name: String) throws -> URL {
    let directory = try AppContainer.databaseDirectory()
        .appendingPathComponent(name, isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    return directory
}

/// A test that is allowed to take longer than the suite's default allowance,
/// and says how much longer at the point it says why.
///
/// Only one test needs it: `ExpiredCodeTest` waits out a real 120-second
/// pairing slot, which is the whole of what it proves. Raising the *default*
/// for everything to cover that one wait would be raising the ceiling on every
/// hang in the suite by two minutes, which is the opposite of what the
/// allowance is for.
///
/// `executionTimeAllowance` is only honoured when `xcodebuild` is run with
/// `-test-timeouts-enabled YES`, and it is clamped to
/// `-maximum-test-execution-time-allowance`; both are set at the job.
class SlowTestCase: XCTestCase {

    /// How long this class's tests may take. Subclasses raise it before
    /// `super.setUp()` returns.
    class var allowance: TimeInterval { Suite.executionTimeAllowance }

    override func setUp() {
        super.setUp()
        executionTimeAllowance = Self.allowance
    }
}
