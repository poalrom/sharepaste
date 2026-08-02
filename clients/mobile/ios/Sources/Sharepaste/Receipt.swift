import SwiftUI
import UIKit

/// A ``Receipt``, on the surface this phone actually has.
///
/// **A Receipt confirms and vanishes; a ``Notice`` waits to be acted on.** The
/// split is by outcome kind and not by which path invoked the verb, and it is
/// the reason this file exists apart from ``NoticeBand``: everything drawn here
/// says a thing happened and needs nothing back, so it takes no tap, offers no
/// control and goes on its own. ``Notice/recalledFromCache`` is the one that
/// proves the line is real — it reads like a confirmation and is a Notice,
/// because ADR 0007 says handing back what may be yesterday's link may never be
/// silent.
///
/// **The surface is the app in front of the person, and that is a decision
/// rather than the obvious choice.** `CONTEXT.md` says a Receipt is "the same
/// whether the app was open or closed", and Android satisfies that with a Toast
/// because a Toast is the only surface a closed Android phone has. A closed iOS
/// app has Shortcuts instead: an App Intent reports through its own return value
/// and its own error, which is ticket 07's business and not a view's. So there
/// is no `Aloud` variant here and there must never be one — the two paths report
/// through two mechanisms because the platform gave them two, and spec row 17
/// says to record that the reason was the platform's.
///
/// The two surfaces ruled out, so nobody re-proposes them:
///
///  * **A notification.** Would reach a person who is not looking, which is the
///    whole appeal — and it is entitlement-gated on a free Personal Team (ADR
///    0007, ADR 0008), so it cannot exist. It would also be a durable surface
///    for a Preview, which is precisely what ADR 0009 weighed and accepted only
///    for something transient.
///  * **A dismissible card.** One control away from a Notice, and the
///    difference between the two would then be nothing a person could see.
///
/// ADR 0009 is what makes ``Receipt/recalled(preview:)`` carry an Entry's
/// Preview at all: `AndroidClipboard` used to mark every Recall sensitive, so
/// the one thing a person needs — *did I get the right Entry?* — was the one
/// thing the phone would not say. The exposure that buys is accepted under
/// **R3**: it is on screen, in the app's own view, on a phone already unlocked
/// and in its owner's hand. `SHOW WHAT WAS RECALLED` silences it entirely rather
/// than redacting it, and that guard lives in the state holder — by the time a
/// Receipt reaches this file it is one that is meant to be seen.
///
/// Nothing here logs. A Receipt is a few seconds over one person's shoulder; a
/// log line is durable, readable over a cable, and nobody asked for it.
@MainActor
struct ReceiptCard: View {

    let receipt: Receipt

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            FuiBadge(text: receipt.label, accent: .emitter)
            Text(receipt.sentence)
                .fuiText(Fui.prose, color: Fui.textBody)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(Fui.gutter)
        // Raised rather than recessed, and opaque: this is the one thing in the
        // app drawn *over* the list rather than in it, and a translucent card
        // would put an Entry's Preview on top of another Entry's Preview.
        .background(Fui.raised)
        .clipShape(NotchShape())
        .overlay { NotchShape().strokeBorder(Fui.frame, lineWidth: 1) }
        // The label and the sentence are one statement, not two stops.
        .accessibilityElement(children: .combine)
    }
}

extension View {

    /// Draw Receipts over this view as they happen.
    ///
    /// Applied once, at the root, rather than per screen: a Recall raised from
    /// the Settings Screen is the same outcome as one raised from the History
    /// and must not report itself in two places or in two shapes.
    ///
    /// See `ReceiptOverlay` for the timing and for why it takes no tap.
    func receiptOverlay(_ receipt: SharepasteViewModel.TimestampedReceipt?) -> some View {
        modifier(ReceiptOverlay(receipt: receipt))
    }
}

/// The transient part: it appears, it holds long enough to read a Preview, and
/// it goes.
///
/// **It takes no tap, and that is the structural difference from a ``Notice``
/// rather than a detail of the animation.** `allowsHitTesting(false)` means a
/// Receipt sitting over the verb bar cannot swallow a second press of `RECALL
/// LATEST`, and it means there is no control to look for — a person who wants it
/// gone waits, because a Receipt needs nothing back. A ``NoticeBand`` carries a
/// `DISMISS` for exactly the opposite reason.
///
/// **The stamp is what makes two identical Recalls two Receipts.** ``Receipt``
/// alone is `Equatable`, so recalling the same Entry twice would be a value that
/// never changed and a card that never re-appeared;
/// ``SharepasteViewModel/TimestampedReceipt`` carries the moment and `task(id:)`
/// restarts on it. The state holder owns that, not this view.
///
/// The hold is 3.5 seconds, which is Android's `Toast.LENGTH_LONG` to the
/// millisecond. Not a coincidence and not a guess: a Recall Receipt names a
/// Preview, which is a line of text somebody has to read rather than a tick, and
/// two phones showing the same Entries should not disagree about how long a
/// person gets to read one.
@MainActor
private struct ReceiptOverlay: ViewModifier {

    let receipt: SharepasteViewModel.TimestampedReceipt?

    /// What is on screen, as against what the state holder last raised. Its own
    /// value because the card outlives the event by the length of the hold.
    @State private var shown: SharepasteViewModel.TimestampedReceipt?

    func body(content: Content) -> some View {
        content
            .overlay(alignment: .bottom) {
                if let shown {
                    ReceiptCard(receipt: shown.receipt)
                        .padding(.horizontal, Fui.gutter)
                        .padding(.bottom, Fui.gutter)
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                        // Over the verb bar, deliberately: the alternative is
                        // reserving a strip of every screen for something that
                        // is absent almost always. Inert, so the bar underneath
                        // keeps working while it is up.
                        .allowsHitTesting(false)
                }
            }
            .animation(.easeOut(duration: 0.18), value: shown)
            .task(id: receipt) {
                guard let receipt else { return }
                shown = receipt
                // VoiceOver reaches a card that appears and leaves on its own
                // only if it is announced; TalkBack does this for a Toast
                // without being asked. Same exposure as the visible card and
                // accepted the same way (R3) — and silenced by the same switch,
                // because a Receipt the state holder suppressed never arrives
                // here at all.
                UIAccessibility.post(
                    notification: .announcement,
                    argument: "\(receipt.receipt.label). \(receipt.receipt.sentence)"
                )
                try? await Task.sleep(for: .milliseconds(3500))
                // Only if nothing has replaced it. A second Recall while the
                // first card is up restarts this task, and the cancelled one
                // must not then clear the card its successor drew.
                if shown == receipt { shown = nil }
            }
    }
}

#if DEBUG

/// Both Receipts, and the shapes the Recall one takes.
///
/// Four cards rather than two, because ``Receipt/recalled(preview:)`` is one
/// variant with a nullable Preview and the empty arm is the one that escaped
/// Android's guard. A gallery that only ever drew the happy arm would be the
/// same blind spot in a different file.
struct ReceiptGallery: View {

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            SectionHeading("RECEIPT · OFFERED")
            ReceiptCard(receipt: .offered(pending: 1))

            SectionHeading("RECEIPT · RECALLED, PREVIEW NAMED")
            ReceiptCard(receipt: .recalled(preview: "https://example.invalid/the-one-i-meant"))

            SectionHeading("RECEIPT · RECALLED, NO PREVIEW TO NAME")
            ReceiptCard(receipt: .recalled(preview: nil))

            SectionHeading("RECEIPT · RECALLED, PREVIEW BLANK")
            ReceiptCard(receipt: .recalled(preview: "   "))
        }
        .padding(Fui.gutter)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Fui.panel)
        .fuiBackdrop()
    }
}

/// `PreviewProvider` rather than the `#Preview` macro, and not by preference.
///
/// The macro expands to `#externalMacro(module: "PreviewsMacros", …)`, whose
/// plugin ships inside Xcode. This package is built by `swift build --swift-sdk
/// arm64-apple-ios` on the open-source toolchain (spec rows 1 and 3), where that
/// plugin does not exist and the macro is a hard compile error, not a warning.
/// `PreviewProvider` compiles clean on the same command and draws the same
/// picture. Every preview in this shell is one for that reason.
struct ReceiptGallery_Previews: PreviewProvider {
    static var previews: some View {
        ReceiptGallery().previewDisplayName("Receipts")
    }
}

#endif
