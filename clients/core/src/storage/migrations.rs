use crate::errors::AppError;
use rusqlite::Connection;
use std::collections::HashSet;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS accounts (
  user_id        TEXT PRIMARY KEY,
  device_id      TEXT NOT NULL,
  device_label   TEXT NOT NULL,
  server_url     TEXT NOT NULL,
  last_seen_seq  INTEGER NOT NULL DEFAULT 0,
  created_at     INTEGER NOT NULL,
  username       TEXT,
  last_contact_at INTEGER
);

CREATE TABLE IF NOT EXISTS devices (
  user_id    TEXT NOT NULL,
  device_id  TEXT NOT NULL,
  label      TEXT,
  revoked_at INTEGER,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (user_id, device_id)
);

CREATE TABLE IF NOT EXISTS entries_cache (
  user_id          TEXT NOT NULL,
  id               INTEGER NOT NULL,
  ciphertext       BLOB NOT NULL,
  plaintext        TEXT,
  plaintext_sha256 TEXT,
  created_at       INTEGER NOT NULL,
  last_use         INTEGER NOT NULL DEFAULT 0,
  device_id        TEXT NOT NULL,
  PRIMARY KEY (user_id, id)
);

CREATE TABLE IF NOT EXISTS pending_uploads (
  rowid            INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id          TEXT NOT NULL,
  kind             TEXT NOT NULL,
  entry_id         INTEGER,
  ciphertext       BLOB,
  plaintext_sha256 TEXT,
  captured_at      INTEGER NOT NULL,
  attempts         INTEGER NOT NULL DEFAULT 0,
  last_error       TEXT
);
CREATE INDEX IF NOT EXISTS pending_uploads_user_id_rowid ON pending_uploads (user_id, rowid);

CREATE TABLE IF NOT EXISTS settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
"#;

/// Columns `accounts` grew after it shipped, as `(name, type)`.
///
/// `CREATE TABLE IF NOT EXISTS` is inert against an installed database, so a
/// column added to `SCHEMA` never reaches one. Every entry here is also in
/// `SCHEMA`, for the benefit of databases created from scratch.
const ACCOUNTS_ADDED_COLUMNS: &[(&str, &str)] = &[("username", "TEXT"), ("last_contact_at", "INTEGER")];

/// The same, for `entries_cache`.
///
/// `last_use` is declared `NOT NULL DEFAULT 0` because SQLite will not add a
/// `NOT NULL` column without one — so it arrives full of zeroes, and the
/// backfill in [`run`] is what makes it true. Zero is not a plausible Last Use;
/// it is the marker that the backfill has not run yet.
const ENTRIES_CACHE_ADDED_COLUMNS: &[(&str, &str)] = &[
    ("last_use", "INTEGER NOT NULL DEFAULT 0"),
    ("plaintext_sha256", "TEXT"),
];

/// Indexes that name a column [`ENTRIES_CACHE_ADDED_COLUMNS`] adds.
///
/// Deliberately not in `SCHEMA`: `execute_batch(SCHEMA)` runs first, and on an
/// installed database the columns these order by do not exist yet. The same
/// reason the relay's own migration keeps its two entry indexes out of its
/// schema string.
const ENTRIES_CACHE_INDEXES: &str = r#"
DROP INDEX IF EXISTS entries_cache_user_id_id;
CREATE INDEX IF NOT EXISTS entries_cache_user_last_use
  ON entries_cache (user_id, last_use DESC, id DESC);
CREATE INDEX IF NOT EXISTS entries_cache_user_sha256
  ON entries_cache (user_id, plaintext_sha256);
"#;

/// Rebuild `pending_uploads` so one queue can hold a capture and a use.
///
/// A rebuild rather than three `ALTER`s because `ciphertext` has to *stop*
/// being `NOT NULL` — a pending use has no ciphertext — and SQLite has no way
/// to relax a constraint in place. `rowid` is copied across explicitly: it is
/// the FIFO order the flush relies on, so a rebuild that let SQLite reassign it
/// would reorder an outage's worth of captures.
const REBUILD_PENDING_UPLOADS: &str = r#"
CREATE TABLE pending_uploads_rebuilt (
  rowid            INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id          TEXT NOT NULL,
  kind             TEXT NOT NULL,
  entry_id         INTEGER,
  ciphertext       BLOB,
  plaintext_sha256 TEXT,
  captured_at      INTEGER NOT NULL,
  attempts         INTEGER NOT NULL DEFAULT 0,
  last_error       TEXT
);
INSERT INTO pending_uploads_rebuilt
  (rowid, user_id, kind, entry_id, ciphertext, plaintext_sha256, captured_at, attempts, last_error)
  SELECT rowid, user_id, 'capture', NULL, ciphertext, NULL, captured_at, attempts, last_error
  FROM pending_uploads;
DROP TABLE pending_uploads;
ALTER TABLE pending_uploads_rebuilt RENAME TO pending_uploads;
CREATE INDEX IF NOT EXISTS pending_uploads_user_id_rowid ON pending_uploads (user_id, rowid);
"#;

pub(crate) fn run(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(SCHEMA)?;
    rename_watermark_to_sequence(conn)?;
    add_missing_columns(conn, "accounts", ACCOUNTS_ADDED_COLUMNS)?;
    let added = add_missing_columns(conn, "entries_cache", ENTRIES_CACHE_ADDED_COLUMNS)?;
    if added.contains(&"last_use") {
        // An entry never used since capture has `last_use == created_at`, which
        // is the truth about it — no history of uses exists to backfill.
        conn.execute_batch("UPDATE entries_cache SET last_use = created_at")?;
    }
    if added.contains(&"plaintext_sha256") {
        backfill_plaintext_hashes(conn)?;
    }
    conn.execute_batch(ENTRIES_CACHE_INDEXES)?;
    rebuild_pending_uploads(conn)?;
    Ok(())
}

fn columns(conn: &Connection, table: &str) -> Result<HashSet<String>, AppError> {
    // No placeholders in a PRAGMA; every caller passes a compile-time constant.
    let present = conn
        .prepare(&format!("PRAGMA table_info({table})"))?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<Result<_, _>>()?;
    Ok(present)
}

/// Add whichever of `wanted` the table has not got, and report what was added.
///
/// The report is what lets a caller run a backfill exactly once: on the upgrade
/// that creates the column and never on a database that already had it.
fn add_missing_columns<'a>(
    conn: &Connection,
    table: &str,
    wanted: &'a [(&'a str, &'a str)],
) -> Result<Vec<&'a str>, AppError> {
    let present = columns(conn, table)?;
    let mut added = Vec::new();
    for (name, ty) in wanted {
        if !present.contains(*name) {
            // No placeholders in DDL; all three halves are compile-time constants.
            conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {name} {ty}"))?;
            added.push(*name);
        }
    }
    Ok(added)
}

/// `accounts.last_seen_id` becomes `accounts.last_seen_seq`.
///
/// A rename and not a new column, because the stored number stays valid: the
/// relay seeds every entry's sequence from its id, and ids are globally
/// monotonic, so a per-user sequence starts above anything this device has
/// already fetched. Renamed at all because three doc comments reason explicitly
/// about what the number means, and a watermark that counts sequences under a
/// name that says `id` is a lie in the one place this codebase is most careful.
fn rename_watermark_to_sequence(conn: &Connection) -> Result<(), AppError> {
    let present = columns(conn, "accounts")?;
    if present.contains("last_seen_id") && !present.contains("last_seen_seq") {
        conn.execute_batch("ALTER TABLE accounts RENAME COLUMN last_seen_id TO last_seen_seq")?;
    }
    Ok(())
}

/// Hash the plaintexts already cached, so a repeat copy is recognised on the
/// first launch after the upgrade rather than after a hundred fresh captures.
///
/// Bounded by the cache's own cap — a hundred entries per pairing, each at most
/// 64 KB — and paid once, on the upgrade that adds the column. SQLite has no
/// SHA-256, so this is the one backfill that cannot be a single `UPDATE`.
fn backfill_plaintext_hashes(conn: &Connection) -> Result<(), AppError> {
    let rows: Vec<(String, i64, String)> = conn
        .prepare("SELECT user_id, id, plaintext FROM entries_cache WHERE plaintext IS NOT NULL")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    let tx = conn.unchecked_transaction()?;
    {
        let mut update = tx.prepare(
            "UPDATE entries_cache SET plaintext_sha256 = ?3 WHERE user_id = ?1 AND id = ?2",
        )?;
        for (user_id, id, plaintext) in rows {
            update.execute(rusqlite::params![
                user_id,
                id,
                super::entries_cache::plaintext_sha256(&plaintext)
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn rebuild_pending_uploads(conn: &Connection) -> Result<(), AppError> {
    if columns(conn, "pending_uploads")?.contains("kind") {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(REBUILD_PENDING_UPLOADS)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        run(&c).unwrap();
        c
    }

    fn columns_of(c: &Connection, table: &str) -> Vec<String> {
        c.prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    /// `accounts` and `entries_cache` and `pending_uploads` exactly as they
    /// shipped before Last Use, with one row in each worth keeping.
    fn installed() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE accounts (
               user_id        TEXT PRIMARY KEY,
               device_id      TEXT NOT NULL,
               device_label   TEXT NOT NULL,
               server_url     TEXT NOT NULL,
               last_seen_id   INTEGER NOT NULL DEFAULT 0,
               created_at     INTEGER NOT NULL,
               username       TEXT,
               last_contact_at INTEGER
             );
             INSERT INTO accounts VALUES ('u','d','mac','https://srv',7,1,'alice',99);

             CREATE TABLE entries_cache (
               user_id     TEXT NOT NULL,
               id          INTEGER NOT NULL,
               ciphertext  BLOB NOT NULL,
               plaintext   TEXT,
               created_at  INTEGER NOT NULL,
               device_id   TEXT NOT NULL,
               PRIMARY KEY (user_id, id)
             );
             CREATE INDEX entries_cache_user_id_id ON entries_cache (user_id, id DESC);
             INSERT INTO entries_cache VALUES ('u',1,x'00','hello',1000,'d');
             INSERT INTO entries_cache VALUES ('u',2,x'00',NULL,2000,'d');

             CREATE TABLE pending_uploads (
               rowid       INTEGER PRIMARY KEY AUTOINCREMENT,
               user_id     TEXT NOT NULL,
               ciphertext  BLOB NOT NULL,
               captured_at INTEGER NOT NULL,
               attempts    INTEGER NOT NULL DEFAULT 0,
               last_error  TEXT
             );
             INSERT INTO pending_uploads (user_id, ciphertext, captured_at) VALUES ('u',x'0A',10);
             INSERT INTO pending_uploads (user_id, ciphertext, captured_at) VALUES ('u',x'0B',20);

             CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .unwrap();
        c
    }

    #[test]
    fn creates_all_five_tables() {
        let c = fresh();
        let mut stmt = c.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
        ).unwrap();
        let names: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .filter(|n| !n.starts_with("sqlite_"))
            .collect();
        assert_eq!(names, vec!["accounts", "devices", "entries_cache", "pending_uploads", "settings"]);
    }

    #[test]
    fn devices_table_is_keyed_by_user_and_device() {
        let c = fresh();
        c.execute_batch(
            "INSERT INTO devices VALUES ('u','d',NULL,NULL,1);
             INSERT INTO devices VALUES ('other','d',NULL,NULL,1);",
        )
        .unwrap();
        let dup = c.execute_batch("INSERT INTO devices VALUES ('u','d',NULL,NULL,2)");
        assert!(dup.is_err(), "(user_id, device_id) must be unique");
    }

    #[test]
    fn upgrades_an_installed_accounts_table_without_losing_rows() {
        let c = Connection::open_in_memory().unwrap();
        // `accounts` exactly as it shipped, before username/last_contact_at.
        c.execute_batch(
            "CREATE TABLE accounts (
               user_id        TEXT PRIMARY KEY,
               device_id      TEXT NOT NULL,
               device_label   TEXT NOT NULL,
               server_url     TEXT NOT NULL,
               last_seen_id   INTEGER NOT NULL DEFAULT 0,
               created_at     INTEGER NOT NULL
             );
             INSERT INTO accounts VALUES ('u','d','mac','https://srv',7,1);",
        )
        .unwrap();

        run(&c).unwrap();
        run(&c).unwrap();

        let cols = columns_of(&c, "accounts");
        assert!(cols.contains(&"username".to_string()), "got: {cols:?}");
        assert!(cols.contains(&"last_contact_at".to_string()), "got: {cols:?}");
        assert_eq!(cols.iter().filter(|c| *c == "username").count(), 1, "ALTER must not repeat");

        let row: (i64, Option<String>, Option<i64>) = c
            .query_row(
                "SELECT last_seen_seq, username, last_contact_at FROM accounts WHERE user_id = 'u'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(row, (7, None, None));
    }

    /// The watermark's whole point: the number survives the rename, because a
    /// relay seeds every sequence from an id and a client that already fetched
    /// up to id 7 has fetched up to sequence 7.
    #[test]
    fn the_watermark_is_renamed_and_keeps_its_value() {
        let c = installed();
        run(&c).unwrap();
        run(&c).unwrap();

        let cols = columns_of(&c, "accounts");
        assert!(cols.contains(&"last_seen_seq".to_string()), "got: {cols:?}");
        assert!(!cols.contains(&"last_seen_id".to_string()), "got: {cols:?}");
        let seq: i64 = c
            .query_row("SELECT last_seen_seq FROM accounts WHERE user_id = 'u'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(seq, 7);
    }

    #[test]
    fn cached_entries_are_seeded_with_a_last_use_and_a_hash() {
        let c = installed();
        run(&c).unwrap();
        run(&c).unwrap();

        let rows: Vec<(i64, i64, Option<String>)> = c
            .prepare("SELECT id, last_use, plaintext_sha256 FROM entries_cache ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(rows[0].1, 1000, "an entry never used since capture was last used at capture");
        assert_eq!(rows[1].1, 2000);
        assert_eq!(
            rows[0].2.as_deref(),
            Some(super::super::entries_cache::plaintext_sha256("hello").as_str()),
            "a plaintext already cached is hashed on upgrade, not on its next capture"
        );
        assert_eq!(rows[1].2, None, "an Undecryptable entry has nothing to hash");
    }

    /// The FIFO order *is* the rowid, so a rebuild that reassigned them would
    /// reorder an outage's worth of captures.
    #[test]
    fn the_pending_queue_is_rebuilt_with_its_order_intact() {
        let c = installed();
        run(&c).unwrap();
        run(&c).unwrap();

        let rows: Vec<(i64, String, Option<Vec<u8>>, i64)> = c
            .prepare("SELECT rowid, kind, ciphertext, captured_at FROM pending_uploads ORDER BY rowid")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], (1, "capture".into(), Some(vec![0x0A]), 10));
        assert_eq!(rows[1], (2, "capture".into(), Some(vec![0x0B]), 20));

        // And the rebuilt table takes a use, which the old one could not hold.
        c.execute_batch(
            "INSERT INTO pending_uploads (user_id, kind, entry_id, captured_at) VALUES ('u','use',42,30)",
        )
        .unwrap();
    }

    #[test]
    fn idempotent_when_run_twice() {
        let c = fresh();
        run(&c).unwrap();
        run(&c).unwrap();
    }
}
