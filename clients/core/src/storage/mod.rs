pub mod accounts;
pub mod devices;
pub mod history;
pub mod migrations;
pub mod settings;

use crate::errors::AppError;
use rusqlite::Connection;
use std::path::Path;

pub fn open(path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrations::run(&conn)?;
    Ok(conn)
}

/// Test-only: every production path opens the on-disk database via [`open`].
///
/// Behind the `testing` feature as well as `cfg(test)` so a dependent's tests can
/// reach it. The desktop's session loop is still in the shell crate — untangling
/// its Tauri seams is ticket 04 — and its tests need this same database.
#[cfg(any(test, feature = "testing"))]
pub fn open_in_memory() -> Result<Connection, AppError> {
    let conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrations::run(&conn)?;
    Ok(conn)
}
