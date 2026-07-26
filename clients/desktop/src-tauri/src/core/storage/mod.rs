pub(crate) mod accounts;
pub(crate) mod devices;
pub(crate) mod entries_cache;
pub(crate) mod migrations;
pub(crate) mod pending;
pub(crate) mod settings;

use crate::errors::AppError;
use rusqlite::Connection;
use std::path::Path;

pub(crate) fn open(path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrations::run(&conn)?;
    Ok(conn)
}

/// Test-only: every production path opens the on-disk database via [`open`].
#[cfg(test)]
pub(crate) fn open_in_memory() -> Result<Connection, AppError> {
    let conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrations::run(&conn)?;
    Ok(conn)
}
