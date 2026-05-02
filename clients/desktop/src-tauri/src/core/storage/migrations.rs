use crate::errors::AppError;
use rusqlite::Connection;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS accounts (
  user_id        TEXT PRIMARY KEY,
  device_id      TEXT NOT NULL,
  device_label   TEXT NOT NULL,
  server_url     TEXT NOT NULL,
  last_seen_id   INTEGER NOT NULL DEFAULT 0,
  created_at     INTEGER NOT NULL
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

pub fn run(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(SCHEMA)?;
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

    #[test]
    fn creates_all_four_tables() {
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
        assert_eq!(names, vec!["accounts", "entries_cache", "pending_uploads", "settings"]);
    }

    #[test]
    fn idempotent_when_run_twice() {
        let c = fresh();
        run(&c).unwrap();
        run(&c).unwrap();
    }
}
