import SharepasteCore
import SwiftUI

/// The chrome more than one screen draws.
///
/// The rule is the file's whole reason to exist: **a band two screens draw lives
/// here, a band one screen draws stays private to that screen.** Two copies of a
/// chrome band is the drift ADR 0010 was written about, one layer down — the ADR
/// is about three palettes disagreeing, and this is the same failure at the size
/// of a sentence, where nothing checks it at all. The Contact readout says one
/// set of words about one set of states; a second copy on the Settings Screen
/// would be a second set within a release.
///
/// **No test tags.** Android hangs a `testTag` off every branch here, and each
/// one earns its keep against 116 instrumented tests. Spec row 10 buys none of
/// those for iOS — no XCUITest, no snapshot tests — so a tag would be a string
/// constant naming a node nothing ever asks for. The tags are dropped rather
/// than transcribed, and this paragraph is why an iOS reader diffing the two
/// files finds them missing.
///
/// One Android readout has no counterpart at all: `StandingActionsBlockedNote`.
/// It reports a notification the platform is refusing to draw, and this phone
/// posts no notification to be refused — Standing Actions reach the person
/// through Shortcuts (ADR 0007, spec row 12), and spec row 23 says the person is
/// told how on the Settings Screen rather than warned about it in chrome.

// ── Headers ──────────────────────────────────────────────────────────────────

/// A screen's own header: its name, and the way back if there is one.
///
/// Shared so that three screens with a back arrow cannot end up with three
/// arrows in three places. Android holds it in `PairingScreen.kt` because two
/// screens drew it there; here three do, and the file named for shared readouts
/// is where the third one looks.
///
/// ``onBack`` is optional and `nil` draws no control, which is not the same as
/// drawing a disabled one. On a fresh install the pairing flow is the whole app,
/// and a `◂` leading to an empty History would be a dead end wearing a door's
/// clothes.
///
/// **The `◂` is the only way back, and the left-edge swipe does nothing.** Spec
/// row 29; the argument is in ``SharepasteRoot``.
@MainActor
struct TitleBand: View {

    let title: String
    /// What the `◂` is, in words. The glyph is a picture of the door, and
    /// "left-pointing small triangle" is not what a screen reader should say.
    let backDescription: String
    var onBack: (() -> Void)?

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                if let onBack {
                    Button(action: onBack) {
                        Text(Glyphs.back)
                            .fuiText(Fui.glyph, color: Fui.cyan300)
                            .accessibilityHidden(true)
                            .frame(width: Fui.target, height: Fui.target)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(backDescription)
                }
                Text(title)
                    .fuiText(Fui.heading, color: Fui.textPrimary)
                    .padding(.leading, onBack == nil ? 10 : 4)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 4)
            .frame(height: 52)
            .background(Fui.cyanA08)
            Hairline()
        }
    }
}

/// Who this phone is, and the only door off the History Screen.
///
/// The identity is the **Viewed** Pairing rather than the Active one, because it
/// heads the list underneath it and the list is the Viewed Pairing's. When the
/// two differ the badge says so here as well as in ``DivergenceBand`` — the band
/// explains, this states, and somebody who has scrolled the band out of a long
/// History still has the fact in front of them.
///
/// **The User slot holds a username or `…`, never a `user_id`.** Until the
/// Relay's `/me` mirror answers there is no name, and the id is a 36-character
/// uuid: it fills the line, pushes the Relay host off the end of it and names
/// nothing a person could recognise. ``UiState/identityUser(_:)`` is the single
/// statement of that rule and is asked rather than re-derived here.
/// ``UiState/nameOf(_:)`` still falls back to the id, and that is not an
/// inconsistency — it names a Pairing inside a sentence, where `…` would name
/// nothing.
///
/// Lives here rather than in `HistoryScreen.swift` because it is chrome by the
/// same definition the rest of this file is, and because the `◎` is the one
/// route between the two surfaces — a screen that grew its own header would be
/// a screen that could quietly lose the door.
@MainActor
struct IdentityBand: View {

    let state: UiState
    let actions: AppActions

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                VStack(alignment: .leading, spacing: 0) {
                    Text(Strings.historyTitle)
                        .fuiText(Fui.micro, color: Fui.textEmitter)
                    if let viewed = state.pairings.first(where: { $0.userId == state.viewedPairing }) {
                        Text(
                            Strings.historyIdentity(
                                user: state.identityUser(state.viewedPairing),
                                host: viewed.relayHost
                            )
                        )
                        .fuiText(Fui.data, color: Fui.textBody)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)

                if state.diverged {
                    FuiBadge(text: Strings.pairingsViewedBadge, accent: .neutral)
                }
                // The only way to the Settings Screen, and it belongs here rather
                // than behind a drawer: on a phone holding one Pairing it is a
                // door to the settings, and on a phone holding several it is the
                // only place the other ones exist at all.
                GlyphButton(
                    glyph: Glyphs.pairings,
                    action: actions.openSettings,
                    accessibilityLabel: Strings.settingsOpen
                )
            }
            .padding(.leading, Fui.gutter)
            .padding(.trailing, 8)
            .frame(height: 52)
            .background(
                LinearGradient(
                    colors: [Fui.cyanA08, .clear],
                    startPoint: .top,
                    endPoint: .bottom
                )
            )
            Hairline()
        }
    }
}

/// A section heading on a screen that has no cards to head.
///
/// The emitter's label voice: small, tracked, and never competing with the
/// sentence it introduces.
@MainActor
struct SectionHeading: View {

    private let text: String

    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text).fuiText(Fui.micro, color: Fui.textEmitter)
    }
}

// ── Contact ──────────────────────────────────────────────────────────────────

/// What the phone says about its own Contact with the Relay.
///
/// **"Not in contact" is nominal, and this view is where that rule is kept.** On
/// a desktop, relay health surfaces only when it is degraded (ADR 0002) — a
/// sensible rule for something that is always connected. A phone is out of
/// contact almost all of the time, because sync is foreground-only (ADR 0007),
/// so the same rule would paint a perfectly healthy phone as permanently broken.
///
/// The inversion is structural rather than a change of wording: the readout is
/// **permanent chrome**, one band that is always there, so its appearance
/// carries no news of its own and only the words inside it change. Every phase
/// except ``SessionPhase/refused(userId:detail:)`` is a status light in the
/// ordinary voice, with no container and no alert colour.
///
/// A revoked Pairing is the one thing a person has to act on, so it is the one
/// thing that looks like it — and the only one that is a sentence rather than a
/// readout, because no amount of waiting fixes a revoked token and the band has
/// to say what to do instead. ``onPairAgain`` is how; `nil` leaves the sentence
/// standing without its control.
@MainActor
struct ContactReadout: View {

    let phase: SessionPhase
    var onPairAgain: (() -> Void)?

    var body: some View {
        if let readout = phaseReadout(phase) {
            switch toneOf(phase) {
            case .nominal:
                ChromeBand(height: 30, scanlines: true) {
                    StatusLight(signal: signalOf(phase), label: readout)
                }
            case .fault:
                VStack(spacing: 0) {
                    ChromeBand(height: 30, background: Fui.alertA16) {
                        StatusLight(signal: .alert, label: readout)
                    }
                    VStack(alignment: .leading, spacing: 10) {
                        Text(Strings.contactRefused)
                            .fuiText(Fui.prose, color: Fui.textPrimary)
                            .fixedSize(horizontal: false, vertical: true)
                        if let onPairAgain {
                            FuiButton(
                                text: Strings.contactPairAgain,
                                action: onPairAgain,
                                accent: .alert,
                                solid: true
                            )
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(Fui.gutter)
                    .background(Fui.alertA16)
                    Hairline(color: Fui.alertA40)
                }
            }
        }
    }
}

/// One Pairing's own status, on its card.
///
/// The same words and the same tone rule as ``ContactReadout``, in the shape a
/// card has room for: no band, no scanlines, and the fault branch tinted in
/// place rather than pinned above a list.
///
/// Android's second parameter is a test tag and has no counterpart here — see
/// the note at the top of this file.
@MainActor
struct PairingStatus: View {

    let phase: SessionPhase

    var body: some View {
        if let readout = phaseReadout(phase) {
            switch toneOf(phase) {
            case .nominal:
                StatusLight(signal: signalOf(phase), label: readout)
            case .fault:
                VStack(alignment: .leading, spacing: 6) {
                    StatusLight(signal: .alert, label: readout)
                    Text(Strings.contactRefused)
                        .fuiText(Fui.prose, color: Fui.textPrimary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(10)
                .background(Fui.alertA16)
            }
        }
    }
}

/// The sentence for a phase, in one exhaustive `switch`.
///
/// Shared by the whole-phone readout and by a single Pairing's card, so there is
/// one set of words for one set of states rather than two that drift. `nil` is
/// "say nothing": an unpaired phone is on the pairing flow, where a status line
/// would be noise, and both callers return on it rather than inventing words for
/// a Pairing that does not exist.
private func phaseReadout(_ phase: SessionPhase) -> String? {
    switch phase {
    case .unpaired: nil
    case .looking: Strings.contactLooking
    case .inContact: Strings.contactOnline
    case .outOfContact: Strings.contactOffline
    case .resting: Strings.contactResting
    case .notActive: Strings.contactNotActive
    case .refused: Strings.contactRefusedShort
    }
}

/// Which lamp a phase lights.
///
/// Three of the states are nominal and only one is green: being *in* contact is
/// the exceptional state on a phone, and a band that went green whenever nothing
/// was wrong would be grey almost always and would read as a warning. Standby is
/// the resting colour, caution is work in progress, and alert is only ever the
/// revoked token.
///
/// Which phases are faults is ``toneOf(_:)``'s rule and is asked rather than
/// restated, so a phase cannot be a fault here and a nominal lamp there.
private func signalOf(_ phase: SessionPhase) -> Signal {
    if toneOf(phase) == .fault { return .alert }
    switch phase {
    case .inContact: return .nominal
    case .looking: return .caution
    case .unpaired, .outOfContact, .resting, .notActive: return .standby
    // Answered above. Repeated rather than swept up by a `default`, so a phase
    // added to the core arrives here as a compile error.
    case .refused: return .alert
    }
}

// ── Divergence ───────────────────────────────────────────────────────────────

/// The Viewed Pairing is not the one this phone syncs, said out loud.
///
/// Without this band the History shows one Pairing's Entries while the device
/// syncs another and nothing on screen admits it — so a list that is quietly
/// frozen looks exactly like a list that is up to date. It offers the one action
/// that resolves the divergence rather than merely reporting it.
///
/// Nominal in tone, deliberately: viewing a Pairing this phone is not syncing is
/// a thing a person chose to do, not a fault. Drawn in the emitter's own tint
/// rather than in a warning colour.
@MainActor
struct DivergenceBand: View {

    let viewedName: String
    let activeName: String
    let onUseViewed: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 8) {
                Text(Strings.pairingDiverged(viewed: viewedName, active: activeName))
                    .fuiText(Fui.prose, color: Fui.textPrimary)
                    .fixedSize(horizontal: false, vertical: true)
                FuiButton(text: Strings.pairingDivergedUse, action: onUseViewed)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, Fui.gutter)
            .padding(.vertical, 12)
            .background(Fui.active)
            Hairline(color: Fui.frame)
        }
    }
}

// ── Notice ───────────────────────────────────────────────────────────────────

/// What the last thing the person asked for did, when it needs acting on.
///
/// One `switch` over ``Notice``, so an outcome added without words for it does
/// not compile. Each carries a label naming the outcome and then the sentence,
/// which is the shape a ``Receipt``'s overlay draws too: a Standing Action and a
/// press on this screen are the same operation, and reporting one of them in two
/// idioms would make them look like two.
///
/// **Six outcomes reach this band, and the two that no longer do are the
/// point.** A plain Offer and a plain Recall confirm and need nothing back, so
/// they are ``Receipt``s and go past. What is left here all needs something done
/// or known, which is what earns a container that waits to be dismissed — and it
/// is why this band is never the report of a verb that simply worked. Chrome
/// that only ever appears with something in it is chrome nobody has to learn to
/// ignore.
///
/// ``Notice/recalledFromCache`` is the only one that tints its whole band, and
/// that is not decoration: every other notice is a statement about something the
/// person just did, while that one is a warning about the content now on their
/// pasteboard — it may be yesterday's link. A refusal is ruled down its left
/// edge instead, in the colour of what to do about it: amber for the two that
/// need something done, inert for `ALREADY HERE`, which is the app working
/// correctly and costs the person nothing.
///
/// Called `NoticeBand` and not `NoticeBanner`, which is the Kotlin's name:
/// `CONTEXT.md` lists "banner" under **Notice**'s _Avoid_, and the glossary
/// outranks a symbol that predates the Receipt/Notice split.
@MainActor
struct NoticeBand: View {

    let notice: Notice
    let onDismiss: () -> Void

    /// Everything one notice's appearance turns on, named once.
    ///
    /// A type with named fields rather than a bare tuple, and one `switch` in
    /// place of five. The label, the accent, the sentence, the left-edge rule
    /// and the tint were each decided in their own exhaustive `switch` over the
    /// same value, so working out what one notice looks like meant assembling it
    /// from five places and adding an outcome meant editing five. The point of
    /// those switches survives the collapse: this one is exhaustive too, so an
    /// outcome added without words for it still does not compile — that
    /// property is what they were bought for, and nothing else about them was.
    private struct Face {
        let label: String
        let accent: Accent
        /// Ruled down its left edge in the colour of what to do about it. A
        /// refusal is; an outcome that simply happened is not.
        let ruled: Bool
        /// The one notice that is about the pasteboard rather than about the
        /// app, and therefore the only one that tints its whole band and wears
        /// its badge solid.
        let stale: Bool
        let sentence: String
    }

    private var face: Face {
        switch notice {
        case let .offerRefused(reason):
            Face(
                label: offerRefusalLabel(reason),
                accent: offerRefusalAccent(reason),
                ruled: true,
                stale: false,
                sentence: offerRefusalMessage(reason)
            )
        case .recalledFromCache:
            Face(
                label: Strings.recallFromCacheBadge,
                accent: .caution,
                ruled: false,
                stale: true,
                sentence: Strings.recallFromCache
            )
        case .unpaired:
            Face(
                label: Strings.noticeNotPaired,
                accent: .emitter,
                ruled: false,
                stale: false,
                sentence: Strings.actionUnpaired
            )
        case let .historyCleared(pairing):
            Face(
                label: Strings.noticeCleared,
                accent: .emitter,
                ruled: false,
                stale: false,
                sentence: Strings.historyCleared(pairing)
            )
        case let .pairingForgotten(pairing, promoted):
            Face(
                label: Strings.noticeForgotten,
                accent: .emitter,
                ruled: false,
                stale: false,
                sentence: promoted.map { Strings.pairingForgottenPromoted(pairing, $0) }
                    ?? Strings.pairingForgottenLast(pairing)
            )
        case let .failed(sentence, detail):
            Face(
                label: Strings.noticeFailed,
                accent: .caution,
                ruled: true,
                stale: false,
                sentence: detail.map { "\(sentence)\n\($0)" } ?? sentence
            )
        }
    }

    var body: some View {
        // Read once, so the six things drawn from it are demonstrably the same
        // decision rather than six evaluations that agree.
        let face = self.face
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                if face.ruled {
                    Rectangle()
                        .fill(face.accent.ink)
                        .frame(width: 2)
                        .frame(maxHeight: .infinity)
                }
                VStack(alignment: .leading, spacing: 8) {
                    FuiBadge(text: face.label, accent: face.accent, solid: face.stale)
                    Text(face.sentence)
                        .fuiText(Fui.prose, color: face.stale ? Fui.textPrimary : Fui.textBody)
                        .fixedSize(horizontal: false, vertical: true)
                    FuiButton(
                        text: Strings.noticeDismiss,
                        action: onDismiss,
                        accent: face.accent,
                        height: Fui.targetSmall
                    )
                    .frame(maxWidth: .infinity, alignment: .trailing)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, Fui.gutter)
                .padding(.vertical, 12)
            }
            // `fixedSize` above, not here: the rule stretches to whatever the
            // sentence needs, and a band that sized itself to the rule would
            // clip the sentence instead.
            .background(face.stale ? Fui.amberA16 : Fui.band)
            Hairline(color: face.stale ? Fui.amberA40 : Fui.hairline)
        }
    }
}

/// How loudly a refusal is drawn.
///
/// `ALREADY HERE` is the one that is not a caution: a duplicate Offer is the app
/// working correctly and a person who tapped Offer twice has lost nothing, so it
/// reads as a fact rather than as something to fix. The other two each need
/// something done — put different content on the pasteboard, or send something
/// smaller — and wear the caution rule that says so.
///
/// Here rather than beside its two siblings in `UiState.swift`, which is the one
/// divergence from the Kotlin's arrangement and is deliberate: ``Accent`` is a
/// SwiftUI value, and the state layer is words and rules with no view framework
/// in it at all. `offerRefusalLabel` and `offerRefusalMessage` return `String`s
/// and stay there; this returns a colour decision and belongs where colours are.
private func offerRefusalAccent(_ reason: SkipReason) -> Accent {
    switch reason {
    case .duplicate: .neutral
    case .nonText, .tooLarge, .disabled, .denyList, .selfWrite, .transient: .caution
    }
}

// ── The pinned disclosure ────────────────────────────────────────────────────

/// The one surprising thing about how this app works, pinned where it cannot
/// scroll away.
///
/// Sync is foreground only, so something copied on the laptop does not reach the
/// phone until Sharepaste is opened. Left unsaid that reads as a bug — a person
/// copies a link, picks up their phone, and the History is empty. On iOS it is
/// forced as well as chosen: Background Modes is entitlement-gated alongside
/// push, and a free Personal Team has neither.
///
/// **It is chrome rather than the first item in the list**, which is the whole
/// of the redesign here: a fact that scrolls away is a fact the puzzled person
/// never reaches, and this is the one they are puzzled about. One clipped line,
/// with the verbatim sentence and the four things that are *not* happening one
/// tap behind it. A band that is simply there says nothing by appearing, which
/// is what lets it be permanent without becoming a warning.
///
/// **Two states, owned in two different places, and that is the argument rather
/// than an accident.** Open/closed is `@State`: it changes nothing about the
/// phone and putting it in the snapshot would mean the state holder owned a fact
/// about a disclosure triangle. Dismissed goes out through ``onDismiss`` to the
/// preference store, because it is a decision about what this phone shows from
/// now on, and the caller stops drawing the band at all.
///
/// Android needs `rememberSaveable` for the first of those because a rotation
/// destroys and recreates its Activity. SwiftUI's `@State` survives a rotation
/// on its own, so the plain property is the honest port rather than a guarantee
/// quietly dropped — and `@SceneStorage`, the near-equivalent, would persist a
/// disclosure triangle across a cold launch, which is more than Android does.
///
/// **Only `▴ CLOSE` dismisses.** The whole band is the tap target, not the chip
/// in it, so the first tap can do nothing but open: a thumb that brushes chrome
/// it has not read must not thereby delete the app's most important disclosure.
/// The second tap is taken by somebody with the sentence in front of them, on a
/// control that says what it does — and the note is not lost either, because it
/// stands at full length on the Settings Screen. That is what makes a permanent
/// dismissal an honest offer rather than a trap.
@MainActor
struct ForegroundOnlyNote: View {

    let onDismiss: () -> Void

    @State private var open = false

    var body: some View {
        VStack(spacing: 0) {
            // The whole band is the control, not the chip inside it. The band's
            // argument — that the fact is one tap from anywhere — rests on that
            // tap landing, and a 22pt chip in a 44pt band is half a target.
            Button {
                // Expanding is exploration; closing is acknowledgement. Only the
                // second of the two is remembered.
                if open { onDismiss() }
                open.toggle()
            } label: {
                ChromeBand(height: Fui.targetSmall, background: Fui.recess) {
                    Text(Glyphs.pinned)
                        .fuiText(Fui.micro, color: Fui.amber400)
                    Text(Strings.foregroundOnlyPinned)
                        .fuiText(Fui.micro, color: Fui.amber400)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .padding(.leading, 8)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    // The affordance, not the target. Bordered so it reads as
                    // pressable, and inert so this band holds one control rather
                    // than two of different sizes.
                    Text(open ? Strings.foregroundOnlyClose : "\(Strings.foregroundOnlyWhy)")
                        .fuiText(Fui.micro, color: Fui.textEmitter)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 3)
                        .overlay { Rectangle().strokeBorder(Fui.frame, lineWidth: 1) }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            // Named only while it is shut. The hint is a sentence about opening
            // the band, and reading it out over a tap that now closes the band
            // for good would promise the wrong thing; the open band leaves the
            // naming to the `▴ CLOSE` its own content already reads out.
            .accessibilityHint(open ? Text("") : Text(Strings.foregroundOnlyWhyAction))

            if open {
                VStack(alignment: .leading, spacing: 12) {
                    Text(Strings.foregroundOnlyNote)
                        .fuiText(Fui.prose, color: Fui.textBody)
                        .fixedSize(horizontal: false, vertical: true)
                    // Compose's `FlowRow` has no iOS 16 counterpart short of a
                    // custom `Layout`, and these four chips are long enough that
                    // a flow would put most of them on their own line anyway. A
                    // column is the layout a flow would arrive at; a hand-rolled
                    // `HStack` pair would clip on a 320pt screen, and `FuiTag`
                    // is `lineLimit(1)`, so a clipped chip is a fact nobody can
                    // read.
                    VStack(alignment: .leading, spacing: 6) {
                        FuiTag(text: Strings.foregroundOnlyTagSync)
                        FuiTag(text: Strings.foregroundOnlyTagNotification)
                        FuiTag(text: Strings.foregroundOnlyTagWatching)
                        FuiTag(text: Strings.foregroundOnlyTagCounterparty)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(Fui.gutter)
                .background(Fui.band)
                Hairline()
            }
        }
    }
}

// ── Pending ──────────────────────────────────────────────────────────────────

/// How many Entries are still waiting for the Relay.
///
/// Drawn only when there are some, and drawn at all because sync is
/// foreground-only: an Offer made with no connection sits in the queue until the
/// app is next opened, and a queue nobody can see is a queue nobody comes back
/// for. It goes of its own accord when the uploader drains it.
///
/// The count is a ``Fui/readout`` beside the sentence rather than a number
/// written into it, because the figure is what is being reported and the
/// sentence is what it means.
@MainActor
struct PendingBand: View {

    let count: Int64

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                Text(String(count))
                    .fuiText(Fui.readout, color: Fui.amber400)
                Text(Strings.pendingCount(count))
                    .fuiText(Fui.prose, color: Fui.textBody)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(.horizontal, Fui.gutter)
            .padding(.vertical, 10)
            .background(Fui.amberA16)
            Hairline(color: Fui.amberA40)
        }
    }
}
