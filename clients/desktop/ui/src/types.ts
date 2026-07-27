export type ConnectionState = "Disconnected" | "Connecting" | "Online" | "AuthFailed";

export type EntryView = {
  id: number;
  user_id: string;
  preview: string;
  created_at: number;
  device_id: string;
  device_label?: string;
};

export type Pairing = {
  user_id: string;
  device_id: string;
  label: string;
  /** The User's name on the relay, mirrored by `GET /me`; absent until first contact. */
  username?: string | null;
  server_url: string;
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
