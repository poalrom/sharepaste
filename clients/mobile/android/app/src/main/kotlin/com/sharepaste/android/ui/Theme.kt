package com.sharepaste.android.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable

/**
 * One theme, dark, and the same one the desktop wears.
 *
 * This used to take the device's own Material You scheme and argue that a
 * utility holding someone's clipboard has no business having a brand colour.
 * That argument was right about brand and wrong about kind: the desktop popover
 * and main window are a FUI console, and a phone showing the same Entries in
 * whatever the wallpaper suggested is not neutral, it is a third client that
 * looks like neither of the other two. The palette is in [Fui], ported from
 * `clients/desktop/ui/src/styles.css` so both clients read the same value for
 * the same idea.
 *
 * No light scheme and no `isSystemInDarkTheme`. A HUD is emitted light on a
 * void; there is no light-mode rendering of it that is the same design, and
 * offering one would mean a second contrast audit for a surface nobody asked
 * for. `themes.xml` paints the window the same void so the first frame does not
 * flash.
 *
 * Material 3 is still underneath, because `OutlinedTextField` and its friends
 * read [MaterialTheme]. The scheme below is what those components resolve
 * against; everything this app draws itself reaches for [Fui] directly, which
 * is the only way a token stays one value rather than a mapping onto Material's
 * twenty-nine roles.
 */
@Composable
fun SharepasteTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = scheme, typography = typography, content = content)
}

private val scheme = darkColorScheme(
    primary = Fui.Cyan400,
    onPrimary = Fui.OnEmitter,
    primaryContainer = Fui.CyanA12,
    onPrimaryContainer = Fui.TextPrimary,
    secondary = Fui.Cyan600,
    onSecondary = Fui.OnEmitter,
    secondaryContainer = Fui.Active,
    onSecondaryContainer = Fui.TextPrimary,
    tertiary = Fui.Amber400,
    onTertiary = Fui.OnEmitter,
    tertiaryContainer = Fui.AmberA16,
    onTertiaryContainer = Fui.TextPrimary,
    background = Fui.Panel,
    onBackground = Fui.TextBody,
    surface = Fui.Panel,
    onSurface = Fui.TextBody,
    surfaceVariant = Fui.Band,
    onSurfaceVariant = Fui.TextMuted,
    surfaceContainerHighest = Fui.Raised,
    outline = Fui.Frame,
    outlineVariant = Fui.Hairline,
    error = Fui.Alert400,
    onError = Fui.OnEmitter,
    errorContainer = Fui.AlertA16,
    onErrorContainer = Fui.TextPrimary,
)

/**
 * Material's type scale, pointed at [Fui]'s.
 *
 * Only the roles Material components actually resolve — a text field's label and
 * its input, a dialog's body. Everything this app draws names its style outright.
 */
private val typography = Typography(
    bodyLarge = Fui.Data,
    bodyMedium = Fui.Prose,
    bodySmall = Fui.Micro,
    titleLarge = Fui.Heading,
    titleMedium = Fui.Subheading,
    titleSmall = Fui.Subheading,
    labelLarge = Fui.Label,
    labelMedium = Fui.Label,
    labelSmall = Fui.Micro,
)
