const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;
const MONTH = 30 * DAY;

/**
 * A coarse, single-token age — `now`, `4m`, `6h`, `3d`, `2mo`.
 *
 * Truncating rather than rounding keeps the label from ever claiming an entry
 * is older than it is, and a timestamp ahead of `now` (clock skew between two
 * paired devices) clamps to `now` rather than rendering a negative age.
 */
export function relativeAge(createdAt: number, now: number): string {
  const elapsed = Math.max(0, now - createdAt);
  if (elapsed < MINUTE) return "now";
  if (elapsed < HOUR) return `${Math.floor(elapsed / MINUTE)}m`;
  if (elapsed < DAY) return `${Math.floor(elapsed / HOUR)}h`;
  if (elapsed < MONTH) return `${Math.floor(elapsed / DAY)}d`;
  return `${Math.floor(elapsed / MONTH)}mo`;
}

/**
 * An age as a whole phrase — `now`, `4m ago`, `3d ago`.
 *
 * Every surface that suffixes `relativeAge` with "ago" has to special-case the
 * sub-minute reading, which is already a complete answer: "now ago" is not
 * English. Doing it in one place is how four surfaces stop getting it wrong
 * independently.
 *
 * Lower-case on purpose. Callers render it inside an upper-case chrome band
 * with `normal-case`, because `relativeAge`'s `m` (minutes) and `mo` (months)
 * stop being distinguishable once they are shouted.
 */
export function agePhrase(at: number, now: number): string {
  const age = relativeAge(at, now);
  return age === "now" ? age : `${age} ago`;
}

/**
 * Flattens an entry's plaintext into one bounded line.
 *
 * `preview` is the full plaintext, so without this an indented or multi-line
 * entry renders as leading whitespace — a visually empty row — and up to 100
 * unbounded strings enter the DOM at once.
 */
export function normalizePreview(plaintext: string, limit = 200): string {
  return plaintext.replace(/\s+/g, " ").trim().slice(0, limit);
}

/**
 * The name of the device an entry came from.
 *
 * Memberships paired before Device Labels were mirrored carry no label, so the
 * row falls back to a device-id slice; callers put the full id in `title`.
 */
export function originLabel(deviceLabel: string | null | undefined, deviceId: string): string {
  const trimmed = deviceLabel?.trim();
  return trimmed ? trimmed : deviceId.slice(0, 4);
}

/**
 * The relay as a person names it.
 *
 * A Pairing is identified by User-on-Relay, and that pair has to fit a footer
 * and a card heading; the scheme and trailing slash carry no information the
 * reader is using to tell two relays apart.
 */
export function relayHost(serverUrl: string): string {
  try {
    return new URL(serverUrl).host;
  } catch {
    return serverUrl.replace(/^https?:\/\//, "").replace(/\/+$/, "");
  }
}

/**
 * A byte count as the reading pane states it.
 *
 * Nothing caps the size of an entry — not capture, not the cache, not the
 * relay — so this is the only thing on screen that explains a monstrous one.
 */
export function byteSize(text: string): string {
  const bytes = new TextEncoder().encode(text).length;
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * When an entry was captured, as a clock time for today and a date before that.
 *
 * The relative age answers "how stale is this?"; this answers "which of the
 * three things I copied this afternoon is it?", which the age cannot at
 * one-hour resolution.
 */
export function capturedAt(createdAt: number, now: number): string {
  const d = new Date(createdAt);
  const time = d.toTimeString().slice(0, 8);
  const sameDay = new Date(now).toDateString() === d.toDateString();
  return sameDay ? time : `${d.toISOString().slice(0, 10)} ${time}`;
}
