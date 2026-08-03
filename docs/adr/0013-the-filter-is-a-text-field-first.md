# The filter is a text field first

Both desktop surfaces put a **Filter** above the History list and keep it focused
essentially always: the popover refocuses it on every window `focus` because the window is
shown and hidden rather than unmounted (`clients/desktop/ui/src/views/Filter.tsx:22-34`),
and the main window autofocuses it on mount with nothing else in the pane claiming focus
on its own. [ADR 0002](0002-popover-is-a-picker.md) and
[ADR 0003](0003-the-main-window-reads.md) both reasoned from that and landed on the same
binding: deleting an Entry needs a modifier, so `⌘⌫` / `Ctrl+⌫` deletes.

The modifier did not make the key free. Inside a focused text field, `⌘⌫` is macOS's
*erase to the start of the line* and `Ctrl+⌫` is Windows' *erase the previous word* — text
editing the platform has already promised to the field the caret is sitting in, in every
other app the person uses. Taking them for the list meant that erasing a query destroyed
an Entry instead: unguarded by decision (`popover-redesign.md` §0.13 leaves the
destructive path unconfirmed, with the modifier as its only guard), and at the exact
moment the user's attention was on the text rather than the rows.

The same mistake in a quieter register: the field was left to the platform's text
services, so macOS read a needle as prose. A query of `tail` floated a `Tail ×` correction
bubble over the first row, offering to fix a spelling nobody was writing.

So both go back to the field. The unshifted combination clears the query, deleting adds
`⇧` — `⇧⌘⌫` on a Mac, `CTRL+SHIFT+BKSP` on Windows, which neither platform reserves for
editing text — and the input declines spellcheck, autocorrect and autocapitalisation.

## Considered Options

**Keep `⌘⌫` on delete and treat the collision as the price of a focused filter.** Rejected:
the collision is not a matter of taste about which binding is nicer. One of the two
meanings erases characters and the other erases an Entry from every paired device, and the
person pressing the key learned the first meaning from their operating system.

**Clear when the query is non-empty, delete when it is empty.** Rejected, and it was the
tempting one: it keeps the old binding reachable with no new keys and no new hint. But it
deletes an Entry on the one press where the user could see nothing happen — an empty field
is exactly when `⌘⌫` feels like a no-op — and it makes one keystroke mean two things
depending on a state the eye does not check before pressing.

**Drop the keyboard delete entirely,** leaving `✕` on the row and in the reading pane.
Rejected: the popover exists to be driven without the mouse, and `⇧` costs one finger.

**Bare `Delete` / `Backspace`,** as both design mocks drew it. Still rejected, for the
reason ADR 0003 gave: the filter holds focus, so a bare key either collides with editing
or never fires.

## Consequences

**Clearing is done here, not left to the browser.** The pane's own keydown handler calls
`preventDefault()` and empties the query, because the two platforms' native erases differ
from each other and neither of them is "clear the filter". It is the same act the
`CLEAR FILTER` button offers from the `NO MATCHES` panel, now reachable without leaving the
keyboard.

**Neither surface teaches the clear, and both restate the delete.** A hint for `⌘⌫` would
be chrome that only restates what the platform's own text fields do, which is the test
ADR 0002 applies. `DELETE` keeps its hint and now prints the wider chain.

**The popover's hint strip is 12px tighter — only the popover's.** `CTRL+SHIFT+BKSP` is
35px wider than the `CTRL+BKSP` it replaces, which put the Windows strip at 340.9px against
336px of line: the wrap-and-clip failure ADR 0002 cut the arrow hint to avoid. Measured at
360px, a 4px gap and a 2px keycap inset in place of 6px and 3px bring the three hints back
to 328.9px, leaving 7px spare. The main window's copy of that row has 980px and keeps the
roomier spacing, which is why the two were never one shared component.

**A dictation-free field.** `spellCheck={false}`, `autoCorrect="off"` and
`autoCapitalize="off"` on both filters. Nothing typed there is prose: it is a substring of
a command, a path or an `ss://` URL, matched against `plaintext`. The Android filter
already declined the same services for the same reason (`autoCorrectEnabled = false`,
`KeyboardCapitalization.None`) — for correctness, not for secrecy; nothing is recorded
under **R3** either way.
