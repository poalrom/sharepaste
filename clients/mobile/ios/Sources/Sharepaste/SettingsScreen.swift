import SharepasteCore
import SwiftUI

/// The phone's Settings: every Pairing it holds, and the little it can be told.
///
/// **Titled `SETTINGS`, with the Pairings as one section of five.** Android
/// retitled this screen in `0.5.0` and iOS inherits the retitle before it
/// inherits the problem that caused it: `PAIRINGS` was honest while a Pairing
/// was the only thing here, and the moment the screen grew a preference of its
/// own, a title naming one of its sections sent anyone looking for a switch to a
/// screen that does not exist. Android kept `Screen.Pairings` as its enum case
/// because renaming a symbol nobody reads is churn; ``Screen/settings`` had
/// nothing to keep and names the destination for what it is.
///
/// **Two distinctions, and collapsing them is the easy mistake.** Exactly one
/// Pairing is the **Active Pairing**: it is what this phone syncs and what an
/// Offer captures to, and the choice survives a restart. Any Pairing may be the
/// **Viewed Pairing**: that decides whose History is on screen, changes nothing
/// else, and is forgotten when the app is put down. When they diverge,
/// `DivergenceBand` says so on both screens — otherwise the History shows one
/// Pairing while the phone syncs another and nothing on screen admits it.
///
/// **The section order is a decision, not an accretion.** The cards, then adding
/// one, then ``ThisPhoneSection``, then ``StandingActionsSection``, then
/// ``AboutThisPhone``. It runs from what this phone is *paired to*, through what
/// it can be *told*, to what it *is* — each section answering a question raised
/// by the one above it. The two placements that carry an argument:
///
/// * The one live switch sits under a heading of its own rather than among the
///   inert `N/A` chips, because a switch three lines above `WATCHED CAPTURE ·
///   N/A` makes the chips read as switches somebody stopped wiring up, which is
///   the exact misreading the chips exist to prevent.
/// * Standing Actions is iOS's only addition to Android's four, and it sits
///   after the switch this phone *has* and before the notes about what this
///   phone *is* — a thing to go and build belongs with the controls, not with
///   the absences.
///
/// **No `NavigationStack` and no back gesture.** The `◂` is the whole of the way
/// back, which is spec row 29 and is argued at length on ``SharepasteRoot``.
@MainActor
struct SettingsScreen: View {

    let state: UiState
    let actions: AppActions

    var body: some View {
        VStack(spacing: 0) {
            TitleBand(
                title: Strings.settingsTitle,
                backDescription: Strings.settingsBack,
                onBack: actions.openHistory
            )
            if let notice = state.notice {
                NoticeBand(notice: notice, onDismiss: actions.dismissNotice)
            }
            if state.diverged {
                DivergenceBand(
                    viewedName: state.nameOf(state.viewedPairing),
                    activeName: state.nameOf(state.activeUserId),
                    onUseViewed: {
                        if let viewed = state.viewedPairing { actions.activatePairing(viewed) }
                    }
                )
            }
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 14) {
                    ForEach(state.pairings, id: \.userId) { pairing in
                        PairingCard(
                            pairing: pairing,
                            viewed: pairing.userId == state.viewedPairing,
                            foreground: state.foreground,
                            // The one card that may show a strip is the one the
                            // question is about. A `Confirmation` names its
                            // Pairing precisely so the wrong card cannot arm.
                            confirming: state.confirming?.userId == pairing.userId
                                ? state.confirming
                                : nil,
                            actions: actions
                        )
                    }
                    AddPairingSection(onAdd: actions.openAddPairing)
                    Hairline()
                    ThisPhoneSection(state: state, actions: actions)
                    Hairline()
                    StandingActionsSection()
                    Hairline()
                    AboutThisPhone()
                }
                .padding(Fui.gutter)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Fui.panel)
        .fuiBackdrop()
    }
}

// ── The Pairings ─────────────────────────────────────────────────────────────

/// One Pairing, and everything a phone can do to it.
///
/// The card is headed by the **User**, never by this phone's Device Label:
/// heading a Pairing with the local machine's name made every Pairing on the
/// desktop look like an account named after the computer. The Device Label is a
/// line *inside* the card, where it reads as what it is — what this phone told
/// the Relay to call itself.
///
/// **The address is the relay host and nothing else.** The `user_id` used to
/// lead it, and the argument for putting it back is that it is the only truly
/// unique thing here, since two Pairings can share a username. It stays out: the
/// subtitle is not the disambiguator, ``ConfirmStrip`` is, and that is where a
/// choice with no way back gets spelled out in full. On the card the uuid bought
/// nothing and cost the host, which ellipsised away behind it — `FuiPanel` gives
/// its address one line, so what shares that line decides what survives it.
///
/// A card that is not the Active one is *resting*, not faulty. `pairingPhase`
/// decides that and `toneOf` decides whether it is a fault, which is why no
/// status colour is chosen here — only the accent, and only from the answer.
@MainActor
private struct PairingCard: View {

    let pairing: PairingSummary
    let viewed: Bool
    let foreground: Bool
    let confirming: Confirmation?
    let actions: AppActions

    private var phase: SessionPhase { pairingPhase(pairing, foreground: foreground) }

    var body: some View {
        FuiPanel(
            title: pairing.username ?? pairing.userId,
            code: pairing.relayHost,
            accent: toneOf(phase) == .fault ? .alert : .emitter
        ) {
            VStack(alignment: .leading, spacing: 10) {
                // Exactly one card carries SYNCING; SHOWING moves independently
                // of it, because viewing a Pairing changes nothing about what the
                // phone syncs or captures to.
                if pairing.isActive || viewed {
                    HStack(spacing: 8) {
                        if pairing.isActive {
                            FuiBadge(text: Strings.pairingsActiveBadge, accent: .emitter, solid: true)
                        }
                        if viewed {
                            FuiBadge(text: Strings.pairingsViewedBadge, accent: .neutral)
                        }
                    }
                }
                // The Device Label, and nothing said about changing it. The one
                // rule worth stating is `Strings.pairLabelExplainer`'s, and it
                // is stated at the naming step because that is the only place it
                // is actionable — a person reading this card has already chosen.
                // Repeating it here would be the second statement rather than
                // the useful one, so the absence of that sentence is the
                // assertion.
                Text(Strings.pairingsThisPhone(pairing.label))
                    .fuiText(Fui.data, color: Fui.textBody)
                    .lineLimit(1)
                    .truncationMode(.tail)
                PairingStatus(phase: phase)
                // The one surface on this phone that shows a queue belonging to
                // a Pairing the device has switched away from. Nothing else
                // would: the History's own count is the Viewed Pairing's.
                if pairing.pending > 0 {
                    Text(Strings.pairingsPending(pairing.pending))
                        .fuiText(Fui.micro, color: Fui.amber400)
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(Fui.amberA16)
                }
                FlowRow(spacing: 8) {
                    if !viewed {
                        FuiButton(
                            text: Strings.pairingsView,
                            action: { actions.viewPairing(pairing.userId) },
                            height: Fui.targetSmall
                        )
                    }
                    if !pairing.isActive {
                        FuiButton(
                            text: Strings.pairingsUse,
                            action: { actions.activatePairing(pairing.userId) },
                            height: Fui.targetSmall
                        )
                    }
                    FuiButton(
                        text: Strings.pairingsClearHistory,
                        action: { actions.confirm(.clearHistory(userId: pairing.userId)) },
                        accent: .neutral,
                        height: Fui.targetSmall
                    )
                    FuiButton(
                        text: Strings.pairingsForget,
                        action: { actions.confirm(.forget(userId: pairing.userId)) },
                        accent: .alert,
                        height: Fui.targetSmall
                    )
                }
                if let confirming {
                    ConfirmStrip(confirming: confirming, pairing: pairing, actions: actions)
                }
                Hairline()
                // ADR 0002 asked for cipher disclosure beside pairing, at the
                // moment a Relay is being trusted. This card is not that moment
                // and it is the only place left: the pairing flow's footer band
                // carried `RELAY MUST BE HTTPS` beside the cipher, that second
                // line is inert on a phone that never chooses a scheme, and the
                // whole band went. ADR 0002 records the weaker placement as a
                // consequence of its own argument rather than a reversal of it,
                // so **do not answer the gap by rebuilding the footer** — answer
                // the inert second line first. This is the only cipher this
                // product names anywhere, on either client.
                Text(Strings.cipherDisclosure)
                    .fuiText(Fui.micro, color: Fui.textMuted)
            }
        }
    }
}

/// The yes-or-no strip for the two things that cannot be undone.
///
/// Inline and inside the card it is about, never a `confirmationDialog` and
/// never an `alert`: the scope of the erase stays on screen while the choice is
/// being made, and a sheet that covers the card covers the badges saying whether
/// this is the Pairing the phone syncs.
///
/// It names the **User and the Relay** rather than reusing the card's heading.
/// Two Pairings can share a username, and this is the one action with no way
/// back — so this is the one place the address is spelled out in full, which is
/// the other half of ``PairingCard``'s argument for keeping the uuid off the
/// heading.
///
/// **Forgetting the Active Pairing is answered by the core, and this copy says
/// nothing about it on purpose.** `SharepasteViewModel.forgetPairing` reads
/// `activePairing()` back after the erase and reports whichever Pairing was
/// promoted — or that nothing is paired now — through a Notice. A sentence here
/// promising which one takes over would be this shell guessing at a rule the
/// core owns, and wrong the first time that rule changed.
///
/// `KEEP IT` is the outline and the destructive verb is the solid one, which is
/// the opposite of the usual advice and is right here: the person has already
/// asked for this, the strip exists to make them read what it costs, and burying
/// the verb they came for behind the safe-looking button would just be answered
/// twice.
@MainActor
private struct ConfirmStrip: View {

    let confirming: Confirmation
    let pairing: PairingSummary
    let actions: AppActions

    private var target: String { "\(pairing.username ?? pairing.userId) @ \(pairing.relayHost)" }

    private var question: String {
        switch confirming {
        case .clearHistory: Strings.pairingsClearConfirm(target)
        case .forget: Strings.pairingsForgetConfirm(target)
        }
    }

    private var verb: String {
        switch confirming {
        case .clearHistory: Strings.pairingsClearHistory
        case .forget: Strings.pairingsForget
        }
    }

    private func erase() {
        switch confirming {
        case let .clearHistory(userId): actions.clearHistory(userId)
        case let .forget(userId): actions.forgetPairing(userId)
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            FuiBadge(text: Strings.pairingsConfirmBadge, accent: .alert, solid: true)
            Text(question)
                .fuiText(Fui.prose, color: Fui.textPrimary)
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 8) {
                FuiButton(
                    text: Strings.pairingsCancel,
                    action: { actions.confirm(nil) },
                    height: Fui.targetSmall
                )
                FuiButton(
                    text: verb,
                    action: erase,
                    accent: .alert,
                    solid: true,
                    height: Fui.targetSmall
                )
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Fui.alertA16)
    }
}

@MainActor
private struct AddPairingSection: View {

    let onAdd: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionHeading(Strings.pairingsAddHeading)
            Text(Strings.pairingsAddBody)
                .fuiText(Fui.prose, color: Fui.textBody)
                .fixedSize(horizontal: false, vertical: true)
            FuiButton(text: Strings.pairingsAddButton, action: onAdd)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

// ── What this phone can be told ──────────────────────────────────────────────

/// The one thing this phone can actually be told.
///
/// A Recall reaches the pasteboard whichever way the switch is set; all it
/// decides is whether the Receipt names what arrived. That earns a control
/// because the Receipt is the only part of a Recall legible to whoever is
/// standing next to you — and it earns *only* this control. Off means no Recall
/// Receipt at all rather than a redacted one (ADR 0009): silencing the Recall
/// Receipt while the Offer's still speaks is the whole feature, so this is not a
/// quiet mode and is not worded as one.
///
/// **It holds that switch alone.** A live switch three lines above `WATCHED
/// CAPTURE · N/A` makes the three inert chips read as switches somebody stopped
/// wiring up, which is the exact misreading the chips exist to prevent — so the
/// switch gets a heading about what this phone can be *told* and the chips get
/// one about what this phone *is*. Anything new that can be toggled belongs
/// here; nothing inert ever does.
@MainActor
private struct ThisPhoneSection: View {

    let state: UiState
    let actions: AppActions

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            SectionHeading(Strings.settingsThisPhoneHeading)
            Toggle(
                Strings.settingsShowRecalled,
                // A closure literal and not `set: actions.setShowRecalled`.
                // `Binding.init(get:set:)` takes `@isolated(any) @Sendable`
                // closures; a literal formed here picks up this view's
                // `@MainActor` isolation, while a bare reference to the stored
                // closure is a plain non-Sendable function value and converting
                // it is a data-race warning.
                isOn: Binding(get: { state.showRecalled }, set: { actions.setShowRecalled($0) })
            )
            .toggleStyle(FuiSwitchStyle())
            // Not a `QuotedNote`: the rule down its left edge would cut the
            // sentence away from the switch it explains, and two short sentences
            // do not need an idiom built for prose a box would make look like an
            // alert. `ADD ANOTHER PAIRING` states its own body exactly this way.
            Text(Strings.settingsShowRecalledNote)
                .fuiText(Fui.prose, color: Fui.textBody)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A preference, as a row: the word on the left, its state on the right.
///
/// **A `ToggleStyle` rather than a hand-rolled row, and that is an accessibility
/// decision rather than a stylistic one.** Android drew its own shape and kept
/// the platform's meaning with `Modifier.toggleable(role = Role.Switch)`; the
/// SwiftUI construct that separates those two things the same way is a custom
/// style on a real `Toggle`. VoiceOver then says "on"/"off" and offers the
/// toggle action, which is the entire reason this is not a `FuiButton` that
/// flips a flag. `AccessibilityTraits.isToggle` would be the shorter route and
/// is iOS 17, above this app's floor.
///
/// Nothing of Apple's shape survives. A filled capsule with a circular thumb is
/// the one form in this palette that would announce it came from somewhere else:
/// the vocabulary here is square-cornered borders over the void, so the track is
/// a rectangle and the thumb is a square, for the same reason `StatusLight`'s
/// lamp is — a HUD does not draw circles, and a small square survives a
/// low-density screen that eats a small circle's edges.
///
/// **The whole row is the target, never the track.** A 40pt track is under the
/// 48pt floor on its own, and the label is what a thumb aims at anyway.
///
/// Off drops the label to prose ink and never to ``Fui/textDim``, which on this
/// screen means inert: a preference that is switched off is still a preference,
/// and the three chips further down are the only thing here entitled to look
/// disabled.
private struct FuiSwitchStyle: ToggleStyle {

    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 12) {
            configuration.label
                .fuiText(Fui.label, color: configuration.isOn ? Fui.textEmitter : Fui.textBody)
                .frame(maxWidth: .infinity, alignment: .leading)
            ZStack(alignment: configuration.isOn ? .trailing : .leading) {
                Rectangle()
                    .fill(configuration.isOn ? Fui.cyanA12 : Color.clear)
                Rectangle()
                    .fill(configuration.isOn ? Fui.cyan400 : Fui.inert)
                    .frame(width: 14, height: 14)
                    .padding(3)
            }
            .frame(width: 40, height: 22)
            .overlay {
                Rectangle().strokeBorder(configuration.isOn ? Fui.frame : Fui.inert, lineWidth: 1)
            }
        }
        .frame(minHeight: Fui.target)
        // The row's whole rectangle, gaps included — without this the tappable
        // area is the label's glyphs and the track, with dead space between.
        .contentShape(Rectangle())
        .onTapGesture { configuration.isOn.toggle() }
    }
}

/// How the two Standing Actions get wired up, which on this phone is Shortcuts.
///
/// iOS's only addition to Android's four sections (spec row 23). Neither action
/// touches the pasteboard itself — Shortcuts does, which is what makes "the app
/// never reads or writes a clipboard unasked" a property of the design rather
/// than a promise. That is also why the actions do nothing until somebody builds
/// a shortcut around them, and why this section exists at all: a building block
/// nobody knows about is a building block nobody uses.
///
/// **Deliberately not a chrome callout, anywhere.** Android has
/// `StandingActionsBlockedNote` pinned above its History, and it is not the same
/// thing: that warns about a state that went *wrong* — notifications switched
/// off, so a Standing Action has no way to report. "You have not written a
/// shortcut yet" is the normal condition of a fresh install and of every install
/// belonging to somebody who does not want one. A permanent band for it would
/// nag rather than report, and chrome that nags is chrome people learn to stop
/// reading — including on the day it says something that matters.
@MainActor
private struct StandingActionsSection: View {

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionHeading(Strings.shortcutsHeading)
            Text(Strings.shortcutsBody)
                .fuiText(Fui.prose, color: Fui.textBody)
                .fixedSize(horizontal: false, vertical: true)
            recipe(Strings.shortcutsOfferRecipe, Strings.shortcutsOfferNote)
            recipe(Strings.shortcutsRecallRecipe, Strings.shortcutsRecallNote)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// One shortcut to build, and what building it gets you.
    ///
    /// The recipe is drawn in the control register and in the emitter's ink
    /// because it is a thing to go and assemble, but it is not a `FuiButton` and
    /// not a `FuiTag`: pressing it here could not build anything, and a chip
    /// would file it with the three absences two sections down. It wraps rather
    /// than truncating — a recipe with its second half ellipsised is a recipe
    /// nobody can follow.
    private func recipe(_ recipe: String, _ note: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(recipe)
                .fuiText(Fui.label, color: Fui.textEmitter)
                .fixedSize(horizontal: false, vertical: true)
            Text(note)
                .fuiText(Fui.prose, color: Fui.textBody)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// What this phone is, and why the computer's two switches are not here.
///
/// Stated rather than left as a gap. Someone who knows the desktop will come
/// looking for the capture switch and the deny-list, and finding nothing is
/// indistinguishable from finding a half-built screen. Both are inert on a
/// phone — one governs Watched Capture, which a phone never performs, and the
/// other matches a frontmost application, which a phone has no notion of — and
/// saying so takes two sentences and three chips. The third chip is the one the
/// desktop cannot show: this app carries no update code at all (ADR 0008), so it
/// never asks an update source anything and the Relay is its only counterparty.
///
/// **The foreground-only rule is stated here at full length and permanently.**
/// The History Screen's band says the same thing, but its `▴ CLOSE` retires that
/// band for good — so the fact needs exactly one surface where it cannot be
/// dismissed, and it has to be the section describing what this phone *is*. It
/// therefore ignores `state.foregroundNoteDismissed` entirely, which is why this
/// view takes no state at all: there is no argument it could be handed that
/// would make it right to hide this.
///
/// It reads above `settingsAbsentNote` because that note is the caption for the
/// three chips directly under it, and anything wedged between the two leaves
/// three chips heading nothing.
@MainActor
private struct AboutThisPhone: View {

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionHeading(Strings.settingsHeading)
            QuotedNote(text: Strings.foregroundOnlyNote)
            QuotedNote(text: Strings.settingsAbsentNote)
            FlowRow(spacing: 6) {
                FuiTag(text: Strings.settingsTagWatchedCapture, inert: true)
                FuiTag(text: Strings.settingsTagDenyList, inert: true)
                FuiTag(text: Strings.settingsTagUpdateCheck, inert: true)
            }
            .padding(.top, 2)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

// ── Layout ───────────────────────────────────────────────────────────────────

/// Compose's `FlowRow`, as a `Layout`: children in a line, wrapping when they
/// run out of width.
///
/// Two places here need it and both wrap on a real phone — a card's four verbs
/// (`SHOW ITS ENTRIES`, `SYNC THIS ONE`, `CLEAR HISTORY`, `FORGET`) and the three
/// inert chips. The alternatives were both wrong in the same way: a fixed
/// `LazyVGrid` strands `FORGET` alone on its own row whenever a card is already
/// the Active or the Viewed one and drops a verb, and `ViewThatFits` between an
/// `HStack` and a `VStack` gives one line or four and never the two the buttons
/// actually want.
///
/// Private, because exactly one screen draws it. If a second one ever does, it
/// moves to `Readouts.swift` whole rather than being copied — that is the drift
/// ADR 0010 is about, one layer down.
///
/// One `spacing` for both axes rather than Compose's two, because both call
/// sites here pass the same number twice and a parameter with one possible shape
/// is a parameter that will be got wrong.
private struct FlowRow: Layout {

    let spacing: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout Void) -> CGSize {
        // An unspecified width means "how big would you like to be", and the
        // honest answer for a wrapping row is one line — the container then
        // proposes a real width and asks again.
        let limit = proposal.width ?? .infinity
        var cursor = CGSize.zero
        var lineHeight: CGFloat = 0
        var widest: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if cursor.width > 0, cursor.width + size.width > limit {
                cursor.width = 0
                cursor.height += lineHeight + spacing
                lineHeight = 0
            }
            cursor.width += size.width + spacing
            widest = max(widest, cursor.width - spacing)
            lineHeight = max(lineHeight, size.height)
        }
        return CGSize(width: min(widest, limit), height: cursor.height + lineHeight)
    }

    func placeSubviews(
        in bounds: CGRect,
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout Void
    ) {
        var x = bounds.minX
        var y = bounds.minY
        var lineHeight: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x > bounds.minX, x + size.width > bounds.maxX {
                x = bounds.minX
                y += lineHeight + spacing
                lineHeight = 0
            }
            subview.place(at: CGPoint(x: x, y: y), anchor: .topLeading, proposal: ProposedViewSize(size))
            x += size.width + spacing
            lineHeight = max(lineHeight, size.height)
        }
    }
}

// ── Previews ─────────────────────────────────────────────────────────────────

#if DEBUG

/// A Pairing, as the facade would have listed it.
///
/// Free functions rather than fixtures on ``UiState``: a preview that built its
/// state through the real state holder would need a facade, and one that carried
/// its own copy of the defaults would drift from them silently.
private func previewPairing(
    user: String,
    name: String?,
    host: String,
    status: ConnectionState = .online,
    pending: Int64 = 0,
    active: Bool = false
) -> PairingSummary {
    PairingSummary(
        userId: user,
        deviceId: "device-\(user)",
        label: "iPhone in my pocket",
        username: name,
        serverUrl: "https://\(host)",
        relayHost: host,
        status: status,
        pending: pending,
        isActive: active
    )
}

private func previewState(
    pairings: [PairingSummary],
    active: String?,
    viewed: String? = nil,
    confirming: Confirmation? = nil
) -> UiState {
    var state = UiState()
    state.screen = .settings
    state.foreground = true
    state.pairings = pairings
    state.activeUserId = active
    state.viewedUserId = viewed
    state.confirming = confirming
    return state
}

/// **`PreviewProvider` rather than the `#Preview` macro, and it is the build
/// path that decides that.** The macro's implementation ships as the
/// `PreviewsMacros` plugin inside Xcode's toolchain; this package is built by
/// SwiftPM under WSL against an Apple Swift SDK (spec rows 1 and 3), where that
/// plugin does not exist and every `#Preview` is a hard compile error rather
/// than a preview that merely does not render. `PreviewProvider` is also the
/// only one of the two that works below iOS 17, which is this app's floor.
@MainActor
private let previewActions = AppActions(
    setDeviceLabel: { _ in }, setPairingCode: { _ in }, codeScanned: { _ in },
    pairWithCode: {}, setCameraProblem: { _ in }, dismissPairFailure: {},
    offerPasteboard: {}, recallLatest: {}, recall: { _ in }, deleteEntry: { _ in },
    dismissNotice: {}, openSettings: {}, openHistory: {}, openAddPairing: {},
    viewPairing: { _ in }, activatePairing: { _ in }, confirm: { _ in },
    clearHistory: { _ in }, forgetPairing: { _ in }, setShowRecalled: { _ in },
    dismissForegroundNote: {}
)

private let onePairing = [previewPairing(user: "u-1", name: "ada", host: "relay.example.com", active: true)]

/// Three Pairings, one Active, and a queue on a Pairing that is neither Active
/// nor Viewed — the only surface in the app that shows one.
private let threePairings = [
    previewPairing(user: "u-1", name: "ada", host: "relay.example.com", active: true),
    previewPairing(user: "u-2", name: "grace", host: "paste.othernet.dev", status: .disconnected, pending: 3),
    previewPairing(user: "u-3", name: nil, host: "relay.local", status: .disconnected),
]

/// The floor: one Pairing, syncing and showing, nothing to decide.
struct SettingsScreen_OnePairing_Previews: PreviewProvider {
    static var previews: some View {
        SettingsScreen(
            state: previewState(pairings: onePairing, active: "u-1"),
            actions: previewActions
        )
        .previewDisplayName("One pairing")
    }
}

/// Three Pairings with the Viewed one diverged from the Active one — the case
/// the whole Active/Viewed distinction exists for, and the one a card layout
/// gets wrong by putting SYNCING and SHOWING on the same card.
struct SettingsScreen_Diverged_Previews: PreviewProvider {
    static var previews: some View {
        SettingsScreen(
            state: previewState(pairings: threePairings, active: "u-1", viewed: "u-2"),
            actions: previewActions
        )
        .previewDisplayName("Three, diverged")
    }
}

/// A card mid-confirmation for the erasure that keeps the Pairing.
struct SettingsScreen_Clearing_Previews: PreviewProvider {
    static var previews: some View {
        SettingsScreen(
            state: previewState(
                pairings: threePairings,
                active: "u-1",
                confirming: .clearHistory(userId: "u-2")
            ),
            actions: previewActions
        )
        .previewDisplayName("Clearing a History")
    }
}

/// A card mid-confirmation for the erasure that does not — and it is the Active
/// Pairing, which is the case the copy must not make a promise about.
struct SettingsScreen_Forgetting_Previews: PreviewProvider {
    static var previews: some View {
        SettingsScreen(
            state: previewState(
                pairings: threePairings,
                active: "u-1",
                confirming: .forget(userId: "u-1")
            ),
            actions: previewActions
        )
        .previewDisplayName("Forgetting the Active Pairing")
    }
}

/// The one card that is a fault. Everything else on this screen is nominal by
/// `toneOf`, three Pairings sitting idle included, so this is the only preview
/// in which anything is allowed to be red.
struct SettingsScreen_Refused_Previews: PreviewProvider {
    static var previews: some View {
        SettingsScreen(
            state: previewState(
                pairings: [
                    previewPairing(user: "u-1", name: "ada", host: "relay.example.com", active: true),
                    previewPairing(user: "u-2", name: "grace", host: "paste.othernet.dev", status: .authFailed),
                ],
                active: "u-1"
            ),
            actions: previewActions
        )
        .previewDisplayName("A refused pairing")
    }
}

#endif
