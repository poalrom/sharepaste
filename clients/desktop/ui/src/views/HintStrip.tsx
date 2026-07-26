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
  ? { mod: "⌘", enter: "⏎", back: "⌫", join: "" }
  : { mod: "CTRL", enter: "ENTER", back: "BKSP", join: "+" };

/*
  Arrow-key navigation is deliberately absent. It is the one binding every
  user already expects of a list with a highlighted row, and at 360px its
  hint cost 55px - which crushed the three bindings that do need teaching
  into a line that wrapped and clipped. Same reasoning as ADR 0002: chrome
  that only restates the obvious does not get to cost space.
*/
const HINTS = [
  { keys: [KEY.enter], action: "COPY" },
  { keys: [KEY.mod, KEY.enter], action: "KEEP OPEN" },
  { keys: [KEY.mod, KEY.back], action: "DELETE" },
];

export default function HintStrip() {
  return (
    <div
      data-testid="hint-strip"
      className="fui-band flex h-5 shrink-0 items-center justify-between border-t border-hairline px-3"
    >
      {HINTS.map((hint) => (
        <span key={hint.action} className="flex items-center gap-1.5">
          <kbd className="fui-key">{hint.keys.join(KEY.join)}</kbd>
          <span className="text-label tracking-phrase text-text-muted">{hint.action}</span>
        </span>
      ))}
    </div>
  );
}
