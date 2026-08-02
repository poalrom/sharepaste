package com.sharepaste.android.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Outline
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp

/**
 * The FUI/HUD language, as Compose values.
 *
 * The desktop's popover and main window already speak it, from
 * `clients/desktop/ui/src/styles.css`, and **this file is a port of that token
 * set rather than of the design mock's**. The difference is deliberate and is
 * the whole reason it is stated here: the mock's own `--text-dim` (#3b545f)
 * measures 2.45:1 against the panel and its `--text-muted` 4.39:1, both of which
 * fail at the sizes a phone uses. `docs/popover-redesign.md` §1 raised them once,
 * measured; a phone re-deriving its palette from the mock would quietly fail the
 * same audit a second time.
 *
 * Nothing here is a `CompositionLocal`. There is one theme, it is dark, and it
 * does not vary by device — so a local would be a lookup with one possible
 * answer, recomposition scope included.
 */
object Fui {

    // ── Void ramp: the panel and its recesses ────────────────────────────────

    val Void1000 = Color(0xFF04080C)
    /** The panel, and the window behind it — `window_void` in `themes.xml`. */
    val Void900 = Color(0xFF070C12)
    val Void800 = Color(0xFF0B131B)
    val Void700 = Color(0xFF101B25)
    val Void400 = Color(0xFF24384A)

    // ── Cyan ramp: the emitter colour ────────────────────────────────────────

    val Cyan300 = Color(0xFF7FF3FF)
    val Cyan400 = Color(0xFF4AEAF8)
    val Cyan600 = Color(0xFF1FB9C9)

    val CyanA08 = Color(0x1435E6F6)
    val CyanA12 = Color(0x1F35E6F6)
    val CyanA24 = Color(0x3D35E6F6)
    val CyanA40 = Color(0x6635E6F6)

    // ── Signal ramps, each measured against [Panel] ──────────────────────────

    /** Caution: a queue, a stale Recall, a code with a clock on it.  10.9:1 */
    val Amber400 = Color(0xFFF5B642)
    val AmberA16 = Color(0x29F5B642)
    val AmberA40 = Color(0x66F5B642)

    /** Alert. The one true fault, and the two erasures.               7.0:1 */
    val Alert400 = Color(0xFFFF6B61)
    val Alert500 = Color(0xFFE04B41)
    val AlertA16 = Color(0x29FF6B61)
    val AlertA40 = Color(0x66FF6B61)

    /** In contact.                                                   11.0:1 */
    val Nominal400 = Color(0xFF3DDC84)

    /** Not in contact, which on a phone is nominal (ADR 0007).         5.4:1 */
    val Standby400 = Color(0xFF6D8B98)

    // ── Surfaces ─────────────────────────────────────────────────────────────

    /** The screen itself. Every contrast ratio above is against this. */
    val Panel = Void900

    /** A band cut into the screen: the background policy, the footers. */
    val Recess = Void1000

    /** A band raised off it: identity, Contact, the verb bar. */
    val Band = Void800

    /** A panel raised off the screen: a Pairing card's body. */
    val Raised = Void700

    /** The row the emitter is on, and the band that admits a divergence. */
    val Active = CyanA12

    /**
     * The backdrop grid.
     *
     * Not the mock's composited value — see [fuiBackdrop] for why that one
     * paints nothing.
     */
    val GridLine = Color(0x0A35E6F6)

    // ── Text ─────────────────────────────────────────────────────────────────

    /** Headings and the sentence inside an armed erase.               15.1:1 */
    val TextPrimary = Cyan300

    /** Previews and prose.                                            12.1:1 */
    val TextBody = Color(0xFFB3D0D9)

    /** Meta, identity, chrome captions.                                8.5:1 */
    val TextMuted = Color(0xFF8FB0BB)

    /** Separators and inert chips. Never load-bearing text.            5.4:1 */
    val TextDim = Color(0xFF6D8B98)

    /** The emitter, as text. */
    val TextEmitter = Cyan300

    /** What sits on a solid emitter fill. */
    val OnEmitter = Void1000

    // ── Lines ────────────────────────────────────────────────────────────────

    val Hairline = CyanA24
    val Frame = CyanA40
    val Inert = Void400

    // ── Geometry ─────────────────────────────────────────────────────────────

    /** The smallest thing a thumb may be asked to hit. */
    val Target = 48.dp

    /** A control inside a card, where the card is already the big target. */
    val TargetSmall = 44.dp

    /** One Entry. Two lines when it carries an Origin, one when it does not. */
    val RowHeight = 68.dp

    /** The screen's own gutter. */
    val Gutter = 14.dp

    val Notch = 6.dp

    // ── Type ─────────────────────────────────────────────────────────────────
    //
    // No vendored face. Share Tech Mono's ambiguous 0/O and 1/l are wrong for
    // the paths, base64 and `ss://` URLs this list is mostly made of, which is
    // the same call `docs/popover-redesign.md` §1.3 made for the desktop.

    /** An Entry's Preview, and anything else that is data rather than prose. */
    val Data = TextStyle(
        fontFamily = FontFamily.Monospace,
        fontSize = 14.sp,
        lineHeight = 17.sp,
        letterSpacing = 0.01.em,
    )

    /** A whole sentence. Prose is read, not scanned, so it is not mono. */
    val Prose = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontSize = 13.sp,
        lineHeight = 20.sp,
        fontWeight = FontWeight.Medium,
    )

    /** A screen or card heading. */
    val Heading = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontSize = 16.sp,
        lineHeight = 20.sp,
        fontWeight = FontWeight.SemiBold,
        letterSpacing = 0.06.em,
    )

    /** A step heading inside the pairing flow. */
    val Subheading = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontSize = 15.sp,
        lineHeight = 19.sp,
        fontWeight = FontWeight.SemiBold,
        letterSpacing = 0.06.em,
    )

    /** A control's word. Wide tracking, because it is one or two words. */
    val Label = TextStyle(
        fontFamily = FontFamily.Monospace,
        fontSize = 11.sp,
        lineHeight = 13.sp,
        letterSpacing = 0.14.em,
    )

    /** Chrome telemetry: the Contact line, an Origin, a footer. */
    val Micro = TextStyle(
        fontFamily = FontFamily.Monospace,
        fontSize = 10.sp,
        lineHeight = 14.sp,
        letterSpacing = 0.12.em,
    )

    /**
     * A glyph used as a control: ↓, ✕, ◎, ◂, ⊘, ⌾.
     *
     * Every one of those is in the platform's own fonts on the minSdk floor.
     * That is a constraint, not a coincidence: the mock's Recall arrow is ⤓
     * (U+2913), which no bundled Android face carries, so it silently falls back
     * to a *different* arrow on a good device and to a tofu box on a thin one.
     * A control whose label is a missing-glyph box is a control nobody presses.
     * Check a new glyph on the emulator before adding it here.
     */
    val Glyph = TextStyle(
        fontFamily = FontFamily.Monospace,
        fontSize = 15.sp,
        lineHeight = 15.sp,
    )

    /**
     * A number that is the news: the pending queue's depth.
     *
     * The one place a figure outranks the sentence beside it, so the one type
     * size that is neither data nor chrome.
     */
    val Readout = TextStyle(
        fontFamily = FontFamily.Monospace,
        fontSize = 34.sp,
        lineHeight = 34.sp,
    )
}

/**
 * The corner cut that says "machined" without saying "rounded".
 *
 * Top-left and bottom-right, which is the mock's `--clip-notch-sm`. Two corners
 * rather than four so that a row of them reads as one direction of travel.
 */
@Immutable
class NotchShape(private val notch: Dp = Fui.Notch) : Shape {

    override fun createOutline(size: Size, layoutDirection: LayoutDirection, density: Density): Outline {
        val cut = with(density) { notch.toPx() }
            .coerceAtMost(minOf(size.width, size.height) / 2f)
        val path = Path().apply {
            moveTo(cut, 0f)
            lineTo(size.width, 0f)
            lineTo(size.width, size.height - cut)
            lineTo(size.width - cut, size.height)
            lineTo(0f, size.height)
            lineTo(0f, cut)
            close()
        }
        return Outline.Generic(path)
    }

    override fun equals(other: Any?) = other is NotchShape && other.notch == notch

    override fun hashCode() = notch.hashCode()
}

private val notched = NotchShape()

/**
 * The atmosphere behind a screen: a fine grid and a vignette, both faint.
 *
 * The vignette is at the mock's own 0.35, which is already the corrected value —
 * the desktop plan measured the untrimmed overlay stack at a 9% contrast cost
 * and cut it to exactly that. The grid is **not** a literal port: the mock
 * composites an 8% cyan at 22% layer opacity, which lands under one 8-bit level
 * over the panel and paints nothing at all on a phone. [Fui.GridLine] is the
 * alpha that actually renders, checked on the emulator.
 *
 * Scanlines are not here: they cost the most contrast and belong only on a
 * chrome band, never behind an Entry's Preview.
 */
fun Modifier.fuiBackdrop(): Modifier = drawBehind {
    val step = 32.dp.toPx()
    val line = Fui.GridLine
    var x = 0f
    while (x < size.width) {
        drawRect(line, topLeft = Offset(x, 0f), size = Size(1f, size.height))
        x += step
    }
    var y = 0f
    while (y < size.height) {
        drawRect(line, topLeft = Offset(0f, y), size = Size(size.width, 1f))
        y += step
    }
    drawRect(
        Brush.radialGradient(
            0.4f to Color.Transparent,
            1.0f to Fui.Void1000.copy(alpha = 0.85f * 0.35f),
            center = Offset(size.width / 2f, size.height * 0.4f),
            radius = maxOf(size.width, size.height) * 0.9f,
        ),
    )
}

/**
 * Scanlines, for chrome only.
 *
 * A 1px darkening on every third row. Measured at ~9% of contrast on the
 * desktop, which is why nothing carrying an Entry's Preview gets it.
 */
fun Modifier.fuiScanlines(): Modifier = drawBehind {
    val ink = Color.Black.copy(alpha = 0.25f)
    var y = 2f
    while (y < size.height) {
        drawRect(ink, topLeft = Offset(0f, y), size = Size(size.width, 1f))
        y += 3f
    }
}

/**
 * A dashed outline, for a slot that is empty on purpose.
 *
 * The two camera failures use it: the viewfinder is missing rather than broken,
 * and a solid frame around the explanation would read as an alert when the point
 * is that the typed path is right there and works just as well.
 */
fun Modifier.dashedBorder(color: Color): Modifier = drawBehind {
    drawRect(
        color = color,
        style = Stroke(
            width = 1.dp.toPx(),
            pathEffect = PathEffect.dashPathEffect(floatArrayOf(6.dp.toPx(), 5.dp.toPx())),
        ),
    )
}

/** A one-pixel rule in the emitter's faintest tint. */
@Composable
fun Hairline(modifier: Modifier = Modifier, color: Color = Fui.Hairline) {
    Box(modifier.fillMaxWidth().height(1.dp).background(color))
}

/**
 * A fixed-height band of chrome, with the rule that separates it from the next.
 *
 * Chrome is what cannot scroll away. Everything the History pins above its list
 * is one of these, which is what makes "140dp of chrome" a number rather than an
 * accident.
 */
@Composable
fun ChromeBand(
    height: Dp,
    modifier: Modifier = Modifier,
    background: Color = Fui.Band,
    scanlines: Boolean = false,
    content: @Composable RowScope.() -> Unit,
) {
    Column(modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(height)
                .background(background)
                .then(if (scanlines) Modifier.fuiScanlines() else Modifier)
                .padding(horizontal = Fui.Gutter),
            verticalAlignment = Alignment.CenterVertically,
            content = content,
        )
        Hairline()
    }
}

/**
 * What a status light can say.
 *
 * The mock's own four lamp states, kept in its words. Not to be confused with
 * [Tone], which answers a different question about the same [SessionPhase]:
 * `Tone` is whether the person has to *act*, and only [Alert] here is a `Fault`
 * there. `Signal.Nominal` is narrower than `Tone.Nominal` — it means in contact,
 * while three of `Tone`'s nominal phases light [Standby].
 */
enum class Signal(val colour: Color) {
    /** In contact with the Relay. The exceptional state on a phone. */
    Nominal(Fui.Nominal400),

    /** Work in progress: a session coming up, a code with a clock on it. */
    Caution(Fui.Amber400),

    /** Not in contact, or not the Active Pairing. Nominal on a phone. */
    Standby(Fui.Standby400),

    /** The revoked token. The one lamp that is a fault, and the only red. */
    Alert(Fui.Alert400),
}

/**
 * A lit square and a caps label: the phone's one telemetry idiom.
 *
 * A square rather than a dot, because a HUD does not draw circles and because a
 * 6dp square survives a low-density screen that eats a 6dp circle's edges.
 */
@Composable
fun StatusLight(signal: Signal, label: String, modifier: Modifier = Modifier) {
    Row(
        // One node, not a lamp and a caption. A screen reader should say the
        // reading once, and a test asking this readout what it says should get a
        // sentence rather than a list of two children.
        modifier = modifier.semantics(mergeDescendants = true) {},
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Box(Modifier.size(6.dp).background(signal.colour))
        Text(
            text = label,
            style = Fui.Micro,
            color = if (signal == Signal.Alert) Fui.Alert400 else Fui.TextMuted,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** How a badge or a button is pitched. */
enum class Accent(val ink: Color, val edge: Color, val fill: Color) {
    /** The emitter. What the person came here to press. */
    Emitter(Fui.Cyan300, Fui.CyanA40, Fui.Cyan400),

    /** Present, not pushed. Never solid — a neutral emitter is a contradiction. */
    Neutral(Fui.TextBody, Fui.Inert, Fui.Inert),

    /** A queue, a clock, a Recall that may be stale. */
    Caution(Fui.Amber400, Fui.AmberA40, Fui.Amber400),

    /** Cannot be undone. */
    Alert(Fui.Alert400, Fui.AlertA40, Fui.Alert500),
}

/** A small stated fact — SYNCING, SHOWING, CANNOT BE UNDONE. */
@Composable
fun FuiBadge(text: String, accent: Accent, solid: Boolean = false, modifier: Modifier = Modifier) {
    Box(
        modifier = modifier
            .clip(notched)
            .then(if (solid) Modifier.background(accent.fill) else Modifier.border(1.dp, accent.edge, notched))
            .padding(horizontal = 7.dp, vertical = 3.dp),
    ) {
        Text(
            text = text,
            style = Fui.Micro,
            color = if (solid) Fui.OnEmitter else accent.ink,
            maxLines = 1,
        )
    }
}

/**
 * A chip for something that is not there and is not coming.
 *
 * The three N/A chips on the settings are the whole reason this exists: a
 * missing switch and an unbuilt screen look identical, and a chip that says
 * `WATCHED CAPTURE · N/A` is the difference between a decision and an omission.
 */
@Composable
fun FuiTag(text: String, modifier: Modifier = Modifier, inert: Boolean = false) {
    Box(
        modifier = modifier
            .height(24.dp)
            .border(1.dp, if (inert) Fui.Inert else Fui.Hairline)
            .padding(horizontal = 8.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, style = Fui.Micro, color = if (inert) Fui.TextDim else Fui.TextMuted, maxLines = 1)
    }
}

/**
 * A verb.
 *
 * [solid] fills with the accent and is reserved for the one verb on a surface that
 * outranks the others — Recall Latest on the History, Pair this phone on the
 * pairing flow, Forget inside an armed erase. Two solids on one screen is two
 * emitters, which is none.
 */
@Composable
fun FuiButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    accent: Accent = Accent.Emitter,
    solid: Boolean = false,
    enabled: Boolean = true,
    height: Dp = Fui.Target,
) {
    val ink = when {
        !enabled -> Fui.TextDim
        // Every solid fill takes the void as its ink, alert included. The mock
        // puts `--cyan-100` on `--alert-500`, which measures 3.1:1 at 11sp and
        // fails AA; the void measures 5.0:1 on the same fill. Same correction,
        // same reason, as the text ramp at the top of this file.
        solid -> Fui.OnEmitter
        else -> accent.ink
    }
    Box(
        modifier = modifier
            .height(height)
            .clip(notched)
            .then(
                when {
                    !enabled -> Modifier.border(1.dp, Fui.Inert, notched)
                    solid -> Modifier.background(accent.fill)
                    else -> Modifier.border(1.dp, accent.edge, notched)
                },
            )
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 16.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, style = Fui.Label, color = ink, maxLines = 1)
    }
}

/**
 * A glyph on a square target.
 *
 * 48dp whether or not the glyph fills it. Recall lives on one of these on every
 * row, and a row's worth of thumb is the reason the row has only one control.
 */
@Composable
fun GlyphButton(
    glyph: String,
    onClick: () -> Unit,
    contentDescription: String,
    modifier: Modifier = Modifier,
    accent: Accent = Accent.Emitter,
    enabled: Boolean = true,
) {
    Box(
        modifier = modifier
            .size(Fui.Target)
            .clip(notched)
            .border(1.dp, if (enabled) accent.edge else Fui.Inert, notched)
            // The glyph is a picture of the verb, not its name. Without this the
            // accessible name of every row's Recall is "↓".
            .semantics { this.contentDescription = contentDescription }
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = glyph,
            style = Fui.Glyph,
            color = if (enabled) accent.ink else Fui.TextDim,
            modifier = Modifier.clearAndSetSemantics {},
        )
    }
}

/**
 * A text field in the console's own voice.
 *
 * Material's outlined field underneath, because a hand-rolled one would owe the
 * platform a cursor, a selection handle, an IME contract and an accessibility
 * tree. Only its colours, shape and type are ours.
 *
 * [label] is nullable because one caller cannot afford it. Material floats the
 * label above the value and charges the height for it either way; the History's
 * Filter is a 56dp chrome band and has nowhere to put it. A field with no label
 * has no accessible name either, which is why [contentDescription] exists beside
 * it — a placeholder disappears the moment somebody types, and a TalkBack user
 * who has typed is exactly the one who needs telling which field they are in.
 *
 * [leading] and [trailing] are Material's icon slots under the names they are
 * being used for: the Filter puts a glyph in one and a count with a `✕` in the
 * other, and neither is an icon.
 */
@Composable
fun FuiTextField(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    label: String? = null,
    placeholder: String? = null,
    contentDescription: String? = null,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    leading: @Composable (() -> Unit)? = null,
    trailing: @Composable (() -> Unit)? = null,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        label = label?.let { { Text(it, style = Fui.Micro) } },
        placeholder = placeholder?.let { { Text(it, style = Fui.Data, color = Fui.TextDim) } },
        leadingIcon = leading,
        trailingIcon = trailing,
        singleLine = true,
        shape = RectangleShape,
        textStyle = Fui.Data,
        keyboardOptions = keyboardOptions,
        colors = OutlinedTextFieldDefaults.colors(
            focusedTextColor = Fui.TextPrimary,
            unfocusedTextColor = Fui.TextBody,
            focusedBorderColor = Fui.Cyan400,
            unfocusedBorderColor = Fui.Frame,
            focusedLabelColor = Fui.TextEmitter,
            unfocusedLabelColor = Fui.TextMuted,
            cursorColor = Fui.Cyan400,
            focusedContainerColor = Fui.Recess,
            unfocusedContainerColor = Fui.Recess,
        ),
        modifier = modifier
            .fillMaxWidth()
            .then(
                if (contentDescription == null) {
                    Modifier
                } else {
                    Modifier.semantics { this.contentDescription = contentDescription }
                },
            ),
    )
}

/**
 * A framed panel with a header strip: a Pairing card, the viewfinder.
 *
 * [title] is the thing itself and [code] is its address — for a Pairing that is
 * the **User** and then the relay host, in that order, because heading a Pairing
 * with this machine's own Device Label is the desktop's mistake and the card is
 * where it was made. The User's id is not repeated in the address: the heading
 * already carries the User, and the repetition is what pushed the host off the
 * end of the line.
 */
@Composable
fun FuiPanel(
    title: String,
    modifier: Modifier = Modifier,
    code: String? = null,
    accent: Accent = Accent.Emitter,
    trailing: @Composable RowScope.() -> Unit = {},
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .border(1.dp, if (accent == Accent.Alert) Fui.AlertA40 else Fui.Frame)
            .background(Fui.Raised),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .background(if (accent == Accent.Alert) Fui.AlertA16 else Fui.CyanA08)
                .padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Column(Modifier.weight(1f)) {
                Text(
                    text = title,
                    style = Fui.Heading,
                    color = if (accent == Accent.Alert) Fui.Alert400 else Fui.TextPrimary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (code != null) {
                    Text(
                        text = code,
                        style = Fui.Micro,
                        color = Fui.TextMuted,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            trailing()
        }
        Hairline()
        Column(Modifier.padding(12.dp), content = content)
    }
}

/**
 * A quoted note: a rule left of a sentence, no container around it.
 *
 * For the two settings sentences and the pairing explainers — prose long enough
 * that a box would read as an alert and short enough that a heading would be
 * heavier than the thing it heads.
 */
@Composable
fun QuotedNote(text: String, modifier: Modifier = Modifier) {
    Row(
        modifier = modifier.fillMaxWidth().height(IntrinsicSize.Min)
            // The rule is decoration; the sentence is the note. Merged so that
            // asking this note what it says returns the sentence.
            .semantics(mergeDescendants = true) {},
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(Modifier.width(2.dp).fillMaxHeight().background(Fui.Hairline))
        Text(text, style = Fui.Prose, color = Fui.TextBody, modifier = Modifier.weight(1f))
    }
}
