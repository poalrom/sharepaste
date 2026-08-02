import type { Db } from "./index.js";

const SCHEMA = `
CREATE TABLE IF NOT EXISTS users (
  id          TEXT PRIMARY KEY,
  username    TEXT UNIQUE NOT NULL,
  created_at  INTEGER NOT NULL,
  next_seq    INTEGER NOT NULL DEFAULT 0
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
  token_sha256       TEXT,
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
  claimed_at          INTEGER,
  paired_device_label TEXT,
  failed_attempts     INTEGER NOT NULL DEFAULT 0,
  consumed_at         INTEGER,
  expires_at          INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS entries (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  device_id      TEXT NOT NULL,
  ciphertext_b64 TEXT NOT NULL,
  size           INTEGER NOT NULL,
  created_at     INTEGER NOT NULL,
  -- DEFAULT 0 matches what the ALTER path below can produce: SQLite refuses to
  -- add a NOT NULL column without a default, so an installed database would
  -- otherwise end at a different shape from a fresh one.
  last_use       INTEGER NOT NULL DEFAULT 0,
  seq            INTEGER NOT NULL DEFAULT 0
);
`;

const columnsOf = (db: Db, table: string): Set<string> => {
  const info = db.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>;
  return new Set(info.map((column) => column.name));
};

/**
 * Brings an existing database up to the schema above.
 *
 * SQLite has no schema-version bookkeeping here; each step is guarded by the
 * columns actually present, so `migrate` is idempotent and safe to run at every
 * boot. Fresh databases get the final shape straight from SCHEMA and every
 * guarded step below is a no-op.
 */
export const migrate = (db: Db): void => {
  db.exec(SCHEMA);

  const pairings = columnsOf(db, "pairings");
  if (!pairings.has("paired_device_label")) {
    db.exec("ALTER TABLE pairings ADD COLUMN paired_device_label TEXT");
  }
  if (!pairings.has("claimed_at")) {
    db.exec("ALTER TABLE pairings ADD COLUMN claimed_at INTEGER");
    // `claimed_by` stored sha256(secret_proof), which the claim handler had just
    // proven equal to `secret_hash` — it only ever carried "is claimed". The exact
    // claim instant was never recorded, so approximate it with the slot deadline.
    if (pairings.has("claimed_by")) {
      db.exec("UPDATE pairings SET claimed_at = expires_at WHERE claimed_by IS NOT NULL");
    }
  }
  if (pairings.has("claimed_by")) {
    db.exec("ALTER TABLE pairings DROP COLUMN claimed_by");
  }

  const memberships = columnsOf(db, "memberships");
  if (!memberships.has("token_sha256")) {
    db.exec("ALTER TABLE memberships ADD COLUMN token_sha256 TEXT");
  }
  db.exec(
    `CREATE UNIQUE INDEX IF NOT EXISTS memberships_token_sha256
     ON memberships (token_sha256) WHERE token_sha256 IS NOT NULL`
  );

  const entries = columnsOf(db, "entries");
  if (!entries.has("ciphertext_b64")) {
    db.exec("ALTER TABLE entries ADD COLUMN ciphertext_b64 TEXT");
  }
  if (entries.has("ciphertext")) {
    const rows = db
      .prepare("SELECT id, ciphertext FROM entries WHERE ciphertext_b64 IS NULL")
      .all() as Array<{ id: number; ciphertext: Buffer }>;
    const update = db.prepare("UPDATE entries SET ciphertext_b64 = ? WHERE id = ?");
    db.transaction(() => {
      for (const row of rows) update.run(row.ciphertext.toString("base64"), row.id);
    })();
    db.exec("ALTER TABLE entries DROP COLUMN ciphertext");
  }
  if (!entries.has("last_use")) {
    // SQLite refuses to add a NOT NULL column without a default, so the column
    // arrives at 0 and the backfill is what makes the declared shape true.
    db.exec("ALTER TABLE entries ADD COLUMN last_use INTEGER NOT NULL DEFAULT 0");
    db.exec("UPDATE entries SET last_use = created_at WHERE last_use = 0");
  }
  if (!entries.has("seq")) {
    db.exec("ALTER TABLE entries ADD COLUMN seq INTEGER NOT NULL DEFAULT 0");
    // Seeding from `id` is what keeps every watermark already stored on every
    // installed client valid: ids are globally monotonic, so a per-user sequence
    // seeded from them starts above anything that client has already fetched.
    db.exec("UPDATE entries SET seq = id WHERE seq = 0");
  }
  if (!columnsOf(db, "users").has("next_seq")) {
    db.exec("ALTER TABLE users ADD COLUMN next_seq INTEGER NOT NULL DEFAULT 0");
  }
  // After the seq backfill, and every boot: the counter has to start above every
  // sequence already handed out, and it is what makes a sequence unrepeatable
  // once a delete has taken the row wearing it. Idempotent because it only ever
  // raises the counter.
  db.exec(
    `UPDATE users SET next_seq = MAX(
       next_seq,
       COALESCE((SELECT MAX(seq) FROM entries WHERE entries.user_id = users.id), 0)
     )`
  );
  // These indexes live here rather than in SCHEMA because `db.exec(SCHEMA)` runs
  // first, and on an installed database the columns they name do not exist yet.
  db.exec("DROP INDEX IF EXISTS entries_user_id_id");
  db.exec("CREATE UNIQUE INDEX IF NOT EXISTS entries_user_id_seq ON entries (user_id, seq)");
  db.exec(
    `CREATE INDEX IF NOT EXISTS entries_user_last_use
     ON entries (user_id, last_use DESC, id DESC)`
  );
};
