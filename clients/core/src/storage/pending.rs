//! The queue of acts this device owes the relay.
//!
//! The FIFO order is [`head`]'s, and there is no cap on depth: an act this
//! device has not delivered is undelivered clipboard content, and this queue
//! used to evict the oldest of them silently to keep a number under a thousand
//! (ADR 0014). What bounds it is the relay coming back.

use crate::errors::AppError;
use rusqlite::{params, Connection};

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
    /// The entry this act belongs to: the row a capture created, or the row a
    /// use acts on.
    ///
    /// What lets a queued act be found from its entry, and what the flush
    /// reconciles a relay id onto. `None` only on a row a database written
    /// before this column existed left behind.
    pub(crate) local_entry_id: Option<i64>,
    pub(crate) captured_at: i64,
    pub(crate) attempts: i64,
    pub(crate) last_error: Option<String>,
}

const KIND_CAPTURE: &str = "capture";
const KIND_USE: &str = "use";

/// Queue a **Capture**.
///
/// Answers the queue rowid, which is the handle a requeue and the unit tests
/// address the act by; production re-reads the head instead and never needs it.
///
/// `plaintext_sha256` is stored so [`find_by_hash`] can recognise a repeat copy
/// of something still queued. The queue holds ciphertext, and ciphertext cannot
/// be compared: `crypto::encrypt` draws a fresh nonce every call.
pub fn enqueue_capture(
    conn: &Connection,
    user_id: &str,
    local_entry_id: i64,
    ciphertext: &[u8],
    plaintext_sha256: &str,
    captured_at: i64,
) -> Result<i64, AppError> {
    enqueue(
        conn,
        "INSERT INTO pending_uploads
            (user_id, kind, local_entry_id, ciphertext, plaintext_sha256, captured_at)
         VALUES (?1, 'capture', ?2, ?3, ?4, ?5)",
        params![user_id, local_entry_id, ciphertext, plaintext_sha256, captured_at],
    )
}

/// Queue a **Use** of an Entry, because the relay could not be reached.
///
/// Carries both ids the act needs: `entry_id` is what the relay is told about,
/// `local_entry_id` is the row this device shows and orders.
///
/// The relay stamps it on arrival, exactly as it already stamps a pending
/// capture: one clock in the system, and the flush order is what preserves what
/// actually happened during the outage.
pub(crate) fn enqueue_use(
    conn: &Connection,
    user_id: &str,
    local_entry_id: i64,
    relay_entry_id: i64,
    at: i64,
) -> Result<i64, AppError> {
    enqueue(
        conn,
        "INSERT INTO pending_uploads (user_id, kind, entry_id, local_entry_id, captured_at)
         VALUES (?1, 'use', ?2, ?3, ?4)",
        params![user_id, relay_entry_id, local_entry_id, at],
    )
}

/// Insert one act, and answer where in the queue it landed.
fn enqueue(
    conn: &Connection,
    insert: &str,
    args: &[&dyn rusqlite::ToSql],
) -> Result<i64, AppError> {
    conn.execute(insert, args)?;
    Ok(conn.last_insert_rowid())
}

pub(crate) fn head(conn: &Connection, user_id: &str) -> Result<Option<PendingUpload>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT rowid, user_id, kind, entry_id, ciphertext, captured_at, attempts, last_error,
                local_entry_id
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
                local_entry_id: r.get(8)?,
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
        "INSERT INTO pending_uploads
            (user_id, kind, entry_id, local_entry_id, ciphertext, plaintext_sha256, captured_at)
         SELECT user_id, kind, entry_id, local_entry_id, ciphertext, plaintext_sha256, ?2
           FROM pending_uploads WHERE rowid = ?1",
        params![rowid, at],
    )?;
    if moved == 1 {
        tx.execute("DELETE FROM pending_uploads WHERE rowid = ?1", params![rowid])?;
    }
    tx.commit()?;
    Ok(())
}

/// Take a settled act off the queue, and report whether it was still there.
///
/// Zero is the withdrawal race and the only evidence of it: `flush_once` awaits
/// the relay with the database lock released, and a delete inside that window
/// removes this row. Its caller has to know, because the relay has taken an act
/// nobody wants any more.
pub(crate) fn ack(conn: &Connection, rowid: i64) -> Result<usize, AppError> {
    let n = conn.execute("DELETE FROM pending_uploads WHERE rowid = ?1", params![rowid])?;
    Ok(n)
}

/// The act queued against one entry, if there is one.
///
/// The last of them by queue position, which is the one a fresh act moves: an
/// entry can hold several — a capture and the uses re-copied onto it — and the
/// row already sorts by the newest of them.
pub(crate) fn rowid_for_entry(
    conn: &Connection,
    user_id: &str,
    local_entry_id: i64,
) -> Result<Option<i64>, AppError> {
    let rowid: Option<i64> = conn.query_row(
        "SELECT MAX(rowid) FROM pending_uploads
          WHERE user_id = ?1 AND local_entry_id = ?2",
        params![user_id, local_entry_id],
        |r| r.get(0),
    )?;
    Ok(rowid)
}

/// Withdraw every act queued against one entry.
///
/// What deleting an un-flushed row amounts to: the queue is durable across a
/// force-quit, so without this there is no way to stop a mistaken copy reaching
/// the relay (ADR 0013).
pub(crate) fn delete_for_entry(
    conn: &Connection,
    user_id: &str,
    local_entry_id: i64,
) -> Result<usize, AppError> {
    let n = conn.execute(
        "DELETE FROM pending_uploads WHERE user_id = ?1 AND local_entry_id = ?2",
        params![user_id, local_entry_id],
    )?;
    Ok(n)
}

/// Empty one Pairing's queue.
///
/// `clear_history` needs it: a queue left standing repopulates exactly what was
/// just cleared on the next flush.
pub(crate) fn delete_all(conn: &Connection, user_id: &str) -> Result<usize, AppError> {
    let n = conn.execute("DELETE FROM pending_uploads WHERE user_id = ?1", params![user_id])?;
    Ok(n)
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

    /// A capture whose local entry id is derived from the text, so a test can
    /// assert the link without threading a cache insert through every case.
    /// Answers the queue rowid.
    fn capture(c: &Connection, user: &str, text: &str, at: i64) -> i64 {
        enqueue_capture(c, user, text.len() as i64, text.as_bytes(), &plaintext_sha256(text), at)
            .unwrap()
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
    fn enqueue_answers_where_the_act_landed() {
        let c = open_in_memory().unwrap();
        let rowid = capture(&c, "u", "x", 1);
        assert!(rowid > 0);
        assert_eq!(head(&c, "u").unwrap().unwrap().rowid, rowid);
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
        enqueue_use(&c, "u", 9, 42, 2).unwrap();
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
        ack(&c, r).unwrap();
        assert!(head(&c, "u").unwrap().is_none());
    }

    #[test]
    fn record_failure_increments_attempts_and_stores_error() {
        let c = open_in_memory().unwrap();
        let r = capture(&c, "u", "x", 1);
        let n = record_failure(&c, r, "boom").unwrap();
        assert_eq!(n, 1);
        let n2 = record_failure(&c, r, "again").unwrap();
        assert_eq!(n2, 2);
        let h = head(&c, "u").unwrap().unwrap();
        assert_eq!(h.last_error.as_deref(), Some("again"));
    }

    #[test]
    fn find_by_hash_matches_a_queued_capture_and_never_a_use() {
        let c = open_in_memory().unwrap();
        capture(&c, "u", "hello", 1);
        enqueue_use(&c, "u", 9, 7, 2).unwrap();
        assert_eq!(find_by_hash(&c, "u", &plaintext_sha256("hello")).unwrap(), Some(1));
        assert_eq!(find_by_hash(&c, "u", &plaintext_sha256("hello\n")).unwrap(), None);
        assert_eq!(find_by_hash(&c, "other", &plaintext_sha256("hello")).unwrap(), None);
    }

    #[test]
    fn requeue_moves_a_pending_behind_everything_queued_after_it() {
        let c = open_in_memory().unwrap();
        let first = capture(&c, "u", "first", 1);
        capture(&c, "u", "second", 2);
        requeue_to_back(&c, first, 30).unwrap();

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

    /// Both kinds name the entry they belong to, and the link survives a
    /// requeue: tickets that find an act from its row depend on it.
    #[test]
    fn both_kinds_carry_the_entry_they_belong_to() {
        let c = open_in_memory().unwrap();
        enqueue_capture(&c, "u", 11, b"ct", &plaintext_sha256("ct"), 1).unwrap();
        assert_eq!(head(&c, "u").unwrap().unwrap().local_entry_id, Some(11));
        ack(&c, head(&c, "u").unwrap().unwrap().rowid).unwrap();

        let queued = enqueue_use(&c, "u", 12, 500, 2).unwrap();
        assert_eq!(head(&c, "u").unwrap().unwrap().local_entry_id, Some(12));
        requeue_to_back(&c, queued, 30).unwrap();
        let moved = head(&c, "u").unwrap().unwrap();
        assert_eq!(moved.local_entry_id, Some(12), "the link is a fresh act's too");
        assert_eq!(moved.kind, PendingKind::Use(500), "and so is the relay's id");
    }

    /// The moved pending is a fresh act, so it is still recognisable and no
    /// longer carries the failures of its previous position.
    #[test]
    fn a_requeued_pending_keeps_its_hash_and_drops_its_attempts() {
        let c = open_in_memory().unwrap();
        let first = capture(&c, "u", "first", 1);
        record_failure(&c, first, "boom").unwrap();
        requeue_to_back(&c, first, 30).unwrap();

        let moved = find_by_hash(&c, "u", &plaintext_sha256("first")).unwrap().unwrap();
        assert_ne!(moved, first);
        let h = head(&c, "u").unwrap().unwrap();
        assert_eq!(h.rowid, moved);
        assert_eq!(h.attempts, 0);
        assert_eq!(h.last_error, None);
        assert_eq!(h.captured_at, 30);
    }

    /*
     * The queue used to evict its oldest acts at a cap of a thousand, silently
     * destroying clipboard content that had reached nowhere else to keep a number
     * down (ADR 0014). A thousand and five offline acts are a thousand and five.
     */
    #[test]
    fn nothing_is_ever_evicted_from_the_queue() {
        let c = open_in_memory().unwrap();
        for i in 0..1_005 {
            capture(&c, "u", "x", i);
        }
        for i in 0..3 {
            capture(&c, "v", "x", i);
        }
        assert_eq!(count(&c, "u").unwrap(), 1_005);
        assert_eq!(count(&c, "v").unwrap(), 3);
    }
}
