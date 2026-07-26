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
