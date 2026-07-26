use crate::errors::AppError;
use rusqlite::Connection;
use std::collections::HashSet;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS accounts (
  user_id        TEXT PRIMARY KEY,
  device_id      TEXT NOT NULL,
  device_label   TEXT NOT NULL,
  server_url     TEXT NOT NULL,
  last_seen_id   INTEGER NOT NULL DEFAULT 0,
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
  user_id     TEXT NOT NULL,
  id          INTEGER NOT NULL,
  ciphertext  BLOB NOT NULL,
  plaintext   TEXT,
  created_at  INTEGER NOT NULL,
  device_id   TEXT NOT NULL,
  PRIMARY KEY (user_id, id)
);
CREATE INDEX IF NOT EXISTS entries_cache_user_id_id ON entries_cache (user_id, id DESC);

CREATE TABLE IF NOT EXISTS pending_uploads (
  rowid       INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id     TEXT NOT NULL,
  ciphertext  BLOB NOT NULL,
  captured_at INTEGER NOT NULL,
  attempts    INTEGER NOT NULL DEFAULT 0,
  last_error  TEXT
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

pub(crate) fn run(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(SCHEMA)?;
    add_missing_accounts_columns(conn)?;
    Ok(())
}

fn add_missing_accounts_columns(conn: &Connection) -> Result<(), AppError> {
    let present: HashSet<String> = conn
        .prepare("PRAGMA table_info(accounts)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<Result<_, _>>()?;
    for (name, ty) in ACCOUNTS_ADDED_COLUMNS {
        if !present.contains(*name) {
            // No placeholders in DDL; both halves are compile-time constants.
            conn.execute_batch(&format!("ALTER TABLE accounts ADD COLUMN {name} {ty}"))?;
        }
    }
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

    fn columns(c: &Connection, table: &str) -> Vec<String> {
        c.prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
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

        let cols = columns(&c, "accounts");
        assert!(cols.contains(&"username".to_string()), "got: {cols:?}");
        assert!(cols.contains(&"last_contact_at".to_string()), "got: {cols:?}");
        assert_eq!(cols.iter().filter(|c| *c == "username").count(), 1, "ALTER must not repeat");

        let row: (i64, Option<String>, Option<i64>) = c
            .query_row(
                "SELECT last_seen_id, username, last_contact_at FROM accounts WHERE user_id = 'u'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(row, (7, None, None));
    }

    #[test]
    fn idempotent_when_run_twice() {
        let c = fresh();
        run(&c).unwrap();
        run(&c).unwrap();
    }
}
