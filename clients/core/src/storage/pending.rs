use crate::errors::AppError;
use rusqlite::{params, Connection};

/// The cap that makes the pending queue bounded — invariant 4, whose FIFO half
/// is [`head`] and whose eviction half is [`enqueue_capture`].
pub(crate) const MAX_PER_USER: i64 = 1000;

/// What one queued act is.
///
/// One queue and not two, because the order between the two kinds is the point:
/// pendings reach the relay in the order they were made, so an outage cannot
/// reorder what happened during it. Two queues draining independently would let
/// a capture made after a use land before it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PendingKind {
    /// A **Capture**, encrypted on the way into the queue.
    Capture(Vec<u8>),
    /// A **Use** of an Entry the relay already holds. No ciphertext: the entry
    /// is unchanged, and only when it was last used has moved.
    Use(i64),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PendingUpload {
    pub(crate) rowid: i64,
    pub(crate) user_id: String,
    pub(crate) kind: PendingKind,
    pub(crate) captured_at: i64,
    pub(crate) attempts: i64,
    pub(crate) last_error: Option<String>,
}

const KIND_CAPTURE: &str = "capture";
const KIND_USE: &str = "use";

/// `rowid` is the insert handle the unit tests drive `ack` / `record_failure`
/// with; production re-reads the queue head instead and never needs it.
///
/// `dropped_oldest` counts pendings evicted at the `MAX_PER_USER` cap - acts
/// the user performed that will now never reach the relay. The capture loop
/// logs a warning when it is non-zero; without that the loss is completely
/// silent.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug)]
pub struct EnqueueResult {
    pub rowid: i64,
    pub dropped_oldest: usize,
}

/// Queue a **Capture**.
///
/// `plaintext_sha256` is stored so [`find_by_hash`] can recognise a repeat copy
/// of something still queued. The queue holds ciphertext, and ciphertext cannot
/// be compared: `crypto::encrypt` draws a fresh nonce every call.
pub fn enqueue_capture(
    conn: &Connection,
    user_id: &str,
    ciphertext: &[u8],
    plaintext_sha256: &str,
    captured_at: i64,
) -> Result<EnqueueResult, AppError> {
    enqueue(
        conn,
        user_id,
        "INSERT INTO pending_uploads (user_id, kind, ciphertext, plaintext_sha256, captured_at)
         VALUES (?1, 'capture', ?2, ?3, ?4)",
        params![user_id, ciphertext, plaintext_sha256, captured_at],
    )
}

/// Queue a **Use** of an Entry, because the relay could not be reached.
///
/// The relay stamps it on arrival, exactly as it already stamps a pending
/// capture: one clock in the system, and the flush order is what preserves what
/// actually happened during the outage.
pub(crate) fn enqueue_use(
    conn: &Connection,
    user_id: &str,
    entry_id: i64,
    at: i64,
) -> Result<EnqueueResult, AppError> {
    enqueue(
        conn,
        user_id,
        "INSERT INTO pending_uploads (user_id, kind, entry_id, captured_at)
         VALUES (?1, 'use', ?2, ?3)",
        params![user_id, entry_id, at],
    )
}

fn enqueue(
    conn: &Connection,
    user_id: &str,
    insert: &str,
    args: &[&dyn rusqlite::ToSql],
) -> Result<EnqueueResult, AppError> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(insert, args)?;
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

pub(crate) fn head(conn: &Connection, user_id: &str) -> Result<Option<PendingUpload>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT rowid, user_id, kind, entry_id, ciphertext, captured_at, attempts, last_error
         FROM pending_uploads
         WHERE user_id = ?1
         ORDER BY rowid ASC LIMIT 1"
    )?;
    let row = stmt
        .query_row(params![user_id], |r| {
            let kind: String = r.get(2)?;
            let kind = match kind.as_str() {
                KIND_USE => PendingKind::Use(r.get(3)?),
                // Anything else is a capture. The discriminator is written by
                // this module alone, and a row the migration produced is one.
                _ => PendingKind::Capture(r.get(4)?),
            };
            Ok(PendingUpload {
                rowid: r.get(0)?,
                user_id: r.get(1)?,
                kind,
                captured_at: r.get(5)?,
                attempts: r.get(6)?,
                last_error: r.get(7)?,
            })
        })
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

/// The queued capture whose plaintext is exactly this text, if there is one.
///
/// Head-first, so a repeat copy moves the *oldest* matching pending rather than
/// leaving it stranded behind its own duplicate.
pub(crate) fn find_by_hash(
    conn: &Connection,
    user_id: &str,
    sha256: &str,
) -> Result<Option<i64>, AppError> {
    let rowid = conn
        .query_row(
            "SELECT rowid FROM pending_uploads
              WHERE user_id = ?1 AND kind = ?2 AND plaintext_sha256 = ?3
              ORDER BY rowid ASC LIMIT 1",
            params![user_id, KIND_CAPTURE, sha256],
            |r| r.get::<_, i64>(0),
        )
        .ok();
    Ok(rowid)
}

/// Move a pending to the back of the queue, as of `at`.
///
/// What a repeat copy of something still queued amounts to. A pending has no
/// relay id, so there is nothing to use; re-copying it is the same act as
/// copying it, and the queue's order is what carries that. The attempt count
/// and the last error do not travel: this is a fresh act, not a retry.
pub(crate) fn requeue_to_back(conn: &Connection, rowid: i64, at: i64) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;
    let moved = tx.execute(
        "INSERT INTO pending_uploads (user_id, kind, entry_id, ciphertext, plaintext_sha256, captured_at)
         SELECT user_id, kind, entry_id, ciphertext, plaintext_sha256, ?2
           FROM pending_uploads WHERE rowid = ?1",
        params![rowid, at],
    )?;
    if moved == 1 {
        tx.execute("DELETE FROM pending_uploads WHERE rowid = ?1", params![rowid])?;
    }
    tx.commit()?;
    Ok(())
}

pub(crate) fn ack(conn: &Connection, rowid: i64) -> Result<(), AppError> {
    conn.execute("DELETE FROM pending_uploads WHERE rowid = ?1", params![rowid])?;
    Ok(())
}

pub(crate) fn record_failure(conn: &Connection, rowid: i64, err: &str) -> Result<i64, AppError> {
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
    use crate::storage::entries_cache::plaintext_sha256;
    use crate::storage::open_in_memory;

    fn capture(c: &Connection, user: &str, text: &str, at: i64) -> EnqueueResult {
        enqueue_capture(c, user, text.as_bytes(), &plaintext_sha256(text), at).unwrap()
    }

    fn kinds(c: &Connection, user: &str) -> Vec<PendingKind> {
        let mut out = Vec::new();
        let mut seen = Vec::new();
        while let Some(h) = head(c, user).unwrap() {
            out.push(h.kind);
            seen.push(h.rowid);
            ack(c, h.rowid).unwrap();
        }
        out
    }

    #[test]
    fn enqueue_returns_rowid_and_no_drops() {
        let c = open_in_memory().unwrap();
        let r = capture(&c, "u", "x", 1);
        assert!(r.rowid > 0);
        assert_eq!(r.dropped_oldest, 0);
        assert_eq!(count(&c, "u").unwrap(), 1);
    }

    #[test]
    fn head_is_fifo() {
        let c = open_in_memory().unwrap();
        for i in 1..=3 {
            capture(&c, "u", &format!("t{i}"), i as i64);
        }
        let h = head(&c, "u").unwrap().unwrap();
        assert_eq!(h.captured_at, 1);
    }

    /// The reason there is one queue: an outage cannot reorder what happened
    /// during it.
    #[test]
    fn captures_and_uses_drain_in_the_order_they_were_made() {
        let c = open_in_memory().unwrap();
        capture(&c, "u", "first", 1);
        enqueue_use(&c, "u", 42, 2).unwrap();
        capture(&c, "u", "second", 3);

        assert_eq!(
            kinds(&c, "u"),
            vec![
                PendingKind::Capture(b"first".to_vec()),
                PendingKind::Use(42),
                PendingKind::Capture(b"second".to_vec()),
            ]
        );
    }

    #[test]
    fn ack_removes_row() {
        let c = open_in_memory().unwrap();
        let r = capture(&c, "u", "x", 1);
        ack(&c, r.rowid).unwrap();
        assert!(head(&c, "u").unwrap().is_none());
    }

    #[test]
    fn record_failure_increments_attempts_and_stores_error() {
        let c = open_in_memory().unwrap();
        let r = capture(&c, "u", "x", 1);
        let n = record_failure(&c, r.rowid, "boom").unwrap();
        assert_eq!(n, 1);
        let n2 = record_failure(&c, r.rowid, "again").unwrap();
        assert_eq!(n2, 2);
        let h = head(&c, "u").unwrap().unwrap();
        assert_eq!(h.last_error.as_deref(), Some("again"));
    }

    #[test]
    fn find_by_hash_matches_a_queued_capture_and_never_a_use() {
        let c = open_in_memory().unwrap();
        capture(&c, "u", "hello", 1);
        enqueue_use(&c, "u", 7, 2).unwrap();
        assert_eq!(find_by_hash(&c, "u", &plaintext_sha256("hello")).unwrap(), Some(1));
        assert_eq!(find_by_hash(&c, "u", &plaintext_sha256("hello\n")).unwrap(), None);
        assert_eq!(find_by_hash(&c, "other", &plaintext_sha256("hello")).unwrap(), None);
    }

    #[test]
    fn requeue_moves_a_pending_behind_everything_queued_after_it() {
        let c = open_in_memory().unwrap();
        let first = capture(&c, "u", "first", 1);
        capture(&c, "u", "second", 2);
        requeue_to_back(&c, first.rowid, 30).unwrap();

        assert_eq!(count(&c, "u").unwrap(), 2, "moved, not duplicated");
        let h = head(&c, "u").unwrap().unwrap();
        assert_eq!(h.kind, PendingKind::Capture(b"second".to_vec()));
        assert_eq!(
            kinds(&c, "u"),
            vec![
                PendingKind::Capture(b"second".to_vec()),
                PendingKind::Capture(b"first".to_vec()),
            ]
        );
    }

    /// The moved pending is a fresh act, so it is still recognisable and no
    /// longer carries the failures of its previous position.
    #[test]
    fn a_requeued_pending_keeps_its_hash_and_drops_its_attempts() {
        let c = open_in_memory().unwrap();
        let first = capture(&c, "u", "first", 1);
        record_failure(&c, first.rowid, "boom").unwrap();
        requeue_to_back(&c, first.rowid, 30).unwrap();

        let moved = find_by_hash(&c, "u", &plaintext_sha256("first")).unwrap().unwrap();
        assert_ne!(moved, first.rowid);
        let h = head(&c, "u").unwrap().unwrap();
        assert_eq!(h.rowid, moved);
        assert_eq!(h.attempts, 0);
        assert_eq!(h.last_error, None);
        assert_eq!(h.captured_at, 30);
    }

    #[test]
    fn over_cap_drops_oldest_only_for_that_user() {
        let c = open_in_memory().unwrap();
        for i in 0..MAX_PER_USER + 5 {
            capture(&c, "u", "x", i);
        }
        for i in 0..3 { capture(&c, "v", "x", i); }
        assert_eq!(count(&c, "u").unwrap(), MAX_PER_USER);
        assert_eq!(count(&c, "v").unwrap(), 3);
    }
}
