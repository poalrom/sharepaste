import Foundation
import SharepasteCore
import SharepasteKit
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
/// Every Offer here goes through ``SharepasteRepository/offerPasteboard()``,
/// because that is the operation: it reads what is on the pasteboard and hands
/// it to the core's one capture filter. The pasteboard it reads is the phone's
/// ``TestPasteboard`` — `UIPasteboard` is not reachable from this bundle, and
/// the reasoning is on that type.
///
/// One claim is therefore out of reach and is not pretended at: that
/// ``IosPasteboard`` refuses to coerce a copied *image* into a string.
/// `UIPasteboard.string` would happily answer with a description of something
/// only this phone can open, which is why the shipped reader asks `hasStrings`
/// first — and a fake pasteboard cannot exercise a real one's coercion rules.
/// What is proven here is the half that lives in the core: handed nothing
/// text-like, it answers `nonText` rather than uploading an empty Entry.
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
        phone.pasteboard.put("the same link twice \(Int(Date().timeIntervalSince1970 * 1000))")

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
        phone.pasteboard.put(String(repeating: "a", count: 64 * 1024 + 1))
        try await assertRefused(.tooLarge, what: "over-size")
    }

    /// A pasteboard holding something that is not text.
    ///
    /// `nil` is what ``IosPasteboard/readText()`` answers for a pasteboard
    /// holding a screenshot, and ``SharepasteRepository/offerPasteboard()``
    /// deliberately offers the **empty string** for it rather than deciding
    /// "there is no text here" a second time up in the shell. The core's one
    /// capture filter is what calls that `nonText`, and that filter is the thing
    /// with the tests.
    func testOfferingANonTextPayloadIsRefusedAsNotText() async throws {
        phone.pasteboard.put(nil)
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
}
