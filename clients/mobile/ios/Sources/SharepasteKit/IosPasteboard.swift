import Foundation
import SharepasteCore
import UIKit

/// The system pasteboard.
///
/// ``readText()`` returning `nil` is ordinary, not an error, and it means the
/// ordinary thing: the pasteboard is holding something that is not text at all,
/// which is what an Offered Capture has to be able to refuse. Reading it is also
/// not free — since iOS 16 a read the person did not authorise raises the paste
/// banner — which is why the app reads only when somebody presses Offer, and why
/// the Offer intent takes its text as a parameter instead (ADR 0007: *Shortcuts
/// touches the pasteboard, never us*).
///
/// ``writeText(text:)`` is the **raw** write. The self-write marker that stops a
/// watcher re-capturing our own write is the facade's job and is not
/// reimplemented here; a shell that tries gets the ordering wrong and a Recall
/// becomes a Capture of itself.
public final class IosPasteboard: Clipboard, @unchecked Sendable {

    public init() {}

    public func readText() throws -> String? {
        let pasteboard = UIPasteboard.general
        // `hasStrings` is the platform's own answer to "is there text here", and
        // asking it first is what keeps a screenshot out of the protocol:
        // `pasteboard.string` coerces, so handed an image it can answer a
        // perfectly good `String` describing something only this phone can open.
        // It is also the cheap question — `hasStrings` does not count as a read
        // and raises no banner.
        guard pasteboard.hasStrings else { return nil }
        guard let text = pasteboard.string, !text.isEmpty else { return nil }
        return text
    }

    public func writeText(text: String) throws {
        // The write is deliberately marked with **nothing** — see ADR 0009.
        // Android used to set `EXTRA_IS_SENSITIVE` and removed it; the iOS
        // analogue would be `UIPasteboard.OptionsKey.localOnly` and an expiry,
        // and setting either here would be reinstating a decision that has
        // already been made and reversed. The app draws its own Receipt instead,
        // because most keyboards show no paste chip and no API reports which do.
        UIPasteboard.general.string = text
    }
}
