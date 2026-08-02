import SwiftUI

/// Pairing this phone to a User that already exists.
///
/// Two ways of filling one field, and then one button. Scanning the square on
/// the computer's pairing pane is the practical one; typing the code printed
/// underneath it is the fallback, and it is load-bearing rather than decorative
/// — it is the only way in when the camera is refused or absent, which is why
/// each of those has its own message rather than a shared shrug, and why the
/// field is on screen either way rather than behind a camera failure.
///
/// **A scan fills that field. It does not pair.** Somebody who opens this screen
/// points it at the square before reading a word, because the square is the only
/// thing here that looks like an instruction — and the name the Pairing has to
/// carry comes after. So the viewfinder hands its code down to the field and
/// stands down, which leaves one thing left to do and a button that says what it
/// is. The rule is ``SharepasteViewModel/codeScanned(_:)``'s, and this screen
/// adds no second path to it: the only thing wired to
/// ``AppActions/pairWithCode`` is the control that says `PAIR THIS PHONE`.
///
/// **Emptying the field re-arms the viewfinder**, and that is the only way to
/// read a second code. A control for it would sit beside the field it
/// duplicates. ``SharepasteViewModel/setPairingCode(_:)`` is where that happens;
/// here it is simply the reason there is no `SCAN AGAIN`.
///
/// **The name is the person's, and pairing waits for it.** The field starts
/// empty and the button is dead until it is not. The desktop's flow hard-codes a
/// default; a machine's guess at what someone calls their own phone is not a
/// default, it is a thing they have to notice and correct in a list they read
/// later.
///
/// The screen renders from a ``PairingState`` and nothing else, which is what
/// makes the five previews at the bottom of this file worth having: a screen
/// that bound a camera as a side effect of being drawn could not be asserted
/// about without one, and the case where the wording matters most is precisely
/// the case where there is no camera to bind. The permission itself is watched
/// by ``CameraAccess``, held at this screen's root rather than inside the branch
/// it steers — see that type for the bug that shape exists to prevent.
@MainActor
struct PairingScreen: View {

    let state: PairingState
    let actions: AppActions

    /// The way back, on a phone that already holds a Pairing.
    ///
    /// `nil` on a fresh install, where this screen is the entire app and a back
    /// control would lead nowhere. The shell decides that (`SharepasteApp.swift`)
    /// and this screen only draws it — and drawing it is the whole of the way
    /// out, because spec row 29 declines a `NavigationStack` and with it the
    /// left-edge swipe. The `◂` is not a shortcut for a gesture; it is the door.
    let onBack: (() -> Void)?

    /// Owned here, at the screen's root, and deliberately not inside the branch
    /// that renders the refusal. ``CameraAccess`` says at length why.
    @StateObject private var access = CameraAccess()

    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        VStack(spacing: 0) {
            TitleBand(title: Strings.pairTitle, backDescription: Strings.settingsBack, onBack: onBack)

            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    // 01 — the name, first, because the code expires and the
                    // name does not.
                    Step(
                        step: Strings.pairStepName,
                        heading: Strings.pairLabelHeading,
                        explainer: Strings.pairLabelExplainer
                    ) {
                        FuiField(
                            label: Strings.pairLabelField,
                            text: state.deviceLabel,
                            onChange: actions.setDeviceLabel,
                            placeholder: Strings.pairLabelPlaceholder
                        )
                    }

                    Hairline()

                    // 02 — the camera, or the reason there isn't one.
                    Step(
                        step: Strings.pairStepScan,
                        heading: Strings.pairScanHeading,
                        explainer: Strings.pairScanExplainer
                    ) {
                        scanner
                    }

                    Hairline()

                    Step(
                        step: nil,
                        heading: Strings.pairTypedHeading,
                        explainer: Strings.pairTypedExplainer
                    ) {
                        FuiField(
                            label: Strings.pairTypedField,
                            text: state.code,
                            onChange: actions.setPairingCode,
                            // A pairing code is not a word. Autocorrect would
                            // rewrite it into one, and the capital iOS puts on
                            // the first character is noise the core strips
                            // anyway — `Strings.pairTypedExplainer` promises
                            // case does not matter and the core's `decode`
                            // keeps that promise.
                            autocorrect: false,
                            submit: actions.pairWithCode
                        )
                        FuiButton(
                            text: state.attempt == .working ? Strings.pairWorking : Strings.pairButton,
                            action: actions.pairWithCode,
                            solid: true,
                            enabled: state.canPair,
                            fillsWidth: true
                        )
                    }

                    if case let .failed(sentence, detail) = state.attempt {
                        Failure(sentence: sentence, detail: detail, onDismiss: actions.dismissPairFailure)
                    }

                    // **No `XCHACHA20-POLY1305 / RELAY MUST BE HTTPS` footer band
                    // here, and it is not an omission.** Android deleted its own
                    // in `0.5.0`. `RELAY MUST BE HTTPS` is inert on a phone,
                    // which never chooses a scheme — the scheme arrives inside
                    // the code, and a relay served over cleartext is refused
                    // with `Strings.pairInsecureRelay` above rather than warned
                    // about here. The cipher line survives on the Settings
                    // card, `Strings.cipherDisclosure`, which ADR 0002 records
                    // as a weakening of what it asked for and accepts.
                    // iOS having room for the band is not a reason to rebuild
                    // it.
                }
                .padding(.horizontal, Fui.gutter)
                .padding(.vertical, 18)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Fui.panel)
        .fuiBackdrop()
        // Once per appearance, carrying this render's action bag. The holder
        // keeps it, so every later reading reports to the same place.
        .task { await access.start(report: actions.setCameraProblem) }
        // The route that has to work: every way of granting a permission that is
        // not the dialog ends with this app coming back to the front. Android
        // watches `ON_RESUME` for the same reason and adds a one-second poll;
        // see ``CameraAccess`` for why the poll has nothing to watch here.
        .onChange(of: scenePhase) { phase in
            if phase == .active { access.refresh() }
        }
    }

    /// The three things that can be in the viewfinder's place, and the
    /// viewfinder.
    ///
    /// Two camera failures and one success, kept apart because they need
    /// different things done about them: a refused permission has something to
    /// turn on, absent hardware has not, and a code already read has nothing
    /// wrong with it at all. `cameraProblem(hasCamera:permissionGranted:)`
    /// decides which failure applies and this is only where each one speaks.
    @ViewBuilder
    private var scanner: some View {
        switch state.camera {
        // Muted for the hardware, because there is nothing to act on; caution
        // for the permission, because there is.
        case .noCamera:
            ViewfinderNote(sentence: Strings.cameraAbsent, mark: Fui.textMuted)

        case .permissionRefused:
            ViewfinderNote(sentence: Strings.cameraPermissionRefused, mark: Fui.amber400) {
                HStack(spacing: 8) {
                    FuiButton(
                        text: Strings.cameraRecheck,
                        action: { access.recheck() },
                        height: Fui.targetSmall
                    )
                    // Android offers no such button: it has no deep link that
                    // reliably lands on this app's own page. iOS does, and the
                    // sentence above already names Settings as the place to go,
                    // so a control that goes there is one tap instead of four.
                    FuiButton(
                        text: Strings.cameraOpenSettings,
                        action: { access.openSettings() },
                        height: Fui.targetSmall
                    )
                }
            }

        // A code already read is the viewfinder's whole job done. The note is
        // what tells a camera that has stood down apart from a camera that has
        // failed, and says where the code went.
        case nil:
            if state.scanned {
                ViewfinderNote(
                    sentence: Strings.pairCodeScanned,
                    mark: Fui.cyan400,
                    glyph: Glyphs.done
                )
            } else {
                Viewfinder(onCode: actions.codeScanned)
            }
        }
    }
}

/// One numbered step, or an unnumbered one.
///
/// The number is drawn beside the heading rather than written into the string,
/// so a step that moves does not need its words re-edited — and so the numeral
/// can carry the emitter colour while the heading stays a heading.
///
/// The explainer is plain prose and **not** a ``QuotedNote``, matching Android.
/// The rule down a quoted note's left edge is for prose long enough that a box
/// would read as an alert; after the `0.5.0` compression to roughly 40% these
/// are one sentence each, and a rule beside one sentence cuts it away from the
/// field it introduces.
@MainActor
private struct Step<Content: View>: View {

    let step: String?
    let heading: String
    let explainer: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                if let step {
                    Text(step).fuiText(Fui.label, color: Fui.cyan400)
                }
                Text(heading).fuiText(Fui.subheading, color: Fui.textPrimary)
            }
            Text(explainer).fuiText(Fui.prose, color: Fui.textBody)
            content
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// The camera, framed and captioned.
///
/// The caption sits inside the frame, over the preview it describes, so the
/// panel is the viewfinder and nothing else.
///
/// **How long a code lives is not stated here, and there is no
/// `CODES EXPIRE AFTER 02:00` strip.** Android deleted its own in `0.5.0` and
/// iOS never builds it: this phone is the claimer and reads a shortcode carrying
/// no timestamp, so a strip could only ever assert the rule and never count it
/// down — and the fact is already delivered in the one place it is actionable,
/// in ``Strings/pairCodeExpired``, which is what a code that arrived too late
/// says for itself. Do not add it back because there is room.
@MainActor
private struct Viewfinder: View {

    let onCode: (String) -> Void

    var body: some View {
        FuiPanel(title: Strings.pairViewfinderTitle, code: Strings.pairViewfinderCode) {
            ZStack {
                Fui.void1000
                CameraScanner(onCode: onCode)
                Text(Strings.pairViewfinderHint)
                    .fuiText(Fui.micro, color: Fui.textMuted)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 24)
            }
            // The sensor's shape, not a height in points: a ratio against the
            // width lands the same frame on every phone, and it is the box the
            // preview is cropped *into* — so it has to be a shape somebody can
            // aim a square at. The clip is the second half of the scanner's
            // `resizeAspectFill`, which by design scales its content past the
            // bounds it was given.
            .frame(maxWidth: .infinity)
            .aspectRatio(4.0 / 3.0, contentMode: .fit)
            .clipped()
        }
    }
}

/// A note in the viewfinder's place, in a slot that is dashed rather than
/// framed.
///
/// Dashed because the viewfinder is *absent* rather than broken — the field it
/// feeds is already on screen underneath and works just as well, so a solid
/// alert frame would overstate every one of the three things this says. Two of
/// them are camera failures and the third is a scan that succeeded, which is why
/// the glyph and its colour are the caller's to choose.
@MainActor
private struct ViewfinderNote<Action: View>: View {

    let sentence: String
    let mark: Color
    var glyph: String = Glyphs.absent
    @ViewBuilder let action: Action

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Text(glyph)
                .fuiText(Fui.glyph, color: mark)
                // A picture of the state, beside the sentence that already says
                // it. Read out, it would be "circled slash" before the words.
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 10) {
                Text(sentence).fuiText(Fui.prose, color: Fui.textBody)
                action
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .dashedBorder(Fui.inert)
    }
}

extension ViewfinderNote where Action == EmptyView {

    /// A note with nothing to press.
    ///
    /// A separate initialiser rather than a defaulted `action:`, for the reason
    /// ``FuiPanel`` gives: a generic parameter cannot be defaulted, and the
    /// alternative is every plain note in this file writing
    /// `action: { EmptyView() }`.
    init(sentence: String, mark: Color, glyph: String = Glyphs.absent) {
        self.init(sentence: sentence, mark: mark, glyph: glyph, action: { EmptyView() })
    }
}

/// A failed attempt, in the words that fit it.
///
/// ``PairAttempt/failed(message:detail:)``'s detail is only ever set for a
/// cleartext Relay, where the core's own sentence names the address and the
/// reason. Showing it *under* the app's wording rather than instead of it keeps
/// the specific fact without handing over a protocol error as an explanation.
///
/// **This is the third of the three failures this flow has to tell apart**, and
/// it is a band rather than a note in the viewfinder's place because it is a
/// fact about an *attempt* and not about the camera. A code that arrived after
/// its slot expired says ``Strings/pairCodeExpired`` here, which is the one
/// place the two-minute life is worth stating — it is being acted on. The other
/// two are ``Strings/cameraPermissionRefused`` and ``Strings/cameraAbsent``,
/// each in its own branch above, and the typed field goes on working under all
/// three.
///
/// `TRY AGAIN` is ``AppActions/dismissPairFailure``, which is the whole of
/// ``PairingState/restarted()`` — the spent code, the scan latch and the failure
/// go, the Device Label stays. Nothing here may be wired to something that
/// clears one field: Android shipped that version and it re-sent a dead code at
/// a viewfinder that had already stood itself down.
@MainActor
private struct Failure: View {

    let sentence: String
    let detail: String?
    let onDismiss: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            FuiBadge(text: Strings.pairFailedBadge, accent: .alert, solid: true)
            Text(sentence).fuiText(Fui.prose, color: Fui.textPrimary)
            if let detail {
                Text(detail).fuiText(Fui.data, color: Fui.textBody)
            }
            FuiButton(
                text: Strings.pairDismiss,
                action: onDismiss,
                accent: .alert,
                height: Fui.targetSmall
            )
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(Fui.alertA16)
    }
}

/// A text field in the console's own voice.
///
/// SwiftUI's `TextField` underneath, because a hand-rolled one would owe the
/// platform a cursor, a selection loupe, an IME contract and an accessibility
/// tree. Only its colours, its frame and its type are ours.
///
/// **The value comes in as a `String` and goes out as a call**, rather than as a
/// `@Binding` onto view state. There is one snapshot on this screen and it is
/// the state holder's; a field with `@State` of its own would be a second copy
/// of the pairing code that could disagree with the first, and the disagreement
/// would show up as a scan that filled a field the button could not see.
@MainActor
private struct FuiField: View {

    let label: String
    let text: String
    let onChange: (String) -> Void
    var placeholder: String?
    var autocorrect = true
    /// The keyboard's own action key. Given one, the field says `Go` and fires
    /// it — the code field is the last thing filled in and reaching past the
    /// keyboard for the button below it is a reach the platform already solved.
    var submit: (() -> Void)?

    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label).fuiText(Fui.micro, color: focused ? Fui.textEmitter : Fui.textMuted)
            // `set:` takes a closure literal rather than `onChange` itself:
            // SwiftUI declares it `@isolated(any) @Sendable`, and handing over
            // a stored function value converts an isolation the compiler cannot
            // see. The literal is formed here, on the main actor, and carries
            // that isolation with it.
            TextField("", text: Binding(get: { text }, set: { onChange($0) }))
                .textFieldStyle(.plain)
                .focused($focused)
                .fuiText(Fui.data, color: focused ? Fui.textPrimary : Fui.textBody)
                // The caret, which is the one piece of chrome SwiftUI draws
                // inside the field and the one that is not covered by a colour
                // above.
                .tint(Fui.cyan400)
                .autocorrectionDisabled(!autocorrect)
                .textInputAutocapitalization(autocorrect ? .sentences : .never)
                .submitLabel(submit == nil ? .return : .go)
                .onSubmit { submit?() }
                .padding(.horizontal, 10)
                .frame(height: Fui.target)
                .overlay(alignment: .leading) {
                    if text.isEmpty, let placeholder {
                        Text(placeholder)
                            .fuiText(Fui.data, color: Fui.textDim)
                            .padding(.horizontal, 10)
                            // Drawn rather than handed to `prompt:`, which
                            // paints the platform's own grey — a colour on
                            // nobody's ramp, in the one slot where the
                            // difference between dim and body text is what says
                            // "this is an example, not your answer".
                            .allowsHitTesting(false)
                    }
                }
                .background(Fui.recess)
                .overlay {
                    Rectangle().strokeBorder(focused ? Fui.cyan400 : Fui.frame, lineWidth: 1)
                }
        }
    }
}

// ── Previews ─────────────────────────────────────────────────────────────────

#if DEBUG

/// The five states of this screen, side by side.
///
/// `PreviewProvider` and **not** `#Preview`, which does not compile here: the
/// macro's implementation ships as a plugin inside Xcode, and spec row 1 builds
/// this package with the open-source toolchain against an iOS SDK sysroot,
/// where that plugin does not exist — `plugin for module 'PreviewsMacros' not
/// found`. `PreviewProvider` is the iOS 13–16 form, it needs no macro, and it
/// carries no deprecation.
///
/// These are not decoration. Spec row 10 buys no XCUITest and no snapshot
/// tests, so looking at the screen is the whole of the UI defence — and three
/// of the five states below cannot be reached on a simulator or on a healthy
/// phone at all. A refused permission needs somebody to refuse it; absent
/// hardware needs a device that has none; a spent code needs one held past its
/// two minutes. The previews are how those three get read without any of that,
/// and they are not a substitute for the device pass the ticket asks for: what
/// they show is the wording and the layout, never that the camera works.
struct PairingScreen_Previews: PreviewProvider {

    static var previews: some View {
        Group {
            // A fresh install: no Pairing yet, so no way back, and the
            // viewfinder is live. On anything without a camera the frame draws
            // its hint over the void, which is the same thing this preview
            // shows.
            PairingScreen(state: PairingState(), actions: .inert, onBack: nil)
                .previewDisplayName("01 fresh")

            // A code read. The viewfinder has stood down, the field holds what
            // it read, and the only thing left is the name — which is what the
            // note says and why the button is still dead.
            PairingScreen(
                state: PairingState(code: "K7QP2M4X", scanned: true),
                actions: .inert,
                onBack: nil
            )
            .previewDisplayName("02 scanned")

            // Refused. The typed path is on screen underneath, untouched, which
            // is the whole reason the slot is dashed rather than framed.
            PairingScreen(
                state: PairingState(deviceLabel: "iPhone in my pocket", camera: .permissionRefused),
                actions: .inert,
                onBack: {}
            )
            .previewDisplayName("03 permission refused")

            // No camera at all. A different sentence, because "turn camera
            // access on" is useless advice here.
            PairingScreen(
                state: PairingState(camera: .noCamera),
                actions: .inert,
                onBack: nil
            )
            .previewDisplayName("04 no camera")

            // A failed attempt, and specifically the one failure that shows the
            // core's own words underneath the app's: a Relay served over plain
            // http://, refused before a byte left the device.
            PairingScreen(
                state: PairingState(
                    deviceLabel: "iPhone in my pocket",
                    code: "K7QP2M4X",
                    attempt: .failed(
                        sentence: Strings.pairInsecureRelay,
                        detail: "relay http://relay.example.net is not https"
                    )
                ),
                actions: .inert,
                onBack: {}
            )
            .previewDisplayName("05 did not pair")
        }
        .preferredColorScheme(.dark)
    }
}

#endif
