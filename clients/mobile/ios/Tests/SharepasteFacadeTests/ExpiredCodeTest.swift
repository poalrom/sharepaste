import Foundation
import SharepasteCore
import XCTest

/// A scan that arrives after the 120-second slot has expired.
///
/// The third pairing failure mode, proven by letting a real code really expire
/// rather than by handing a screen a state and believing it. It costs the suite
/// a little over two minutes, which is the price of the slot's actual lifetime:
/// the relay is what decides when a code dies (`pairingTtlMs = 2 * 60 * 1000`),
/// and it answers a claim against a dead slot with `410 Gone`, which the core
/// maps to `AppError.PairExpired`.
///
/// Faking it with an unknown pair id would take the `404` path instead, which
/// the app maps to the same sentence but is a different journey through the
/// relay. If the relay ever changes which status an expired slot gets, this test
/// notices and that one would not.
final class ExpiredCodeTest: XCTestCase {

    private var phone: PhoneUnderTest!

    override func setUpWithError() throws {
        phone = try PhoneUnderTest.open(databaseName: "expiry-proof.db")
    }

    override func tearDown() async throws {
        await phone.close()
        phone = nil
    }

    func testACodeClaimedAfterItsSlotExpiresIsReportedAsExpired() async throws {
        let other = try await Inviter.shared()
        let code = try await other.freshCompactCode()

        // The relay holds the slot for 120 seconds. Waiting past it is the only
        // way to observe the real thing.
        try await Task.sleep(nanoseconds: ExpiredCodeTest.wait)

        do {
            _ = try await phone.repo.pairWithCode(code: code, deviceLabel: "too late")
            XCTFail("a code whose 120-second slot has closed must not pair")
        } catch AppError.PairExpired(let detail) {
            XCTAssertFalse(detail.isEmpty, "the relay's own reason travels with it")
        }

        // Nothing was left behind by the failed attempt.
        let pairings = try await phone.repo.listPairings()
        XCTAssertTrue(pairings.isEmpty, "a failed claim must leave no Pairing: \(pairings)")
    }

    /// The relay's `pairingTtlMs` is 120s; five seconds past it removes any
    /// doubt.
    private static let wait: UInt64 = 125_000_000_000
}
