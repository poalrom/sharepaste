import Foundation
import SharepasteCore
import SharepasteKit
import UIKit
import XCTest

/// The three refusals an Offer can really receive, each on a real facade.
///
/// A phone's screens pin the *wording* for each reason; this pins that the
/// **core** produces those reasons for those inputs. Both halves are needed:
/// wording asserted against a reason nothing can produce proves nothing, and a
/// reason with no words is a button that appears to do nothing. Only the second
/// half is here — spec row 10 buys no UI test, and the sentences are asserted by
/// hand on the device.
///
/// Every Offer here goes through the pasteboard, because that is the operation:
/// ``SharepasteRepository/offerPasteboard()`` reads what is on the pasteboard
/// and hands it to the core's one capture filter. A simulator's pasteboard is
/// readable without window focus, so unlike Android this needs no activity in
/// front of it.
final class OfferRefusalsTest: XCTestCase {

    private var phone: PhoneUnderTest!
    private var userId: String!

    override func setUp() async throws {
        phone = try PhoneUnderTest.open(databaseName: "offer-refusals-proof.db")
        userId = try await phone.pair(
            with: try await Inviter.shared(),
            label: "offer refusals test phone"
        )
        _ = try await phone.enterForeground()
        try await phone.awaitInContact(userId)
    }

    override func tearDown() async throws {
        await phone.close()
        phone = nil
    }

    /// The same text twice.
    ///
    /// A repeat of the immediately preceding capture costs nothing to drop, and
    /// an Offered Capture is the easiest way to send one twice — the button is
    /// right there and nothing about the pasteboard has changed.
    func testOfferingTheSameTextTwiceIsRefusedAsADuplicate() async throws {
        UIPasteboard.general.string = "the same link twice \(Int(Date().timeIntervalSince1970 * 1000))"

        let first = try await phone.repo.offerPasteboard()
        guard case .settled(_, let taken) = first, case .queued = taken else {
            return XCTFail("the first Offer must be taken: \(first)")
        }

        try await assertRefused(.duplicate, what: "duplicate")
    }

    /// A payload over the 64 KiB cap.
    ///
    /// One byte over, not comfortably over: the cap is `MAX_BYTES` in the core's
    /// one filter, and a test that offered a megabyte would pass under any cap
    /// at all.
    func testOfferingAnOverSizePayloadIsRefusedForItsSize() async throws {
        UIPasteboard.general.string = String(repeating: "a", count: 64 * 1024 + 1)
        try await assertRefused(.tooLarge, what: "over-size")
    }

    /// A pasteboard holding something that is not text.
    ///
    /// An image, which is what copying a screenshot leaves behind. It matters
    /// that this is an image and not an empty pasteboard: `UIPasteboard.string`
    /// coerces, so a phone that trusted it could encrypt and upload a
    /// description of something only this phone can open. ``IosPasteboard`` asks
    /// `hasStrings` first and therefore hands the core nothing — and the core's
    /// one filter is what calls that `nonText`.
    func testOfferingANonTextPayloadIsRefusedAsNotText() async throws {
        UIPasteboard.general.image = Self.oneOpaquePixel()
        XCTAssertFalse(
            UIPasteboard.general.hasStrings,
            "the pasteboard still holds text, so this test would prove nothing"
        )
        try await assertRefused(.nonText, what: "non-text")
    }

    private func assertRefused(
        _ expected: SkipReason,
        what: String,
        file: StaticString = #filePath,
        line: UInt = #line
    ) async throws {
        let attempt = try await phone.repo.offerPasteboard()
        guard case .settled(_, let outcome) = attempt else {
            return XCTFail("a paired phone must not report itself unpaired: \(attempt)", file: file, line: line)
        }
        guard case .rejected(let reason) = outcome else {
            return XCTFail("the \(what) Offer must be refused, and it was \(outcome)", file: file, line: line)
        }
        XCTAssertEqual(reason, expected, "the \(what) Offer was refused for the wrong reason", file: file, line: line)
    }

    /// The smallest thing that is unambiguously not text.
    ///
    /// Drawn rather than loaded: a test bundle resource would be one more file
    /// for a one-pixel image, and `UIGraphicsImageRenderer` is on every iOS this
    /// app supports.
    private static func oneOpaquePixel() -> UIImage {
        UIGraphicsImageRenderer(size: CGSize(width: 1, height: 1)).image { context in
            UIColor.black.setFill()
            context.fill(CGRect(x: 0, y: 0, width: 1, height: 1))
        }
    }
}
