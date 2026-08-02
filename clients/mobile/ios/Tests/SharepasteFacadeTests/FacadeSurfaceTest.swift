import Foundation
import SharepasteCore
import SharepasteKit
import XCTest

/// Every exported operation is reachable from Swift, and all three platform
/// traits are supplied by Swift.
///
/// Most operations need a Pairing and this facade has none, so most of them are
/// exercised through their failure. That is not a weaker test than a happy path:
/// a typed ``AppError`` arriving in Swift proves the whole chain — argument
/// lowering, the blocking `block_on`, the error's case surviving the crossing —
/// for exactly the same code the happy path would use. The one case that
/// genuinely matters on a phone, `InsecureRelay`, can only be produced this way.
///
/// This is the one class that talks to the core rather than to
/// ``SharepasteRepository``. The chokepoint deliberately exposes a *narrower*
/// surface than the facade has — no `updateSettings`, no `readEntry`, no
/// `pairWithInvite`, because the app needs none of them — and the surface under
/// test here is the boundary itself.
final class FacadeSurfaceTest: XCTestCase {

    private var keychain: IosKeychain!
    private var clipboard: RecordingClipboard!
    private var sink: RecordingSink!
    private var core: Sharepaste!

    override func setUpWithError() throws {
        keychain = IosKeychain()
        clipboard = RecordingClipboard()
        sink = RecordingSink()
        // `false` on purpose: this suite reaches the cleartext test relay, which
        // the shipped app's `true` would refuse. That the app itself passes
        // `true` is what `TransportPolicyTest` exists to prove.
        core = try Sharepaste.openInMemory(
            keychain: keychain,
            clipboard: clipboard,
            events: sink,
            requireHttps: false
        )
    }

    override func tearDown() {
        core?.stopAllSessions()
        core = nil
    }

    func testSwiftSuppliesTheKeychain() throws {
        // The shipped implementation, exercised on the simulator: real Keychain
        // Services items filed `AfterFirstUnlockThisDeviceOnly`, which is the
        // one part of this that cannot be tested anywhere else.
        try keychain.delete(account: "surface-test")
        XCTAssertNil(try keychain.get(account: "surface-test"))
        try keychain.put(account: "surface-test", secret: "a-secret")
        XCTAssertEqual(try keychain.get(account: "surface-test"), "a-secret")
        // Written twice, because `put` is an update-or-insert and the update arm
        // is the one a second pairing on the same account takes.
        try keychain.put(account: "surface-test", secret: "a-second-secret")
        XCTAssertEqual(try keychain.get(account: "surface-test"), "a-second-secret")
        try keychain.delete(account: "surface-test")
        XCTAssertNil(try keychain.get(account: "surface-test"))
    }

    func testSwiftSuppliesTheClipboard() throws {
        try core.writeClipboard(text: "clipboard crossing")
        XCTAssertEqual(clipboard.written, ["clipboard crossing"])
        // And the read comes back through the same object, which is what makes
        // an Offered Capture of the pasteboard possible at all. Unlike Android,
        // a simulator's pasteboard is readable without window focus.
        XCTAssertEqual(try clipboard.readText(), "clipboard crossing")
    }

    func testTheReadOnlyOperationsAnswerOnAnEmptyFacade() throws {
        XCTAssertEqual(try core.listPairings().count, 0)
        XCTAssertNil(core.activePairing())
        XCTAssertNil(try core.resumeActivePairing())
        XCTAssertEqual(core.connectionState(userId: "nobody"), .disconnected)
        XCTAssertEqual(try core.listHistory(userId: "nobody", beforeId: nil, limit: 50).count, 0)
        XCTAssertNil(try core.readEntry(userId: "nobody", entryId: 1))
    }

    func testSettingsRoundTripThroughATypedPatch() throws {
        let disabled = try core.updateSettings(patch: SettingsPatch(captureEnabled: false))
        XCTAssertFalse(disabled.captureEnabled)

        let bound = try core.updateSettings(
            patch: SettingsPatch(hotkey: .set(hotkey: "CommandOrControl+Shift+V"))
        )
        XCTAssertEqual(bound.hotkey, "CommandOrControl+Shift+V")

        // The reason `hotkey` is an enum rather than a nullable: "leave it
        // alone" and "clear it" are different asks, and one optional cannot say
        // both.
        let cleared = try core.updateSettings(patch: SettingsPatch(hotkey: .clear))
        XCTAssertNil(cleared.hotkey)
        XCTAssertFalse(cleared.captureEnabled)

        let untouched = try core.updateSettings(
            patch: SettingsPatch(denyList: ["com.example.vault"])
        )
        XCTAssertEqual(untouched.denyList, ["com.example.vault"])
        XCTAssertNil(untouched.hotkey, "an absent field must not clear a stored one")
    }

    func testACleartextRelayIsExplainedRatherThanGuessedAt() {
        // Port 1 refuses immediately. The core does not reject `http://` — a
        // desktop paired to a cleartext relay must keep working — but when the
        // request fails it says why, instead of surfacing an opaque transport
        // error a phone's owner cannot act on.
        XCTAssertThrowsError(
            try core.pairWithInvite(
                serverUrl: "http://127.0.0.1:1",
                token: "not-a-real-token",
                deviceLabel: "facade surface test"
            )
        ) { error in
            guard case AppError.InsecureRelay(let detail) = error else {
                return XCTFail("a dead cleartext relay must be explained: \(error)")
            }
            XCTAssertFalse(detail.isEmpty)
        }
    }

    func testEveryOperationThatNeedsAPairingSaysSoInItsOwnWords() {
        let attempts: [(String, () throws -> Void)] = [
            ("pairStart", { _ = try self.core.pairStart(userId: "nobody") }),
            ("pairWithCode", {
                _ = try self.core.pairWithCode(code: "not-a-code", deviceLabel: "surface test")
            }),
            ("startSession", { try self.core.startSession(userId: "nobody") }),
            ("recall", { try self.core.recall(userId: "nobody", entryId: 1) }),
            ("recallLatest", { _ = try self.core.recallLatest(userId: "nobody") }),
            ("offer", { _ = try self.core.offer(userId: "nobody", text: "text") }),
            ("deleteEntry", { try self.core.deleteEntry(userId: "nobody", entryId: 1) }),
            ("clearHistory", { try self.core.clearHistory(userId: "nobody") }),
        ]
        for (name, attempt) in attempts {
            do {
                try attempt()
                XCTFail("\(name) must not succeed without a Pairing")
            } catch let error as AppError {
                // The case is not pinned per operation on purpose: which one a
                // given call maps to is the core's business and has its own
                // tests there. What has to hold *here* is that it crossed the
                // boundary as a typed error rather than as a trap.
                XCTAssertFalse("\(error)".isEmpty)
            } catch {
                XCTFail("\(name) raised something that is not an AppError: \(error)")
            }
        }
    }

    func testTheTolerantOperationsAreCallableOnAnEmptyFacade() throws {
        // These four do not fail on an unknown user, and that is the facade's
        // behaviour rather than an oversight: selecting and forgetting are
        // idempotent bookkeeping, and stopping something that is not running is
        // what a resign-active edge does on every launch.
        try core.setActivePairing(userId: "nobody")
        XCTAssertEqual(core.activePairing(), "nobody")
        try core.forgetPairing(userId: "nobody")
        XCTAssertNil(core.activePairing(), "forgetting the active pairing must clear it")
        core.stopSession(userId: "nobody")
        core.stopAllSessions()
    }
}
