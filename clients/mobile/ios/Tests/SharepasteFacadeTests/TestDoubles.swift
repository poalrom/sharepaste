import Foundation
import SharepasteCore
import SharepasteKit

/// Swift implementations of the three platform traits.
///
/// Two of them are substituted for the same reason and it is not a preference:
/// this bundle has no host application, so it has no application identity, so
/// every platform service that authorises by one refuses it. The full diagnosis
/// — all three instances of it, and the rule that falls out — is at the top of
/// `Suite.swift`. What is faked beyond those two is only ever the other device.

/// The pasteboard, as a process that has one would see it.
///
/// **`UIPasteboard` is unusable from this bundle.** Since iOS 16 a read raises
/// the system paste-permission prompt, and in the unit-test runner the prompt
/// has nobody to present it and nobody to tap it — so the call does not fail, it
/// never returns. One run of this suite sat on `UIPasteboard.general` for
/// thirty-six minutes before it was cancelled. See the note at the top of
/// `Suite.swift`; this is instance one of three.
///
/// Android's suite records the same rule from the other side, and the note is
/// already in its `FacadeSurfaceTest`: the clipboard is readable only by the
/// focused app or the default IME, and an instrumentation run is neither. That
/// suite therefore asserts on what the core *wrote*, and reads nothing back.
/// Porting its assertion without porting that reasoning is precisely what hung
/// this job, so the reasoning is written down here and the assertion follows
/// from it.
///
/// What is lost is one claim, and it is named rather than papered over: that
/// ``IosPasteboard`` refuses to coerce a copied image into a string. That lives
/// in `hasStrings` and can only be exercised where there is a real pasteboard —
/// on the device, by hand, as tickets 04 to 06 accept it.
final class TestPasteboard: Clipboard, @unchecked Sendable {

    private let lock = NSLock()
    private var contents: String?
    private var writes: [String] = []

    init(holding contents: String? = nil) {
        self.contents = contents
    }

    /// Everything written through this pasteboard, oldest first.
    ///
    /// A list rather than the last value: a test that asserted only on the
    /// newest could not tell "the Recall wrote nothing" from "the Recall was
    /// overtaken".
    var written: [String] {
        lock.lock()
        defer { lock.unlock() }
        return writes
    }

    /// Put something on the pasteboard from outside, as a person copying would.
    ///
    /// `nil` is a pasteboard holding something that is not text at all — a
    /// screenshot — which is what ``IosPasteboard/readText()`` answers for one
    /// and what the core's capture filter calls `nonText`.
    func put(_ text: String?) {
        lock.lock()
        defer { lock.unlock() }
        contents = text
    }

    func readText() throws -> String? {
        lock.lock()
        defer { lock.unlock() }
        return contents
    }

    func writeText(text: String) throws {
        lock.lock()
        defer { lock.unlock() }
        writes.append(text)
        contents = text
    }
}

/// A clipboard for a facade that is standing in for another device.
final class NoClipboard: Clipboard, @unchecked Sendable {
    func readText() throws -> String? { nil }
    func writeText(text: String) throws {}
}

/// An event sink for a facade whose events no test reads.
final class SilentSink: EventSink, @unchecked Sendable {
    func emit(event: CoreEvent) {}
}

/// The keychain, as a process that has one would see it.
///
/// **Keychain Services is unusable from this bundle**, and it says so with a
/// number: `SecItemAdd` and `SecItemDelete` answer `OSStatus -34018`,
/// `errSecMissingEntitlement`. A keychain item belongs to an access group, an
/// access group comes from the application-identifier entitlement, and the
/// unit-test runner has no application identity to have one. Fourteen tests
/// failed on it in one run, every claim of a Pairing among them, because the
/// core writes `<user_id>:key` before it writes the Pairing row. See the note at
/// the top of `Suite.swift`; this is instance two of three, and it is the same
/// fact as the pasteboard wearing a different error.
///
/// Substituting it takes nothing away from what these tests defend. They are
/// facade tests: what is under test is that the core reaches a Swift keychain
/// at all, files the two secrets a Pairing is made of under the accounts it
/// claims to, and erases both when a Pairing is forgotten — and every one of
/// those is a statement about the **trait**, which is why the trait is foreign
/// in the first place. `TwoPairingsTest` still reads `<user>:key` and
/// `<user>:token` out of this object and still fails if a forget leaves either
/// behind.
///
/// What is lost is ``IosKeychain`` itself, and it is lost entirely: its
/// `AfterFirstUnlockThisDeviceOnly` accessibility, the update-or-insert arm of
/// `put`, and the `OSStatus` wrapping have no automated coverage on this
/// platform at all. That is recorded in ticket 08's Evidence and on ticket 04's
/// human checklist rather than left to be discovered — the accessibility choice
/// in particular is only provable the way it matters by running a Standing
/// Action from a locked device.
final class TestKeychain: Keychain, @unchecked Sendable {

    private let lock = NSLock()
    private var entries: [String: String] = [:]

    /// Every account currently held, for a test that wants to say what is gone.
    var accounts: [String] {
        lock.lock()
        defer { lock.unlock() }
        return entries.keys.sorted()
    }

    func put(account: String, secret: String) throws {
        lock.lock()
        defer { lock.unlock() }
        entries[account] = secret
    }

    func get(account: String) throws -> String? {
        lock.lock()
        defer { lock.unlock() }
        return entries[account]
    }

    func delete(account: String) throws {
        lock.lock()
        defer { lock.unlock() }
        entries.removeValue(forKey: account)
    }
}

/// An event sink that records what arrived and **which thread raised it**.
///
/// The thread is half the point. The events that matter are raised by the
/// session loop's own tokio tasks — the SSE reader, the uploader, the contact
/// stamp — on threads belonging to the core's private runtime and to no Swift
/// executor at all. A test that only inspected the list afterwards would pass
/// just as well if nothing had ever arrived and the assertion were loose.
final class RecordingSink: EventSink, @unchecked Sendable {

    /// One event, and the thread the core raised it on.
    struct Received: Sendable {
        let event: CoreEvent
        let thread: String
    }

    private let lock = NSLock()
    private var received: [Received] = []

    func emit(event: CoreEvent) {
        lock.lock()
        defer { lock.unlock() }
        received.append(Received(event: event, thread: RecordingSink.currentThread()))
    }

    var snapshot: [Received] {
        lock.lock()
        defer { lock.unlock() }
        return received
    }

    /// Every distinct thread the core has emitted on so far.
    var threads: [String] {
        var seen: [String] = []
        for entry in snapshot where !seen.contains(entry.thread) {
            seen.append(entry.thread)
        }
        return seen
    }

    /// The case names, in arrival order, for a failure message.
    ///
    /// Names only, never values: `entryAdded` embeds an Entry and
    /// `pairShortcode` is the pairing secret for the next two minutes. The
    /// shipped ``StreamEventSink`` logs under exactly the same rule.
    var names: [String] {
        snapshot.map { RecordingSink.name(of: $0.event) }
    }

    /// The first recorded event matching `predicate`, waiting for one to arrive.
    func first(
        matching predicate: @escaping (CoreEvent) -> Bool,
        timeout: TimeInterval = Suite.timeout
    ) async -> Received? {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if let found = snapshot.first(where: { predicate($0.event) }) { return found }
            try? await Task.sleep(nanoseconds: 100_000_000)
        }
        return nil
    }

    /// A thread's identity, stable for as long as the thread lives.
    ///
    /// `Thread.current.name` is empty for a thread nothing named, which is every
    /// thread the core's runtime creates, so the kernel id is what distinguishes
    /// them. The main thread is called out by name because "not the main thread"
    /// is one of the two things this suite asserts about the crossing.
    static func currentThread() -> String {
        var id: UInt64 = 0
        pthread_threadid_np(nil, &id)
        return Thread.isMainThread ? "main(\(id))" : "thread(\(id))"
    }

    static func name(of event: CoreEvent) -> String {
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
