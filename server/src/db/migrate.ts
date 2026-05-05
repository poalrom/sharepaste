import type { Db } from "./index.js";

const SCHEMA = `
CREATE TABLE IF NOT EXISTS users (
  id          TEXT PRIMARY KEY,
  username    TEXT UNIQUE NOT NULL,
  created_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS invites (
  token_hash  TEXT PRIMARY KEY,
  user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  expires_at  INTEGER NOT NULL,
  claimed_at  INTEGER
);

CREATE TABLE IF NOT EXISTS memberships (
  user_id            TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  device_id          TEXT NOT NULL,
  device_token_hash  TEXT NOT NULL,
  device_label       TEXT,
  created_at         INTEGER NOT NULL,
  revoked_at         INTEGER,
  PRIMARY KEY (user_id, device_id)
);
CREATE INDEX IF NOT EXISTS memberships_token_hash
  ON memberships (device_token_hash);

CREATE TABLE IF NOT EXISTS pairings (
  id                  TEXT PRIMARY KEY,
  user_id             TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  secret_hash         TEXT NOT NULL,
  encrypted_payload   BLOB,
  claimed_by          TEXT,
  paired_device_label TEXT,
  failed_attempts     INTEGER NOT NULL DEFAULT 0,
  consumed_at         INTEGER,
  expires_at          INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS entries (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  device_id   TEXT NOT NULL,
  ciphertext  BLOB NOT NULL,
  size        INTEGER NOT NULL,
  created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS entries_user_id_id ON entries (user_id, id);
`;

export const migrate = (db: Db): void => {
  db.exec(SCHEMA);
  const pairingColumns = db.prepare("PRAGMA table_info(pairings)").all() as Array<{ name: string }>;
  if (!pairingColumns.some((column) => column.name === "paired_device_label")) {
    db.exec("ALTER TABLE pairings ADD COLUMN paired_device_label TEXT");
  }
};
