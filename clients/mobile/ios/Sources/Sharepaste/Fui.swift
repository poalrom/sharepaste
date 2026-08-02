import SwiftUI

/// The FUI/HUD language, as SwiftUI values.
///
/// The third hand-written copy of one palette, and the copy furthest from its
/// source: the desktop's `clients/desktop/ui/src/styles.css` is the ancestor,
/// `clients/mobile/android/.../ui/Fui.kt` is what this file is a port **of**,
/// and the design mock is what none of the three is a port of any more.
///
/// That last part is the whole reason this comment exists. The mock's own
/// `--text-dim` (#3b545f) measures 2.45:1 against the panel and its
/// `--text-muted` 4.39:1, both of which fail at the sizes a phone uses.
/// `docs/popover-redesign.md` §1 measured them, raised them, and wrote the
/// corrected numbers down; `Fui.kt` ported the corrected set rather than the
/// design file, saying so at length. A third client re-deriving its palette
/// from the mock would fail the same audit a third time, quietly, and the
/// ratio comments below are the only place that audit survives in code. ADR
/// 0010 rejected generating these three files precisely because a generator
/// would strip them.
///
/// Nothing here is an `EnvironmentKey`. There is one theme, it is dark, and it
/// does not vary by device — so an environment value would be a lookup with one
/// possible answer, view invalidation included. It is not a `ShapeStyle`
/// hierarchy either: `Color.accentColor` and friends are Apple's semantic roles,
/// and mapping twenty-nine of them onto a token stops the token being one value.
///
/// `enum` rather than `struct` because there is nothing to instantiate and an
/// empty enum cannot be instantiated by accident.
enum Fui {

    // ── Void ramp: the panel and its recesses ────────────────────────────────

    static let void1000 = Color(0xFF04080C)
    /// The panel, and the window behind it.
    static let void900 = Color(0xFF070C12)
    static let void800 = Color(0xFF0B131B)
    static let void700 = Color(0xFF101B25)
    static let void400 = Color(0xFF24384A)

    // ── Cyan ramp: the emitter colour ────────────────────────────────────────

    static let cyan300 = Color(0xFF7FF3FF)
    static let cyan400 = Color(0xFF4AEAF8)
    static let cyan600 = Color(0xFF1FB9C9)

    static let cyanA08 = Color(0x1435E6F6)
    static let cyanA12 = Color(0x1F35E6F6)
    static let cyanA24 = Color(0x3D35E6F6)
    static let cyanA40 = Color(0x6635E6F6)

    // ── Signal ramps, each measured against ``panel`` ────────────────────────
    //
    // `alert` is on `CONTEXT.md`'s Avoid list, and it stays here anyway. The
    // glossary governs what a *surface* is called in front of a person — a
    // thing that states a consequence is a Notice, and a view showing one is
    // named so. This is the name of a hue: `--alert-400` in the CSS,
    // `Alert400` in the Kotlin, matched across the three on a de-camelised
    // name by ticket 10's `check-tokens`. Renaming it on the third copy would
    // break the one check that exists to stop the copies drifting, in order to
    // fix a word nobody reads.

    /// Caution: a queue, a stale Recall, a code with a clock on it.  10.9:1
    static let amber400 = Color(0xFFF5B642)
    static let amberA16 = Color(0x29F5B642)
    static let amberA40 = Color(0x66F5B642)

    /// Alert. The one true fault, and the two erasures.               7.0:1
    static let alert400 = Color(0xFFFF6B61)
    static let alert500 = Color(0xFFE04B41)
    static let alertA16 = Color(0x29FF6B61)
    static let alertA40 = Color(0x66FF6B61)

    /// In contact.                                                   11.0:1
    static let nominal400 = Color(0xFF3DDC84)

    /// Not in contact, which on a phone is nominal (ADR 0007).         5.4:1
    static let standby400 = Color(0xFF6D8B98)

    // ── Surfaces ─────────────────────────────────────────────────────────────

    /// The screen itself. Every contrast ratio above is against this.
    static let panel = void900

    /// A band cut into the screen: the background policy, the footers.
    static let recess = void1000

    /// A band raised off it: identity, Contact, the verb bar.
    static let band = void800

    /// A panel raised off the screen: a Pairing card's body.
    static let raised = void700

    /// The row the emitter is on, and the band that admits a divergence.
    static let active = cyanA12

    /// The backdrop grid.
    ///
    /// Not the mock's composited value — see ``SwiftUI/View/fuiBackdrop()`` for
    /// why that one paints nothing.
    static let gridLine = Color(0x0A35E6F6)

    // ── Text ─────────────────────────────────────────────────────────────────

    /// Headings and the sentence inside an armed erase.               15.1:1
    static let textPrimary = cyan300

    /// Previews and prose.                                            12.1:1
    static let textBody = Color(0xFFB3D0D9)

    /// Meta, identity, chrome captions.                                8.5:1
    static let textMuted = Color(0xFF8FB0BB)

    /// Separators and inert chips. Never load-bearing text.            5.4:1
    static let textDim = Color(0xFF6D8B98)

    /// The emitter, as text.
    static let textEmitter = cyan300

    /// What sits on a solid emitter fill.
    static let onEmitter = void1000

    // ── Lines ────────────────────────────────────────────────────────────────

    static let hairline = cyanA24
    static let frame = cyanA40
    static let inert = void400

    // ── Geometry ─────────────────────────────────────────────────────────────
    //
    // Points, not dp, and the numbers are carried across unchanged. A point and
    // a dp are the same idea at slightly different reference densities, and
    // nudging 48 to 44 "because iOS says 44" would make two phones that are
    // supposed to look alike measurably not. Apple's 44pt minimum is a floor,
    // not a target, and 48 clears it.
    //
    // Ticket 10's `check-tokens` compares colours only. Nothing checks these,
    // which is why they were transcribed from `Fui.kt` rather than measured off
    // a screenshot.

    /// The smallest thing a thumb may be asked to hit.
    static let target: CGFloat = 48

    /// A control inside a card, where the card is already the big target.
    static let targetSmall: CGFloat = 44

    /// One Entry. Two lines when it carries an Origin, one when it does not.
    static let rowHeight: CGFloat = 68

    /// The screen's own gutter.
    static let gutter: CGFloat = 14

    static let notch: CGFloat = 6

    // ── Type ─────────────────────────────────────────────────────────────────
    //
    // No vendored face, and this is the one place the three shells are knowingly
    // allowed to differ (ADR 0010). Decision 11 of the desktop plan refused a
    // bundled font because Share Tech Mono's ambiguous 0/O and 1/l are wrong for
    // the paths, base64 and `ss://` URLs this list is mostly made of; `Fui.kt`
    // inherited the refusal and so does this file. The consequence is that
    // `.monospaced` here is SF Mono and on Android it is Roboto Mono. The rule
    // ports; the glyph shapes cannot, and adding a face to close the gap would
    // be re-litigating a settled decision on the client with the least standing
    // to do it.
    //
    // `lineSpacing` is the gap *added between* lines, where Compose's
    // `lineHeight` is the whole line box. Each value below is
    // `lineHeight - fontSize`, which is the closest honest translation: SwiftUI
    // adds it on top of the face's own leading, so a two-line paragraph is not
    // pixel-identical to Android's. It cannot be, for the same reason the
    // glyphs are not.
    //
    // `tracking` is in points where Compose's `letterSpacing` is in `em`, so
    // each value is `fontSize x em`. Both are exact.

    /// An Entry's Preview, and anything else that is data rather than prose.
    static let data = FuiTextStyle(
        font: .system(size: 14, design: .monospaced),
        lineSpacing: 3,
        tracking: 0.14
    )

    /// A whole sentence. Prose is read, not scanned, so it is not mono.
    static let prose = FuiTextStyle(
        font: .system(size: 13, weight: .medium),
        lineSpacing: 7,
        tracking: 0
    )

    /// A screen or card heading.
    static let heading = FuiTextStyle(
        font: .system(size: 16, weight: .semibold),
        lineSpacing: 4,
        tracking: 0.96
    )

    /// A step heading inside the pairing flow.
    static let subheading = FuiTextStyle(
        font: .system(size: 15, weight: .semibold),
        lineSpacing: 4,
        tracking: 0.9
    )

    /// A control's word. Wide tracking, because it is one or two words.
    static let label = FuiTextStyle(
        font: .system(size: 11, design: .monospaced),
        lineSpacing: 2,
        tracking: 1.54
    )

    /// Chrome telemetry: the Contact line, an Origin, a footer.
    static let micro = FuiTextStyle(
        font: .system(size: 10, design: .monospaced),
        lineSpacing: 4,
        tracking: 1.2
    )

    /// A glyph used as a control. The set is ``Glyphs``.
    ///
    /// iOS is not short of glyphs and could draw any of them, which is exactly
    /// why the list is fixed elsewhere and not extended here: the set was
    /// narrowed by a constraint that is Android's, and this shell keeps the
    /// result. See ``Glyphs``.
    static let glyph = FuiTextStyle(
        font: .system(size: 15, design: .monospaced),
        lineSpacing: 0,
        tracking: 0
    )

    /// A number that is the news: the pending queue's depth.
    ///
    /// The one place a figure outranks the sentence beside it, so the one type
    /// size that is neither data nor chrome.
    static let readout = FuiTextStyle(
        font: .system(size: 34, design: .monospaced),
        lineSpacing: 0,
        tracking: 0
    )
}

/// The 8-digit ARGB literal, so a token can be diffed against its two siblings.
///
/// `Color(0xFF04080C)` beside Kotlin's `Color(0xFF04080C)` beside CSS's
/// `#04080c` is what makes ticket 10's `check-tokens` a regex rather than a
/// parser, and what makes a wrong hex digit visible in review. Alpha is the high
/// byte, matching Compose, which is why this is not `Color(red:green:blue:)` at
/// each site: a hand-converted `0x3D` to `0.239` is a divergence nothing can
/// check.
///
/// sRGB explicitly. SwiftUI's default `Color(red:green:blue:)` is sRGB already,
/// but the desktop's `#b3d0d9` and Android's `0xFFB3D0D9` are both sRGB by
/// definition and saying so here means a future display-P3 default cannot
/// silently shift the palette.
private extension Color {
    init(_ argb: UInt64) {
        self.init(
            .sRGB,
            red: Double((argb >> 16) & 0xFF) / 255,
            green: Double((argb >> 8) & 0xFF) / 255,
            blue: Double(argb & 0xFF) / 255,
            opacity: Double((argb >> 24) & 0xFF) / 255
        )
    }
}

/// A type role: a face, its leading, and its tracking, as one value.
///
/// SwiftUI has no `TextStyle` — `Font` carries size, weight and design but not
/// line spacing or tracking, both of which are view modifiers. Splitting the
/// three across call sites is how a heading ends up at the right size with the
/// wrong tracking on one screen out of six, so they travel together and are
/// applied together by ``SwiftUI/View/fuiText(_:color:)``.
struct FuiTextStyle: Sendable {
    let font: Font
    let lineSpacing: CGFloat
    let tracking: CGFloat
}

/// The glyphs that are allowed to be controls.
///
/// One list, in one place, because the reason it is short is not obvious from
/// any single use of it. On Android every one of these is in a bundled face on
/// the minSdk floor, and that is a constraint rather than a coincidence: the
/// mock's Recall arrow is **⤓** (U+2913), which no bundled Android face carries,
/// so it falls back to a *different* arrow on a good device and to a tofu box on
/// a thin one. A control whose label is a missing-glyph box is a control nobody
/// presses. `Fui.kt:234-238` substituted **↓** (U+2193).
///
/// **iOS can almost certainly draw ⤓ and deliberately does not.** The constraint
/// was Android's; the outcome is the design's, and two phones showing the same
/// Entries must not show two different Recall arrows (ADR 0010). This paragraph
/// is the whole reason the constant is here rather than inline at the call site:
/// a bare `"↓"` in a row body reads as an oversight and invites a future "fix"
/// into a divergence.
enum Glyphs {
    /// Recall. **Not** the mock's ⤓ — see the type's note.
    static let recall = "↓"
    /// Delete. Unguarded on the desktop, behind a swipe on a phone.
    static let delete = "✕"
    /// Out of a screen. A picture of the door, never a screen reader's word.
    static let back = "◂"
    /// Into the Pairings.
    static let pairings = "◎"
    /// A capability that is absent and is not coming.
    static let absent = "⊘"
    /// The foreground-only pin.
    static let pinned = "⌾"
    /// Opens the foreground-only disclosure: `WHY ▸`.
    static let why = "▸"
    /// Closes it for good: `▴ CLOSE`.
    static let close = "▴"
    /// A step that has been done: the scanned-code note's `✓`.
    ///
    /// Not in `Fui.kt`'s list, which names the six glyphs that are *controls*.
    /// It is here because `PairingScreen.kt:190` passes `"✓"` inline and this
    /// file is where a reader looks for the answer to "which characters is this
    /// app allowed to draw"; one list that is slightly wider than the Kotlin's
    /// beats one list plus an inline literal that nobody greps.
    static let done = "✓"
}

/// Applies a whole type role, and the colour that goes with it.
///
/// Colour is a required argument rather than a default. Every token in ``Fui``
/// is on a measured ramp and there is no such thing as a default one: the same
/// ``Fui/micro`` is `textMuted` in a chrome caption and `alert400` in a fault
/// readout, and a defaulted colour is how the second becomes the first.
extension View {
    func fuiText(_ style: FuiTextStyle, color: Color) -> some View {
        self.font(style.font)
            .tracking(style.tracking)
            .lineSpacing(style.lineSpacing)
            .foregroundStyle(color)
    }
}

// ── Shape ────────────────────────────────────────────────────────────────────

/// The corner cut that says "machined" without saying "rounded".
///
/// Top-left and bottom-right, which is the mock's `--clip-notch-sm`. Two corners
/// rather than four so that a row of them reads as one direction of travel.
///
/// `InsettableShape` and not merely `Shape`, so that a border can be
/// `strokeBorder` rather than `stroke`. Compose's `Modifier.border` draws
/// entirely *inside* the bounds; SwiftUI's `stroke` straddles the edge, which
/// puts half a point of every frame outside the layout rectangle and makes a
/// badge sitting flush against a hairline overlap it. `strokeBorder` insets by
/// half the line width and matches Compose.
struct NotchShape: InsettableShape {

    var notch: CGFloat = Fui.notch
    private var inset: CGFloat = 0

    func path(in rect: CGRect) -> Path {
        let r = rect.insetBy(dx: inset, dy: inset)
        let cut = min(notch, min(r.width, r.height) / 2)
        var path = Path()
        path.move(to: CGPoint(x: r.minX + cut, y: r.minY))
        path.addLine(to: CGPoint(x: r.maxX, y: r.minY))
        path.addLine(to: CGPoint(x: r.maxX, y: r.maxY - cut))
        path.addLine(to: CGPoint(x: r.maxX - cut, y: r.maxY))
        path.addLine(to: CGPoint(x: r.minX, y: r.maxY))
        path.addLine(to: CGPoint(x: r.minX, y: r.minY + cut))
        path.closeSubpath()
        return path
    }

    func inset(by amount: CGFloat) -> NotchShape {
        var copy = self
        copy.inset += amount
        return copy
    }
}

// ── Atmosphere ───────────────────────────────────────────────────────────────

/// The atmosphere behind a screen: a fine grid and a vignette, both faint.
///
/// The vignette is at the mock's own 0.35, which is already the corrected
/// value — the desktop plan measured the untrimmed overlay stack at a 9%
/// contrast cost and cut it to exactly that. The grid is **not** a literal port
/// of the mock: it composites an 8% cyan at 22% layer opacity, which lands under
/// one 8-bit level over the panel and paints nothing at all on a phone.
/// ``Fui/gridLine`` is the alpha Android found actually renders.
///
/// Scanlines are not here: they cost the most contrast and belong only on a
/// chrome band, never behind an Entry's Preview.
///
/// **Line widths are in device pixels, not points.** Compose's `DrawScope` works
/// in pixels, so Android's grid is a one-*pixel* line however dense the screen
/// is. A literal transcription to a 1-point line would be two or three times as
/// much ink on the phones this ships to, which on a token whose whole design
/// problem is that it nearly does not render is not a rounding difference. The
/// step stays in points, because 32dp is a spacing and spacings are the one
/// thing dp and pt agree on.
private struct FuiBackdrop: ViewModifier {

    @Environment(\.displayScale) private var displayScale

    func body(content: Content) -> some View {
        content.background {
            Canvas(opaque: false) { ctx, size in
                let step: CGFloat = 32
                let hair = 1 / displayScale
                var x: CGFloat = 0
                while x < size.width {
                    ctx.fill(
                        Path(CGRect(x: x, y: 0, width: hair, height: size.height)),
                        with: .color(Fui.gridLine)
                    )
                    x += step
                }
                var y: CGFloat = 0
                while y < size.height {
                    ctx.fill(
                        Path(CGRect(x: 0, y: y, width: size.width, height: hair)),
                        with: .color(Fui.gridLine)
                    )
                    y += step
                }
                ctx.fill(
                    Path(CGRect(origin: .zero, size: size)),
                    with: .radialGradient(
                        Gradient(stops: [
                            .init(color: .clear, location: 0.4),
                            // 0.85 x 0.35: the mock's overlay opacity times the
                            // corrected vignette strength, kept as a product so
                            // both numbers stay readable against the plan.
                            .init(color: Fui.void1000.opacity(0.85 * 0.35), location: 1.0),
                        ]),
                        center: CGPoint(x: size.width / 2, y: size.height * 0.4),
                        startRadius: 0,
                        endRadius: max(size.width, size.height) * 0.9
                    )
                )
            }
            .accessibilityHidden(true)
        }
    }
}

/// Scanlines, for chrome only.
///
/// A one-pixel darkening on every third row. Measured at ~9% of contrast on the
/// desktop, which is why nothing carrying an Entry's Preview gets it and why
/// ``ChromeBand`` asks for it rather than assuming it.
///
/// Pixels again, and here it decides the pattern rather than the weight: Android
/// darkens one physical pixel in three. At 3x, a points-based period would put
/// nine physical pixels between lines and the band would read as a grille rather
/// than a texture.
///
/// **Apply this before the band's own background colour, not after.** SwiftUI
/// stacks `background` backwards — the later one goes further back — so
/// `.fuiScanlines().background(colour)` is the order that puts the lines over
/// the fill and under the content, which is what Compose's `drawBehind` after
/// `background` does. The other order paints them and then hides them.
///
/// ``active`` is a flag rather than an `if` at the call site, and that is a
/// correctness point rather than a tidiness one: branching a modifier chain
/// changes the view's *type*, and SwiftUI treats a type change as a different
/// view. The History's Contact band gains and loses its scanlines with the
/// Session Phase, so an `if` there would tear down and rebuild the band's whole
/// subtree on a state change that is meant to repaint one texture.
private struct FuiScanlines: ViewModifier {

    var active: Bool = true

    @Environment(\.displayScale) private var displayScale

    func body(content: Content) -> some View {
        content.background {
            Canvas(opaque: false) { ctx, size in
                guard active else { return }
                let px = 1 / displayScale
                var y = 2 * px
                while y < size.height {
                    ctx.fill(
                        Path(CGRect(x: 0, y: y, width: size.width, height: px)),
                        with: .color(.black.opacity(0.25))
                    )
                    y += 3 * px
                }
            }
            .accessibilityHidden(true)
        }
    }
}

/// A dashed outline, for a slot that is empty on purpose.
///
/// The two camera failures use it: the viewfinder is missing rather than broken,
/// and a solid frame around the explanation would read as an alert when the
/// point is that the typed path is right there and works just as well.
private struct DashedBorder: ViewModifier {

    let color: Color

    func body(content: Content) -> some View {
        // A stroked `Rectangle` rather than a `Canvas`: the dash phase is the
        // whole content of this modifier and `StrokeStyle` already says it in
        // one line, where a Canvas would be four sides of hand-walked path with
        // the corners to get wrong.
        content.overlay {
            Rectangle()
                .strokeBorder(color, style: StrokeStyle(lineWidth: 1, dash: [6, 5]))
                .accessibilityHidden(true)
        }
    }
}

extension View {

    /// See ``FuiBackdrop``.
    func fuiBackdrop() -> some View { modifier(FuiBackdrop()) }

    /// See ``FuiScanlines`` — including its note on modifier order.
    func fuiScanlines() -> some View { modifier(FuiScanlines()) }

    /// See ``DashedBorder``.
    func dashedBorder(_ color: Color) -> some View { modifier(DashedBorder(color: color)) }
}

// ── Views ────────────────────────────────────────────────────────────────────

/// A one-pixel rule in the emitter's faintest tint.
@MainActor
struct Hairline: View {

    var color: Color = Fui.hairline

    var body: some View {
        Rectangle()
            .fill(color)
            .frame(maxWidth: .infinity)
            .frame(height: 1)
            .accessibilityHidden(true)
    }
}

/// A fixed-height band of chrome, with the rule that separates it from the next.
///
/// Chrome is what cannot scroll away. Everything the History pins above its list
/// is one of these, which is what makes "140pt of chrome" a number rather than
/// an accident.
///
/// The stack is zero-spaced. SwiftUI's default `HStack` spacing is around eight
/// points and depends on what is adjacent, which would make a band's internal
/// gaps a property of its contents rather than of the design; callers space
/// their own children.
@MainActor
struct ChromeBand<Content: View>: View {

    private let height: CGFloat
    private let background: Color
    private let scanlines: Bool
    private let content: Content

    init(
        height: CGFloat,
        background: Color = Fui.band,
        scanlines: Bool = false,
        @ViewBuilder content: () -> Content
    ) {
        self.height = height
        self.background = background
        self.scanlines = scanlines
        self.content = content()
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 0) { content }
                .padding(.horizontal, Fui.gutter)
                .frame(maxWidth: .infinity, alignment: .leading)
                .frame(height: height)
                // Order matters and is not a style choice: see `FuiScanlines`,
                // which also says why this is a flag and not an `if`.
                .modifier(FuiScanlines(active: scanlines))
                .background(background)
            Hairline()
        }
    }
}

/// What a status light can say.
///
/// The mock's own four lamp states, kept in its words. Not to be confused with
/// `Tone`, which arrives with the History Screen and answers a different
/// question about the same Session Phase: `Tone` is whether the person has to
/// *act*, and only ``Signal/alert`` here is a fault there. ``Signal/nominal`` is
/// narrower than `Tone.nominal` — it means in contact, while three of `Tone`'s
/// nominal phases light ``Signal/standby``.
enum Signal {

    /// In contact with the Relay. The exceptional state on a phone.
    case nominal

    /// Work in progress: a session coming up, a code with a clock on it.
    case caution

    /// Not in contact, or not the Active Pairing. Nominal on a phone.
    case standby

    /// The revoked token. The one lamp that is a fault, and the only red.
    case alert

    var colour: Color {
        switch self {
        case .nominal: return Fui.nominal400
        case .caution: return Fui.amber400
        case .standby: return Fui.standby400
        case .alert: return Fui.alert400
        }
    }
}

/// A lit square and a caps label: the phone's one telemetry idiom.
///
/// A square rather than a dot, because a HUD does not draw circles and because a
/// 6pt square survives a low-density screen that eats a 6pt circle's edges.
@MainActor
struct StatusLight: View {

    let signal: Signal
    let label: String

    var body: some View {
        HStack(spacing: 8) {
            Rectangle()
                .fill(signal.colour)
                .frame(width: 6, height: 6)
            Text(label)
                .fuiText(Fui.micro, color: signal == .alert ? Fui.alert400 : Fui.textMuted)
                .lineLimit(1)
                .truncationMode(.tail)
        }
        // One node, not a lamp and a caption. A screen reader should say the
        // reading once, and a test asking this readout what it says should get a
        // sentence rather than a list of two children.
        .accessibilityElement(children: .combine)
    }
}

/// How a badge or a button is pitched.
enum Accent {

    /// The emitter. What the person came here to press.
    case emitter

    /// Present, not pushed. Never solid — a neutral emitter is a contradiction.
    case neutral

    /// A queue, a clock, a Recall that may be stale.
    case caution

    /// Cannot be undone.
    case alert

    var ink: Color {
        switch self {
        case .emitter: return Fui.cyan300
        case .neutral: return Fui.textBody
        case .caution: return Fui.amber400
        case .alert: return Fui.alert400
        }
    }

    var edge: Color {
        switch self {
        case .emitter: return Fui.cyanA40
        case .neutral: return Fui.inert
        case .caution: return Fui.amberA40
        case .alert: return Fui.alertA40
        }
    }

    var fill: Color {
        switch self {
        case .emitter: return Fui.cyan400
        case .neutral: return Fui.inert
        case .caution: return Fui.amber400
        case .alert: return Fui.alert500
        }
    }
}

/// A small stated fact — SYNCING, SHOWING, CANNOT BE UNDONE.
@MainActor
struct FuiBadge: View {

    let text: String
    let accent: Accent
    var solid: Bool = false

    var body: some View {
        Text(text)
            .fuiText(Fui.micro, color: solid ? Fui.onEmitter : accent.ink)
            .lineLimit(1)
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background {
                if solid {
                    NotchShape().fill(accent.fill)
                } else {
                    NotchShape().strokeBorder(accent.edge, lineWidth: 1)
                }
            }
    }
}

/// A chip for something that is not there and is not coming.
///
/// The three N/A chips on the settings are the whole reason this exists: a
/// missing switch and an unbuilt screen look identical, and a chip that says
/// `WATCHED CAPTURE · N/A` is the difference between a decision and an omission.
@MainActor
struct FuiTag: View {

    let text: String
    var inert: Bool = false

    var body: some View {
        Text(text)
            .fuiText(Fui.micro, color: inert ? Fui.textDim : Fui.textMuted)
            .lineLimit(1)
            .padding(.horizontal, 8)
            .frame(height: 24)
            // Square, not notched: the notch is for things that can be pressed.
            .overlay {
                Rectangle().strokeBorder(inert ? Fui.inert : Fui.hairline, lineWidth: 1)
            }
    }
}

/// A verb.
///
/// ``solid`` fills with the accent and is reserved for the one verb on a surface
/// that outranks the others — Recall Latest on the History, Pair this phone on
/// the pairing flow, Forget inside an armed erase. Two solids on one screen is
/// two emitters, which is none.
@MainActor
struct FuiButton: View {

    let text: String
    let action: () -> Void
    var accent: Accent = .emitter
    var solid: Bool = false
    var enabled: Bool = true
    var height: CGFloat = Fui.target

    /// Whether the verb takes the width it is given.
    ///
    /// This exists because SwiftUI cannot express it from the outside. Kotlin's
    /// caller passes `Modifier.fillMaxWidth()` or `weight(1f)` — PAIR THIS PHONE
    /// on the pairing flow, the two verbs in the History's bar — and a
    /// `.frame(maxWidth: .infinity)` applied to a `FuiButton` stretches the
    /// layout rectangle while the notched border stays hugging the word, because
    /// the border is inside the `Button`'s own label. A verb at intrinsic width
    /// where the design says full width reads as a chip rather than the one
    /// emitter on the screen, which is the whole point of ``solid``.
    var fillsWidth: Bool = false

    private var ink: Color {
        if !enabled { return Fui.textDim }
        // Every solid fill takes the void as its ink, alert included. The mock
        // puts `--cyan-100` on `--alert-500`, which measures 3.1:1 at 11pt and
        // fails AA; the void measures 5.0:1 on the same fill. Same correction,
        // same reason, as the text ramp at the top of this file.
        if solid { return Fui.onEmitter }
        return accent.ink
    }

    var body: some View {
        Button(action: action) {
            Text(text)
                .fuiText(Fui.label, color: ink)
                .lineLimit(1)
                .padding(.horizontal, 16)
                .frame(maxWidth: fillsWidth ? .infinity : nil)
                .frame(height: height)
                .background {
                    if !enabled {
                        NotchShape().strokeBorder(Fui.inert, lineWidth: 1)
                    } else if solid {
                        NotchShape().fill(accent.fill)
                    } else {
                        NotchShape().strokeBorder(accent.edge, lineWidth: 1)
                    }
                }
                .contentShape(NotchShape())
        }
        // `.plain`, because every button style Apple ships paints something —
        // a tint, a capsule, a pressed fill — and all of it would land on top of
        // the notch and the accent this view has just decided.
        .buttonStyle(.plain)
        .disabled(!enabled)
    }
}

/// A glyph on a square target.
///
/// 48pt whether or not the glyph fills it. Recall lives on one of these on every
/// row, and a row's worth of thumb is the reason the row has only one control.
///
/// Disabled is drawn, not hidden: an Undecryptable Entry's Recall stays on the
/// row in ``Fui/inert`` so that the row still says what it would be able to do,
/// rather than losing a control and looking like a different kind of row.
@MainActor
struct GlyphButton: View {

    let glyph: String
    let action: () -> Void
    /// The verb, in words. Required, and see ``body`` for why.
    let accessibilityLabel: String
    var accent: Accent = .emitter
    var enabled: Bool = true

    var body: some View {
        Button(action: action) {
            Text(glyph)
                .fuiText(Fui.glyph, color: enabled ? accent.ink : Fui.textDim)
                // The glyph is a picture of the verb, not its name. Without
                // these two lines the accessible name of every row's Recall is
                // "↓", which VoiceOver reads as "down arrow".
                .accessibilityHidden(true)
                .frame(width: Fui.target, height: Fui.target)
                .background {
                    NotchShape().strokeBorder(enabled ? accent.edge : Fui.inert, lineWidth: 1)
                }
                .contentShape(NotchShape())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .accessibilityLabel(accessibilityLabel)
    }
}

/// A framed panel with a header strip: a Pairing card, the viewfinder.
///
/// ``title`` is the thing itself and ``code`` is its address — for a Pairing that
/// is the **User** and then the relay host, in that order, because heading a
/// Pairing with this machine's own Device Label is the desktop's mistake and the
/// card is where it was made. The User's id is not repeated in the address: the
/// heading already carries the User, and the repetition is what pushed the host
/// off the end of the line.
@MainActor
struct FuiPanel<Trailing: View, Content: View>: View {

    private let title: String
    private let code: String?
    private let accent: Accent
    private let trailing: Trailing
    private let content: Content

    init(
        title: String,
        code: String? = nil,
        accent: Accent = .emitter,
        @ViewBuilder trailing: () -> Trailing,
        @ViewBuilder content: () -> Content
    ) {
        self.title = title
        self.code = code
        self.accent = accent
        self.trailing = trailing()
        self.content = content()
    }

    private var isAlert: Bool { accent == .alert }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                VStack(alignment: .leading, spacing: 0) {
                    Text(title)
                        .fuiText(Fui.heading, color: isAlert ? Fui.alert400 : Fui.textPrimary)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    if let code {
                        Text(code)
                            .fuiText(Fui.micro, color: Fui.textMuted)
                            .lineLimit(1)
                            .truncationMode(.tail)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                trailing
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(isAlert ? Fui.alertA16 : Fui.cyanA08)
            Hairline()
            VStack(alignment: .leading, spacing: 0) { content }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(12)
        }
        .frame(maxWidth: .infinity)
        .background(Fui.raised)
        .overlay {
            Rectangle().strokeBorder(isAlert ? Fui.alertA40 : Fui.frame, lineWidth: 1)
        }
    }
}

extension FuiPanel where Trailing == EmptyView {

    /// A panel with nothing in its header but its own heading.
    ///
    /// A separate initialiser rather than a defaulted `trailing:`, because a
    /// generic parameter cannot be defaulted and the alternative is every plain
    /// panel in the app writing `trailing: { EmptyView() }`.
    init(
        title: String,
        code: String? = nil,
        accent: Accent = .emitter,
        @ViewBuilder content: () -> Content
    ) {
        self.init(title: title, code: code, accent: accent, trailing: { EmptyView() }, content: content)
    }
}

/// A quoted note: a rule left of a sentence, no container around it.
///
/// For the two settings sentences and the pairing explainers — prose long enough
/// that a box would read as an alert and short enough that a heading would be
/// heavier than the thing it heads.
@MainActor
struct QuotedNote: View {

    let text: String

    var body: some View {
        // The rule is an overlay on the sentence rather than a sibling in a
        // stack. Compose says `height(IntrinsicSize.Min)` and gets a rule
        // exactly as tall as the text; SwiftUI has no equivalent, and a
        // `Rectangle` beside a `Text` in an `HStack` is greedy — it would take
        // whatever height the parent proposed and the rule would overshoot the
        // sentence by however much room the screen had left. An overlay is
        // measured against the sentence's own frame, which is the property that
        // was wanted in the first place.
        Text(text)
            .fuiText(Fui.prose, color: Fui.textBody)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.leading, 12)
            .overlay(alignment: .leading) {
                Rectangle()
                    .fill(Fui.hairline)
                    .frame(width: 2)
                    .accessibilityHidden(true)
            }
    }
}

// ── Gallery ──────────────────────────────────────────────────────────────────

#if DEBUG

/// Every word of the vocabulary, top to bottom, on one scroll.
///
/// Not decoration and not a unit test's substitute. ADR 0010 accepts that
/// `check-tokens` compares colours only and that geometry drift "is caught by
/// looking at the device" — this is the surface that gets looked at. Run it
/// beside the Android build's own preview and the two should differ in the
/// glyph shapes and in nothing else.
///
/// **A `PreviewProvider` rather than a `#Preview`, and not by preference.**
/// `#Preview` expands to `#externalMacro(module: "PreviewsMacros", ...)`, and
/// that plugin ships inside Xcode. This package is built by `xtool` against an
/// open-source toolchain with no Xcode on the machine, so the macro fails at
/// compile time with *"plugin for module 'PreviewsMacros' not found"* — it does
/// not degrade to nothing, it breaks the build. There is no Mac in this loop and
/// there is not meant to be one. The iOS 13-era ``FuiGallery_Previews`` below
/// compiles clean on this path and is what the canvas reads if anyone ever does
/// open one.
///
/// On this build path the canvas is nobody's, so the way to actually *look* at
/// it is to swap the body of `SharepasteApp`'s `WindowGroup` for `FuiGallery()`
/// and run `xtool dev`. Deliberately not wired to a gesture or a hidden setting:
/// a debug surface reachable from the shipped app is a debug surface that
/// eventually ships.
@MainActor
struct FuiGallery: View {

    private struct Swatch: View {
        let name: String
        let color: Color
        var body: some View {
            HStack(spacing: 8) {
                Rectangle()
                    .fill(color)
                    .frame(width: 22, height: 22)
                    .overlay { Rectangle().strokeBorder(Fui.inert, lineWidth: 1) }
                Text(name).fuiText(Fui.micro, color: Fui.textMuted)
            }
        }
    }

    private struct GallerySection<Inner: View>: View {
        let title: String
        @ViewBuilder let inner: Inner
        var body: some View {
            VStack(alignment: .leading, spacing: 10) {
                Text(title).fuiText(Fui.label, color: Fui.textDim)
                inner
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {

                GallerySection(title: "VOID / CYAN") {
                    VStack(alignment: .leading, spacing: 6) {
                        Swatch(name: "void1000", color: Fui.void1000)
                        Swatch(name: "void900 · panel", color: Fui.void900)
                        Swatch(name: "void800 · band", color: Fui.void800)
                        Swatch(name: "void700 · raised", color: Fui.void700)
                        Swatch(name: "void400 · inert", color: Fui.void400)
                        Swatch(name: "cyan300", color: Fui.cyan300)
                        Swatch(name: "cyan400", color: Fui.cyan400)
                        Swatch(name: "cyan600", color: Fui.cyan600)
                        Swatch(name: "cyanA08 / 12 / 24 / 40", color: Fui.cyanA40)
                    }
                }

                GallerySection(title: "SIGNAL") {
                    VStack(alignment: .leading, spacing: 6) {
                        Swatch(name: "amber400  10.9:1", color: Fui.amber400)
                        Swatch(name: "alert400   7.0:1", color: Fui.alert400)
                        Swatch(name: "alert500", color: Fui.alert500)
                        Swatch(name: "nominal400 11.0:1", color: Fui.nominal400)
                        Swatch(name: "standby400  5.4:1", color: Fui.standby400)
                    }
                }

                GallerySection(title: "TEXT") {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("textPrimary 15.1:1").fuiText(Fui.prose, color: Fui.textPrimary)
                        Text("textBody 12.1:1").fuiText(Fui.prose, color: Fui.textBody)
                        Text("textMuted 8.5:1").fuiText(Fui.prose, color: Fui.textMuted)
                        Text("textDim 5.4:1").fuiText(Fui.prose, color: Fui.textDim)
                    }
                }

                GallerySection(title: "TYPE") {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("data · ss://Y2hhY2hh@10.0.0.1:8388")
                            .fuiText(Fui.data, color: Fui.textBody)
                        Text("prose · a whole sentence, read rather than scanned.")
                            .fuiText(Fui.prose, color: Fui.textBody)
                        Text("HEADING").fuiText(Fui.heading, color: Fui.textPrimary)
                        Text("SUBHEADING").fuiText(Fui.subheading, color: Fui.textPrimary)
                        Text("LABEL").fuiText(Fui.label, color: Fui.textEmitter)
                        Text("MICRO · CHROME TELEMETRY").fuiText(Fui.micro, color: Fui.textMuted)
                        Text("12").fuiText(Fui.readout, color: Fui.amber400)
                    }
                }

                GallerySection(title: "GLYPHS") {
                    HStack(spacing: 10) {
                        ForEach(
                            [
                                Glyphs.recall, Glyphs.delete, Glyphs.back, Glyphs.pairings,
                                Glyphs.absent, Glyphs.pinned, Glyphs.why, Glyphs.close,
                                Glyphs.done,
                            ],
                            id: \.self
                        ) { glyph in
                            Text(glyph).fuiText(Fui.glyph, color: Fui.textPrimary)
                        }
                    }
                }

                GallerySection(title: "STATUS LIGHTS") {
                    VStack(alignment: .leading, spacing: 6) {
                        StatusLight(signal: .nominal, label: "IN CONTACT")
                        StatusLight(signal: .caution, label: "PAIRING · 04:52")
                        StatusLight(signal: .standby, label: "NOT IN CONTACT · NOMINAL")
                        StatusLight(signal: .alert, label: "PAIRING REVOKED")
                    }
                }

                GallerySection(title: "BADGES / TAGS") {
                    VStack(alignment: .leading, spacing: 8) {
                        HStack(spacing: 8) {
                            FuiBadge(text: "SYNCING", accent: .emitter)
                            FuiBadge(text: "SHOWING", accent: .emitter, solid: true)
                            FuiBadge(text: "QUEUED", accent: .caution)
                            FuiBadge(text: "CANNOT BE UNDONE", accent: .alert, solid: true)
                        }
                        HStack(spacing: 8) {
                            FuiTag(text: "THIS PHONE")
                            FuiTag(text: "WATCHED CAPTURE · N/A", inert: true)
                        }
                    }
                }

                GallerySection(title: "BUTTONS") {
                    VStack(alignment: .leading, spacing: 8) {
                        HStack(spacing: 8) {
                            FuiButton(text: "RECALL LATEST", action: {}, solid: true)
                            FuiButton(text: "OFFER", action: {})
                        }
                        // The pairing flow's one emitter, at the width the
                        // Kotlin gives it with `Modifier.fillMaxWidth()`.
                        FuiButton(text: "PAIR THIS PHONE", action: {}, solid: true, fillsWidth: true)
                        HStack(spacing: 8) {
                            FuiButton(text: "FORGET", action: {}, accent: .alert, solid: true)
                            FuiButton(text: "PAIRINGS", action: {}, accent: .neutral)
                            FuiButton(text: "UNAVAILABLE", action: {}, enabled: false)
                        }
                        HStack(spacing: 8) {
                            GlyphButton(glyph: Glyphs.recall, action: {}, accessibilityLabel: "Recall")
                            // The Undecryptable row's Recall: drawn, not hidden.
                            GlyphButton(
                                glyph: Glyphs.recall,
                                action: {},
                                accessibilityLabel: "Recall",
                                enabled: false
                            )
                            GlyphButton(
                                glyph: Glyphs.delete,
                                action: {},
                                accessibilityLabel: "Delete",
                                accent: .alert
                            )
                            GlyphButton(glyph: Glyphs.back, action: {}, accessibilityLabel: "Back")
                        }
                    }
                }

                GallerySection(title: "PANELS") {
                    VStack(alignment: .leading, spacing: 10) {
                        FuiPanel(title: "ada", code: "relay.example.net") {
                            Text("Paired 3 days ago · last contact 2 minutes ago")
                                .fuiText(Fui.micro, color: Fui.textMuted)
                        }
                        FuiPanel(
                            title: "FORGET THIS PAIRING",
                            accent: .alert,
                            trailing: { FuiBadge(text: "ARMED", accent: .alert, solid: true) },
                            content: {
                                Text("This phone will stop receiving Entries.")
                                    .fuiText(Fui.prose, color: Fui.textBody)
                            }
                        )
                        VStack(alignment: .leading, spacing: 0) {
                            Text("NO CAMERA")
                                .fuiText(Fui.label, color: Fui.textDim)
                                .frame(maxWidth: .infinity, alignment: .center)
                                .frame(height: 96)
                        }
                        .frame(maxWidth: .infinity)
                        .dashedBorder(Fui.inert)
                    }
                }

                GallerySection(title: "CHROME") {
                    VStack(spacing: 0) {
                        ChromeBand(height: 44) {
                            Text("SHAREPASTE").fuiText(Fui.label, color: Fui.textEmitter)
                            Spacer(minLength: 0)
                            StatusLight(signal: .nominal, label: "IN CONTACT")
                        }
                        ChromeBand(height: 30, background: Fui.recess, scanlines: true) {
                            Text("\(Glyphs.pinned) NOTHING ARRIVES WHILE THIS IS CLOSED")
                                .fuiText(Fui.micro, color: Fui.amber400)
                            Spacer(minLength: 0)
                            Text("WHY \(Glyphs.why)").fuiText(Fui.micro, color: Fui.textMuted)
                        }
                    }
                }

                GallerySection(title: "NOTE") {
                    QuotedNote(
                        text: "Sharepaste only looks for new Entries while this app is open. "
                            + "It does no work in the background — that is deliberate."
                    )
                }
            }
            .padding(Fui.gutter)
        }
        .background(Fui.panel)
        .fuiBackdrop()
    }
}

struct FuiGallery_Previews: PreviewProvider {
    static var previews: some View {
        FuiGallery()
    }
}

#endif
