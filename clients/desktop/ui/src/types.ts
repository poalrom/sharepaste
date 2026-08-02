export type ConnectionState = "Disconnected" | "Connecting" | "Online" | "AuthFailed";

/**
 * One entry as `list_history` returns it and `entry-added` carries it.
 *
 * `preview` is the Preview on both paths: one line, control characters
 * flattened, capped, built by the core. Rows render it as it arrives.
 *
 * `plaintext` is the whole text — what the reader pane renders (ADR 0003), what
 * the Filter matches, so a word on an entry's third line still narrows the
 * history, and what the header's byte count measures. `null` only for an
 * Undecryptable entry.
 *
 * `undecryptable` is stated by the backend and must never be re-derived. An
 * entry whose plaintext is genuinely empty is indistinguishable from one this
 * device holds no key for to anything guessing from an empty `preview`.
 *
 * `last_use` is the moment of the entry's most recent Use — capturing it,
 * recalling it, or copying its text again on a device that already holds it —
 * and it is the only fact the list is ordered by (ADR 0011). It equals
 * `created_at` for an entry never used since capture: no history of uses
 * exists to say otherwise, and that is the truth about such an entry rather
 * than a gap in it.
 *
 * `origin_label` is the Device Label or a slice of the Device id, resolved by
 * the core so the phone and this window cannot disagree about it.
 */
export type EntryView = {
  id: number;
  user_id: string;
  preview: string;
  plaintext: string | null;
  created_at: number;
  last_use: number;
  device_id: string;
  device_label?: string;
  origin_label: string;
  undecryptable: boolean;
};

export type Pairing = {
  user_id: string;
  device_id: string;
  label: string;
  /** The User's name on the relay, mirrored by `GET /me`; absent until first contact. */
  username?: string | null;
  server_url: string;
  /** The relay as a person reads it: host and port, no scheme, no credentials. */
  relay_host: string;
  status: ConnectionState;
  pending: number;
  is_active: boolean;
};

/**
 * What the relay connection knows about itself, out of band from the entries.
 *
 * `last_contact_at` is the epoch-ms of the most recent byte received from the
 * relay — heartbeat comments included — or null if this device has never
 * reached it.
 */
export type Contact = {
  user_id: string;
  last_contact_at: number | null;
};

export type Settings = {
  capture_enabled: boolean;
  deny_list: string[];
  autostart: boolean;
  hotkey?: string | null;
  update_check_enabled: boolean;
};

/** A Release the Update Source is offering and this device does not have. */
export type UpdateAvailable = {
  version: string;
  /** The changelog section the pipeline put in `latest.json`. */
  notes?: string | null;
};

/**
 * What this device knows about releases right now.
 *
 * `available` reflects the last check only; reading it costs no request, which
 * is what lets the Settings pane render while the automatic check is off.
 */
export type UpdateStatus = {
  current_version: string;
  available?: UpdateAvailable | null;
};

export type AppErrorPayload = { kind: string; message: string };
