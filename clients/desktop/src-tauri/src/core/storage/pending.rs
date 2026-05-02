use crate::errors::AppError;
use rusqlite::{params, Connection};

pub const MAX_PER_USER: i64 = 1000;

#[derive(Debug, Clone, PartialEq)]
pub struct PendingUpload {
    pub rowid: i64,
    pub user_id: String,
    pub ciphertext: Vec<u8>,
    pub captured_at: i64,
    pub attempts: i64,
    pub last_error: Option<String>,
}

#[derive(Debug)]
pub struct EnqueueResult {
    pub rowid: i64,
    pub dropped_oldest: usize,
}

pub fn enqueue(
    conn: &Connection,
    user_id: &str,
    ciphertext: &[u8],
    captured_at: i64,
) -> Result<EnqueueResult, AppError> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO pending_uploads (user_id, ciphertext, captured_at) VALUES (?1, ?2, ?3)",
        params![user_id, ciphertext, captured_at],
    )?;
    let rowid = tx.last_insert_rowid();
    let dropped_oldest = tx.execute(
        "DELETE FROM pending_uploads
          WHERE user_id = ?1
            AND rowid NOT IN (
              SELECT rowid FROM pending_uploads
              WHERE user_id = ?1
              ORDER BY rowid DESC
              LIMIT ?2
            )",
        params![user_id, MAX_PER_USER],
    )?;
    tx.commit()?;
    Ok(EnqueueResult { rowid, dropped_oldest })
}

pub fn head(conn: &Connection, user_id: &str) -> Result<Option<PendingUpload>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT rowid, user_id, ciphertext, captured_at, attempts, last_error
         FROM pending_uploads
         WHERE user_id = ?1
         ORDER BY rowid ASC LIMIT 1"
    )?;
    let row = stmt
        .query_row(params![user_id], |r| Ok(PendingUpload {
            rowid: r.get(0)?,
            user_id: r.get(1)?,
            ciphertext: r.get(2)?,
            captured_at: r.get(3)?,
            attempts: r.get(4)?,
            last_error: r.get(5)?,
        }))
        .ok();
    Ok(row)
}

pub fn count(conn: &Connection, user_id: &str) -> Result<i64, AppError> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pending_uploads WHERE user_id = ?1",
        params![user_id],
        |r| r.get(0),
    )?;
    Ok(n)
}

pub fn ack(conn: &Connection, rowid: i64) -> Result<(), AppError> {
    conn.execute("DELETE FROM pending_uploads WHERE rowid = ?1", params![rowid])?;
    Ok(())
}

pub fn record_failure(conn: &Connection, rowid: i64, err: &str) -> Result<i64, AppError> {
    conn.execute(
        "UPDATE pending_uploads SET attempts = attempts + 1, last_error = ?2 WHERE rowid = ?1",
        params![rowid, err],
    )?;
    let attempts: i64 = conn.query_row(
        "SELECT attempts FROM pending_uploads WHERE rowid = ?1",
        params![rowid],
        |r| r.get(0),
    )?;
    Ok(attempts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::storage::open_in_memory;

    #[test]
    fn enqueue_returns_rowid_and_no_drops() {
        let c = open_in_memory().unwrap();
        let r = enqueue(&c, "u", b"x", 1).unwrap();
        assert!(r.rowid > 0);
        assert_eq!(r.dropped_oldest, 0);
        assert_eq!(count(&c, "u").unwrap(), 1);
    }

    #[test]
    fn head_is_fifo() {
        let c = open_in_memory().unwrap();
        for i in 1..=3 {
            enqueue(&c, "u", &[i], i as i64).unwrap();
        }
        let h = head(&c, "u").unwrap().unwrap();
        assert_eq!(h.captured_at, 1);
    }

    #[test]
    fn ack_removes_row() {
        let c = open_in_memory().unwrap();
        let r = enqueue(&c, "u", b"x", 1).unwrap();
        ack(&c, r.rowid).unwrap();
        assert!(head(&c, "u").unwrap().is_none());
    }

    #[test]
    fn record_failure_increments_attempts_and_stores_error() {
        let c = open_in_memory().unwrap();
        let r = enqueue(&c, "u", b"x", 1).unwrap();
        let n = record_failure(&c, r.rowid, "boom").unwrap();
        assert_eq!(n, 1);
        let n2 = record_failure(&c, r.rowid, "again").unwrap();
        assert_eq!(n2, 2);
        let h = head(&c, "u").unwrap().unwrap();
        assert_eq!(h.last_error.as_deref(), Some("again"));
    }

    #[test]
    fn over_cap_drops_oldest_only_for_that_user() {
        let c = open_in_memory().unwrap();
        for i in 0..MAX_PER_USER + 5 {
            enqueue(&c, "u", &[0], i).unwrap();
        }
        for i in 0..3 { enqueue(&c, "v", &[0], i).unwrap(); }
        assert_eq!(count(&c, "u").unwrap(), MAX_PER_USER);
        assert_eq!(count(&c, "v").unwrap(), 3);
    }
}
