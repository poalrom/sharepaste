/**
 * The window's only teaching surface. Someone who paired a minute ago has
 * never seen these bindings and will not go looking for them, so the strip
 * spells the action out as a verb instead of abbreviating it: the reader is
 * by definition the person who does not yet know what the abbreviation meant.
 */

const IS_MAC = /mac|iphone|ipad|ipod/i.test(navigator.platform || navigator.userAgent);

/**
 * Each platform's keys, named the way that platform's own keycaps are. A
 * Windows user does not read `⌘` or `⌫`, and a mac user has no key labelled
 * BKSP - the one with `⌫` on it is called Delete. Windows joins a combination
 * with `+` because its own shortcut notation does; mac butts the glyphs up.
 */
const KEY = IS_MAC
  ? { mod: "⌘", shift: "⇧", enter: "⏎", back: "⌫", join: "" }
  : { mod: "CTRL", shift: "SHIFT", enter: "ENTER", back: "BKSP", join: "+" };

/* Each platform prints its modifier chain in its own order too: macOS puts ⇧
   before ⌘, Windows names CTRL first. */
const DELETE_KEYS = IS_MAC ? [KEY.shift, KEY.mod, KEY.back] : [KEY.mod, KEY.shift, KEY.back];

/*
  Arrow-key navigation is deliberately absent. It is the one binding every
  user already expects of a list with a highlighted row, and at 360px its
  hint cost 55px - which crushed the three bindings that do need teaching
  into a line that wrapped and clipped. Same reasoning as ADR 0002: chrome
  that only restates the obvious does not get to cost space.

  `⌘⌫` clearing the Filter is absent for both reasons at once: it is what a
  focused text field does on either platform (ADR 0013), and a fourth hint
  does not fit the line the third one already fills.
*/
const HINTS = [
  { keys: [KEY.enter], action: "COPY" },
  { keys: [KEY.mod, KEY.enter], action: "KEEP OPEN" },
  { keys: DELETE_KEYS, action: "DELETE" },
];

export default function HintStrip() {
  return (
    <div
      data-testid="hint-strip"
      className="fui-band flex h-5 shrink-0 items-center justify-between border-t border-hairline px-3"
    >
      {/*
        Tighter than the main window's copy of this row, and only here: at 360px
        with `px-3` the widest platform's three hints measure 340.9px against
        336px of line — `CTRL+SHIFT+BKSP` is 35px wider than the `CTRL+BKSP` it
        replaced. A 4px gap and a 2px keycap inset buy 12px back and leave 7px
        spare, which is what keeps the line from wrapping and clipping the way
        the arrow hint above did. The main window's row has a whole 980px and
        keeps the roomier spacing.
      */}
      {HINTS.map((hint) => (
        <span key={hint.action} className="flex items-center gap-1">
          <kbd className="fui-key px-0.5">{hint.keys.join(KEY.join)}</kbd>
          <span className="text-label tracking-phrase text-text-muted">{hint.action}</span>
        </span>
      ))}
    </div>
  );
}
