use crate::errors::AppError;
use data_encoding::HEXLOWER;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq)]
pub struct CachedEntry {
    pub user_id: String,
    /// This device's own name for the row, assigned at insert and stable until
    /// the row is deleted.
    ///
    /// An Entry exists from the moment of capture, on the device that made it,
    /// and the relay's id is what makes it *shared* rather than what makes it
    /// real (ADR 0013). So the row needs an identity the relay did not assign,
    /// and this is it: nothing about a flush changes it, which is what lets a
    /// shell keep a row's selection and keyboard cursor across one.
    pub local_id: i64,
    /// The relay's id, once the relay has taken the act that created this row.
    ///
    /// `None` is an Entry this device holds and no other device knows of yet.
    pub relay_id: Option<i64>,
    pub ciphertext: Vec<u8>,
    pub plaintext: Option<String>,
    pub created_at: i64,
    /// The moment of this entry's most recent **Use**, on whichever device.
    ///
    /// The only fact the History is ordered by and the only fact either
    /// retention cap measures. Equal to `created_at` for an entry never used
    /// since capture, which is the truth about it rather than a placeholder.
    pub last_use: i64,
    pub device_id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct NewCachedEntry<'a> {
    pub(crate) user_id: &'a str,
    pub(crate) relay_id: Option<i64>,
    pub(crate) ciphertext: &'a [u8],
    pub(crate) plaintext: Option<&'a str>,
    pub(crate) plaintext_sha256: Option<&'a str>,
    pub(crate) created_at: i64,
    pub(crate) last_use: i64,
    pub(crate) device_id: &'a str,
}

pub(crate) const MAX_PER_USER: i64 = 100;
pub(crate) const MAX_AGE_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// What goes in `plaintext_sha256`, defined once by the column's owner.
///
/// Over the text **exactly as copied** — no trimming, no normalisation — so the
/// same URL with and without a trailing newline is two entries. Trimming would
/// recognise more and would make a **Recall** hand back the *stored* variant
/// rather than the one just copied, which in a shell is the difference between
/// a command that runs and one that waits. See ADR 0012.
pub(crate) fn plaintext_sha256(text: &str) -> String {
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    HEXLOWER.encode(&h.finalize())
}

/// What one ingest did to the cache.
///
/// `local_id` is the row the ingest landed on, which the caller needs because
/// it is the only id that crosses the facade: the relay's id is what this path
/// arrived with, and the shell has never heard of it.
///
/// Two facts beside it and not one, because three paths reach the same relay id
/// and each owes the shell a different event. `first_insert` says an Entry is
/// new here, and exactly one path may raise `EntryAdded` for it. `moved` says an
/// Entry already here changed its place, which is what `HistoryChanged` reports.
///
/// The pair is what tells a **Use** apart from the relay's echo of an Entry
/// this device uploaded: both are repeat ingests of a row the cache holds, and
/// only the use carries a later Last Use. Deriving "it moved" from
/// `!first_insert` would make every one of this device's own uploads announce a
/// reorder that did not happen, and cost both shells a full refetch each time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stored {
    pub local_id: i64,
    pub first_insert: bool,
    pub moved: bool,
}

/// Store one Entry the relay has named, then bring the user's cache back inside
/// its caps.
///
/// Keyed on `relay_id`, because that is the only handle its callers have: this
/// is the relay-delivered path — a backfill row, an SSE frame — and what they
/// carry is the relay's number for the entry. The row's `local_id` is allocated
/// on the insert and never touched again.
///
/// The answer comes from the database rather than from a caller's bookkeeping:
/// `ON CONFLICT DO UPDATE` reports one changed row either way and SQLite has no
/// "was it an insert" flag, so both facts are read before the write.
///
/// Both caps measure from `last_use`, so ciphertext in regular use is never
/// aged out. Eviction and ordering reading different facts is the version where
/// the count cap deletes the row sitting at the top of the list.
pub(crate) fn upsert_and_prune(conn: &Connection, e: NewCachedEntry<'_>, now_ms: i64) -> Result<Stored, AppError> {
    let tx = conn.unchecked_transaction()?;
    let held: Option<i64> = tx
        .query_row(
            "SELECT last_use FROM entries_cache WHERE user_id = ?1 AND relay_id = ?2",
            params![e.user_id, e.relay_id],
            |r| r.get(0),
        )
        .optional()?;
    tx.execute(
        "INSERT INTO entries_cache
            (user_id, relay_id, ciphertext, plaintext, plaintext_sha256, created_at, last_use, device_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT (user_id, relay_id) DO UPDATE SET
            ciphertext       = excluded.ciphertext,
            plaintext        = COALESCE(excluded.plaintext, entries_cache.plaintext),
            plaintext_sha256 = COALESCE(excluded.plaintext_sha256, entries_cache.plaintext_sha256),
            created_at       = excluded.created_at,
            last_use         = excluded.last_use,
            device_id        = excluded.device_id",
        params![
            e.user_id, e.relay_id, e.ciphertext, e.plaintext, e.plaintext_sha256,
            e.created_at, e.last_use, e.device_id
        ],
    )?;
    let local_id = tx.query_row(
        "SELECT local_id FROM entries_cache WHERE user_id = ?1 AND relay_id = ?2",
        params![e.user_id, e.relay_id],
        |r| r.get(0),
    )?;
    prune(&tx, e.user_id, now_ms)?;
    tx.commit()?;
    Ok(Stored {
        local_id,
        first_insert: held.is_none(),
        moved: held.is_some_and(|was| was != e.last_use),
    })
}

/// Store an Entry this device just captured, before the relay has named it.
///
/// Hands back the `local_id` the row now has, which is the handle everything
/// above `storage/` uses for it and the handle the queued act carries.
///
/// **`created_at` and `last_use` are zero**, and stay zero until the relay
/// stamps the act. There is one clock in this system and it is the relay's
/// (ADR 0014): a provisional device timestamp would order this row against
/// relay-stamped ones by a number no other device agrees with, and would then be
/// silently overwritten. Zero is the same "not stamped yet" marker `last_use`
/// already carries before its backfill runs. What orders the row meanwhile is
/// its place in the queue.
///
/// **It does not prune**, and must not: the caps bound what the relay has
/// ordered, and an act still owed to the relay is not competing for that room.
/// Evicting one would destroy clipboard content that has reached nowhere else.
pub(crate) fn insert_captured(
    conn: &Connection,
    user_id: &str,
    ciphertext: &[u8],
    plaintext: &str,
    plaintext_sha256: &str,
    device_id: &str,
) -> Result<i64, AppError> {
    conn.execute(
        "INSERT INTO entries_cache
            (user_id, relay_id, ciphertext, plaintext, plaintext_sha256, created_at, last_use, device_id)
         VALUES (?1, NULL, ?2, ?3, ?4, 0, 0, ?5)",
        params![user_id, ciphertext, plaintext, plaintext_sha256, device_id],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Attach what the relay recorded to the row a capture already created.
///
/// The reconciliation half of [`insert_captured`]: an update and never an
/// insert, so the row keeps the `local_id` both shells are keying on. Reports
/// zero when there is no such un-named row — the act was withdrawn while the
/// upload was in flight, and its caller has to decide what to do about that.
///
/// Prunes, because the row has just joined the region the caps bound.
pub(crate) fn attach_relay_id(
    conn: &Connection,
    user_id: &str,
    local_id: i64,
    relay_id: i64,
    created_at: i64,
    last_use: i64,
    now_ms: i64,
) -> Result<usize, AppError> {
    let tx = conn.unchecked_transaction()?;
    let n = tx.execute(
        "UPDATE entries_cache
            SET relay_id = ?3, created_at = ?4, last_use = ?5
          WHERE user_id = ?1 AND local_id = ?2 AND relay_id IS NULL",
        params![user_id, local_id, relay_id, created_at, last_use],
    )?;
    prune(&tx, user_id, now_ms)?;
    tx.commit()?;
    Ok(n)
}

/// The relay's id for one row, if the relay has taken the act that created it.
pub(crate) fn relay_id_for(
    conn: &Connection,
    user_id: &str,
    local_id: i64,
) -> Result<Option<i64>, AppError> {
    let id: Option<Option<i64>> = conn
        .query_row(
            "SELECT relay_id FROM entries_cache WHERE user_id = ?1 AND local_id = ?2",
            params![user_id, local_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(id.flatten())
}

/// This device's id for a row the relay has named — the reverse resolution, for
/// the paths that arrive holding the relay's number.
pub(crate) fn local_id_for(
    conn: &Connection,
    user_id: &str,
    relay_id: i64,
) -> Result<Option<i64>, AppError> {
    let id = conn
        .query_row(
            "SELECT local_id FROM entries_cache WHERE user_id = ?1 AND relay_id = ?2",
            params![user_id, relay_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(id)
}

/// Bring the rows the relay has ordered back inside both caps.
///
/// **Un-flushed rows are exempt.** Both caps measure from `last_use`, and a row
/// the relay has not stamped has none — it would age out on the write that
/// stored it. More to the point, the caps exist to bound a *cache* of what the
/// relay holds; an act still owed to the relay is undelivered content, and
/// discarding it to protect a display invariant is the trade ADR 0014 refuses.
///
/// `local_id` is the tiebreak rather than the relay's id, and has to be: it is
/// the only number every row has. The migration copies an installed cache in
/// relay-id order precisely so the two agree about relative age for everything
/// that predates it.
fn prune(conn: &Connection, user_id: &str, now_ms: i64) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM entries_cache
          WHERE user_id = ?1
            AND relay_id IS NOT NULL
            AND (
              last_use < (?2 - ?3)
              OR local_id NOT IN (
                SELECT local_id FROM entries_cache
                WHERE user_id = ?1 AND relay_id IS NOT NULL
                ORDER BY last_use DESC, local_id DESC
                LIMIT ?4
              )
            )",
        params![user_id, now_ms, MAX_AGE_MS, MAX_PER_USER],
    )?;
    Ok(())
}

/// Record that an entry the relay has named was used, without touching anything
/// else about it.
///
/// Keyed on `relay_id`, like [`upsert_and_prune`] and for the same reason: every
/// caller is holding the answer to a relay call it just made.
///
/// Deliberately does not prune. A use only ever *raises* one entry's Last Use,
/// so the set of prunable rows can only shrink; prune stays on insert, where
/// something new has arrived to make room for.
pub(crate) fn set_last_use(
    conn: &Connection,
    user_id: &str,
    relay_id: i64,
    last_use: i64,
) -> Result<usize, AppError> {
    let n = conn.execute(
        "UPDATE entries_cache SET last_use = ?3 WHERE user_id = ?1 AND relay_id = ?2",
        params![user_id, relay_id, last_use],
    )?;
    Ok(n)
}

/// The entry whose plaintext is exactly this text, if this device holds one.
///
/// The device makes this judgement because the relay cannot: `crypto::encrypt`
/// draws a fresh nonce every call, so the same plaintext never produces the
/// same ciphertext. An **Undecryptable** row has no plaintext and therefore no
/// hash, so it can never match.
///
/// Answers the row's own id, which is the only one every match has: a capture
/// the relay has not taken yet is exactly the case this has to recognise.
///
/// Ordered by Last Use so that, in the impossible-in-practice case of two rows
/// holding the same text, the one already at the head is the one used.
pub(crate) fn find_by_hash(
    conn: &Connection,
    user_id: &str,
    sha256: &str,
) -> Result<Option<i64>, AppError> {
    let id = conn
        .query_row(
            "SELECT local_id FROM entries_cache
              WHERE user_id = ?1 AND plaintext_sha256 = ?2
              ORDER BY last_use DESC, local_id DESC LIMIT 1",
            params![user_id, sha256],
            |r| r.get::<_, i64>(0),
        )
        .optional()?;
    Ok(id)
}

/// One page of the History, last use first.
///
/// `before` is the `(last_use, local_id)` of the last row of the previous page.
/// It is a tuple and not an id because id is not the order: paging by it alone
/// would skip and repeat rows the moment anything was used.
///
/// The tiebreak is `local_id`, and every id here is one: it is the only number
/// every row has, and a row that has not reached the relay still has to page.
pub fn list_recent(conn: &Connection, user_id: &str, before: Option<(i64, i64)>, limit: i64) -> Result<Vec<CachedEntry>, AppError> {
    let limit = limit.clamp(1, MAX_PER_USER);
    let mut rows: Vec<CachedEntry> = if let Some((last_use, id)) = before {
        let mut stmt = conn.prepare(
            "SELECT user_id, local_id, relay_id, ciphertext, plaintext, created_at, last_use, device_id
             FROM entries_cache
             WHERE user_id = ?1 AND (last_use < ?2 OR (last_use = ?2 AND local_id < ?3))
             ORDER BY last_use DESC, local_id DESC LIMIT ?4"
        )?;
        let out = stmt.query_map(params![user_id, last_use, id, limit], map_row)?.collect::<Result<Vec<_>, _>>()?;
        out
    } else {
        let mut stmt = conn.prepare(
            "SELECT user_id, local_id, relay_id, ciphertext, plaintext, created_at, last_use, device_id
             FROM entries_cache
             WHERE user_id = ?1
             ORDER BY last_use DESC, local_id DESC LIMIT ?2"
        )?;
        let out = stmt.query_map(params![user_id, limit], map_row)?.collect::<Result<Vec<_>, _>>()?;
        out
    };
    rows.shrink_to_fit();
    Ok(rows)
}

pub fn get_full(conn: &Connection, user_id: &str, local_id: i64) -> Result<Option<String>, AppError> {
    let pt: Option<Option<String>> = conn
        .query_row(
            "SELECT plaintext FROM entries_cache WHERE user_id = ?1 AND local_id = ?2",
            params![user_id, local_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?;
    Ok(pt.flatten())
}

/// Clears a cached plaintext once its entry stops decrypting.
///
/// Necessary because `upsert_and_prune` COALESCEs a NULL incoming plaintext
/// onto the stored one, which is right for a ciphertext-only backfill and
/// wrong for a decryption failure. `sync::decryptor::ingest` calls this on its
/// `undecryptable` branch so `get_full` stops handing back plaintext the app
/// has just told the user it cannot decrypt.
///
/// The hash goes with it, and must: a row this device can no longer read must
/// not go on matching copies of text it can no longer prove it holds.
pub(crate) fn mark_undecryptable(conn: &Connection, user_id: &str, relay_id: i64) -> Result<(), AppError> {
    conn.execute(
        "UPDATE entries_cache SET plaintext = NULL, plaintext_sha256 = NULL
          WHERE user_id = ?1 AND relay_id = ?2",
        params![user_id, relay_id],
    )?;
    Ok(())
}

pub fn delete_one(conn: &Connection, user_id: &str, local_id: i64) -> Result<usize, AppError> {
    let n = conn.execute(
        "DELETE FROM entries_cache WHERE user_id = ?1 AND local_id = ?2",
        params![user_id, local_id],
    )?;
    Ok(n)
}

pub fn delete_all(conn: &Connection, user_id: &str) -> Result<usize, AppError> {
    let n = conn.execute("DELETE FROM entries_cache WHERE user_id = ?1", params![user_id])?;
    Ok(n)
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<CachedEntry> {
    Ok(CachedEntry {
        user_id: r.get(0)?,
        local_id: r.get(1)?,
        relay_id: r.get(2)?,
        ciphertext: r.get(3)?,
        plaintext: r.get(4)?,
        created_at: r.get(5)?,
        last_use: r.get(6)?,
        device_id: r.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open_in_memory;

    /// Ingest one relay-delivered entry. `id` is the relay's, which is the only
    /// id this path knows; the answer is the row's own.
    fn ins(c: &Connection, user: &str, id: i64, pt: Option<&str>, ts: i64, now: i64) -> i64 {
        used(c, user, id, pt, ts, ts, now)
    }

    fn used(
        c: &Connection, user: &str, id: i64, pt: Option<&str>, ts: i64, last_use: i64, now: i64,
    ) -> i64 {
        let hash = pt.map(plaintext_sha256);
        upsert_and_prune(c, NewCachedEntry {
            user_id: user, relay_id: Some(id), ciphertext: b"ct", plaintext: pt,
            plaintext_sha256: hash.as_deref(), created_at: ts, last_use, device_id: "d1"
        }, now).unwrap().local_id
    }

    fn ids(c: &Connection, user: &str) -> Vec<i64> {
        list_recent(c, user, None, 200).unwrap().iter().filter_map(|r| r.relay_id).collect()
    }

    fn cursor(row: &CachedEntry) -> (i64, i64) {
        (row.last_use, row.local_id)
    }

    fn ids_of(rows: &[CachedEntry]) -> Vec<i64> {
        rows.iter().filter_map(|r| r.relay_id).collect()
    }

    #[test]
    fn list_returns_last_used_first() {
        let c = open_in_memory().unwrap();
        for i in 1..=3 { ins(&c, "u", i, Some(&format!("p{i}")), 1000 + i, 9_999); }
        assert_eq!(ids(&c, "u"), vec![3, 2, 1]);
    }

    /// The whole point of the ordering: an old capture that was just used leads
    /// a history of entries captured since.
    #[test]
    fn a_use_moves_an_old_entry_to_the_head() {
        let c = open_in_memory().unwrap();
        for i in 1..=3 { ins(&c, "u", i, Some(&format!("p{i}")), 1000 + i, 9_999); }
        assert_eq!(set_last_use(&c, "u", 1, 5_000).unwrap(), 1);
        assert_eq!(ids(&c, "u"), vec![1, 3, 2]);
    }

    /// Recalling the entry already at the head cannot move the order. It renews
    /// tenure, which is the half that matters to the age cap.
    #[test]
    fn using_the_head_renews_it_without_reordering() {
        let c = open_in_memory().unwrap();
        for i in 1..=3 { ins(&c, "u", i, None, 1000 + i, 9_999); }
        set_last_use(&c, "u", 3, 8_000).unwrap();
        assert_eq!(ids(&c, "u"), vec![3, 2, 1]);
        let head = list_recent(&c, "u", None, 1).unwrap().pop().unwrap();
        assert_eq!(head.last_use, 8_000);
        assert_eq!(head.created_at, 1003, "a use leaves identity alone");
    }

    #[test]
    fn caps_at_max_per_user() {
        let c = open_in_memory().unwrap();
        for i in 1..=105 { ins(&c, "u", i, None, i, 100_000); }
        let rows = list_recent(&c, "u", None, 200).unwrap();
        assert_eq!(rows.len() as i64, MAX_PER_USER);
        assert_eq!(rows.first().unwrap().relay_id, Some(105));
        assert_eq!(rows.last().unwrap().relay_id, Some(6));
    }

    /// The count cap measures from Last Use, so the row sitting at the top of
    /// the list is never the one evicted to make room.
    #[test]
    fn the_count_cap_keeps_a_used_entry_and_evicts_an_unused_newer_one() {
        let c = open_in_memory().unwrap();
        ins(&c, "u", 1, None, 1, 100_000);
        for i in 2..=MAX_PER_USER { ins(&c, "u", i, None, i, 100_000); }
        set_last_use(&c, "u", 1, 90_000).unwrap();
        // One more capture: something has to go, and it must not be entry 1.
        ins(&c, "u", 500, None, 500, 100_000);

        let kept = ids(&c, "u");
        assert_eq!(kept.len() as i64, MAX_PER_USER);
        assert_eq!(kept.first(), Some(&1), "the entry last used leads the history");
        assert!(!kept.contains(&2), "the least recently used entry is the one evicted");
    }

    #[test]
    fn evicts_old_by_age() {
        let c = open_in_memory().unwrap();
        let now = 100_000_000_000_i64;
        ins(&c, "u", 1, None, now - MAX_AGE_MS - 1, now);
        ins(&c, "u", 2, None, now, now);
        assert_eq!(ids(&c, "u"), vec![2]);
    }

    /// Thirty days now means thirty days since last use, so ciphertext in
    /// regular use is never aged out.
    #[test]
    fn the_age_cap_measures_from_last_use() {
        let c = open_in_memory().unwrap();
        let now = 100_000_000_000_i64;
        used(&c, "u", 1, None, now - MAX_AGE_MS - 1, now - 1000, now);
        ins(&c, "u", 2, None, now, now);
        assert_eq!(ids(&c, "u"), vec![2, 1], "an ancient capture used a second ago survives");
    }

    #[test]
    fn paging_with_a_last_use_and_id_cursor() {
        let c = open_in_memory().unwrap();
        for i in 1..=10 { ins(&c, "u", i, None, i, 9_999); }
        let first = list_recent(&c, "u", None, 3).unwrap();
        assert_eq!(ids_of(&first), vec![10, 9, 8]);
        let tail = first.last().unwrap();
        let page = list_recent(&c, "u", Some(cursor(tail)), 3).unwrap();
        assert_eq!(ids_of(&page), vec![7, 6, 5]);
    }

    /// Two entries sharing a millisecond still page without skipping or
    /// repeating, which is the whole reason the cursor carries the id.
    #[test]
    fn paging_is_total_when_two_entries_share_a_last_use() {
        let c = open_in_memory().unwrap();
        for i in 1..=4 { used(&c, "u", i, None, i, 500, 9_999); }
        let first = list_recent(&c, "u", None, 2).unwrap();
        assert_eq!(ids_of(&first), vec![4, 3]);
        let tail = first.last().unwrap();
        let page = list_recent(&c, "u", Some(cursor(tail)), 2).unwrap();
        assert_eq!(ids_of(&page), vec![2, 1]);
    }

    #[test]
    fn upsert_preserves_plaintext_when_new_one_is_null() {
        let c = open_in_memory().unwrap();
        ins(&c, "u", 1, Some("first"), 1, 9);
        ins(&c, "u", 1, None, 2, 9);
        assert_eq!(get_full(&c, "u", 1).unwrap().as_deref(), Some("first"));
        assert_eq!(
            find_by_hash(&c, "u", &plaintext_sha256("first")).unwrap(),
            Some(1),
            "and the hash survives a ciphertext-only re-ingest with it"
        );
    }

    #[test]
    fn find_by_hash_matches_exact_bytes_only() {
        let c = open_in_memory().unwrap();
        ins(&c, "u", 1, Some("https://example.test"), 1, 9);
        assert_eq!(find_by_hash(&c, "u", &plaintext_sha256("https://example.test")).unwrap(), Some(1));
        assert_eq!(
            find_by_hash(&c, "u", &plaintext_sha256("https://example.test\n")).unwrap(),
            None,
            "a trailing newline makes it a different entry"
        );
        assert_eq!(find_by_hash(&c, "other", &plaintext_sha256("https://example.test")).unwrap(), None);
    }

    #[test]
    fn mark_undecryptable_clears_plaintext_and_stops_it_matching() {
        let c = open_in_memory().unwrap();
        ins(&c, "u", 1, Some("secret"), 1, 9);
        mark_undecryptable(&c, "u", 1).unwrap();
        assert_eq!(get_full(&c, "u", 1).unwrap(), None);
        assert_eq!(find_by_hash(&c, "u", &plaintext_sha256("secret")).unwrap(), None);
    }

    #[test]
    fn set_last_use_reports_a_miss() {
        let c = open_in_memory().unwrap();
        assert_eq!(set_last_use(&c, "u", 404, 1).unwrap(), 0);
    }

    #[test]
    fn delete_one_and_all_scope_to_user() {
        let c = open_in_memory().unwrap();
        ins(&c, "a", 1, None, 1, 9);
        ins(&c, "b", 1, None, 1, 9);
        assert_eq!(delete_one(&c, "a", 1).unwrap(), 1);
        assert_eq!(delete_one(&c, "a", 1).unwrap(), 0);
        ins(&c, "a", 2, None, 2, 9);
        ins(&c, "a", 3, None, 3, 9);
        assert_eq!(delete_all(&c, "a").unwrap(), 2);
        assert_eq!(list_recent(&c, "b", None, 10).unwrap().len(), 1);
    }

    /*
     * A relay id names one row *per Pairing* and not per machine. Two pairings
     * on one device are two relays' numbering, and nothing says they do not
     * collide — a device paired to two relays would otherwise have one pairing's
     * entry overwrite the other's.
     */
    #[test]
    fn a_relay_id_is_unique_within_a_pairing_and_not_across_them() {
        let c = open_in_memory().unwrap();
        let theirs = ins(&c, "a", 1, Some("theirs"), 1, 9);
        let mine = ins(&c, "b", 1, Some("mine"), 1, 9);
        assert_ne!(theirs, mine, "one relay id, two rows");
        assert_eq!(get_full(&c, "a", theirs).unwrap().as_deref(), Some("theirs"));
        assert_eq!(get_full(&c, "b", mine).unwrap().as_deref(), Some("mine"));

        // Re-ingesting the same pairing's same relay id updates that one row.
        assert_eq!(ins(&c, "a", 1, Some("theirs, again"), 2, 9), theirs);
        assert_eq!(list_recent(&c, "a", None, 10).unwrap().len(), 1);
        assert_eq!(get_full(&c, "a", theirs).unwrap().as_deref(), Some("theirs, again"));
    }

    /*
     * An Entry exists before the relay names it (ADR 0013), so a capture is a
     * row from the moment it is made — findable, readable and deletable there,
     * with no relay id and nothing the caps can measure.
     */
    #[test]
    fn a_capture_is_a_row_before_the_relay_names_it() {
        let c = open_in_memory().unwrap();
        let hash = plaintext_sha256("offered offline");
        let local_id =
            insert_captured(&c, "u", b"sealed", "offered offline", &hash, "this-phone").unwrap();

        let rows = list_recent(&c, "u", None, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].local_id, local_id);
        assert_eq!(rows[0].relay_id, None, "the relay has not named it");
        assert_eq!(rows[0].created_at, 0, "and has not stamped it either");
        assert_eq!(rows[0].last_use, 0);
        assert_eq!(rows[0].device_id, "this-phone");
        assert_eq!(get_full(&c, "u", local_id).unwrap().as_deref(), Some("offered offline"));
        assert_eq!(find_by_hash(&c, "u", &hash).unwrap(), Some(local_id));
        assert_eq!(relay_id_for(&c, "u", local_id).unwrap(), None);

        // The relay takes it: same row, same id, now stamped.
        assert_eq!(attach_relay_id(&c, "u", local_id, 77, 5_000, 5_000, 9_999).unwrap(), 1);
        let settled = list_recent(&c, "u", None, 10).unwrap();
        assert_eq!(settled.len(), 1, "reconciliation, not insertion");
        assert_eq!(settled[0].local_id, local_id);
        assert_eq!(settled[0].relay_id, Some(77));
        assert_eq!(settled[0].created_at, 5_000);
        assert_eq!(relay_id_for(&c, "u", local_id).unwrap(), Some(77));
        assert_eq!(local_id_for(&c, "u", 77).unwrap(), Some(local_id));

        // And a second attempt reports nothing: the row is no longer un-named.
        assert_eq!(attach_relay_id(&c, "u", local_id, 78, 6_000, 6_000, 9_999).unwrap(), 0);
    }

    /*
     * The caps bound what the relay has ordered. An act still owed to the relay
     * is undelivered clipboard content, and evicting one to protect a display
     * invariant is the trade ADR 0014 refuses — so a hundred and one offline
     * captures are a hundred and one rows.
     */
    #[test]
    fn an_un_flushed_capture_is_never_evicted() {
        let c = open_in_memory().unwrap();
        let now = 100_000_000_000_i64;
        let text = "the first offline copy";
        let first =
            insert_captured(&c, "u", b"sealed", text, &plaintext_sha256(text), "d").unwrap();
        for i in 1..=MAX_PER_USER + 20 {
            ins(&c, "u", i, None, now, now);
        }
        // Counted off the table rather than a page: `list_recent` clamps to the
        // cap, and what is under test is what survives eviction.
        assert_eq!(
            get_full(&c, "u", first).unwrap().as_deref(),
            Some(text),
            "the un-flushed capture was evicted by rows the relay had already ordered"
        );
        let settled: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM entries_cache WHERE user_id = 'u' AND relay_id IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(settled, MAX_PER_USER, "and the settled region is still capped at a hundred");

        // The age cap is the same rule: nothing to measure, nothing to evict.
        ins(&c, "u", 9_999, None, now + MAX_AGE_MS * 2, now + MAX_AGE_MS * 2);
        assert_eq!(get_full(&c, "u", first).unwrap().as_deref(), Some(text));
    }

    /// The local id is the cache's own, and does not become the relay's by
    /// coincidence: it is dense per machine while relay ids are not.
    #[test]
    fn every_row_has_a_local_id_of_its_own() {
        let c = open_in_memory().unwrap();
        ins(&c, "u", 40, Some("first"), 1, 9);
        ins(&c, "u", 90, Some("second"), 2, 9);
        let rows = list_recent(&c, "u", None, 10).unwrap();
        assert_eq!(ids_of(&rows), vec![90, 40]);
        assert_eq!(
            rows.iter().map(|r| r.local_id).collect::<Vec<_>>(),
            vec![2, 1],
            "local ids follow insertion, not the relay"
        );

        // And re-ingesting leaves it where it was: a shell's row key must not
        // move because the relay said something about the row again.
        ins(&c, "u", 40, Some("first"), 1, 9);
        let again = list_recent(&c, "u", None, 10).unwrap();
        let held = again.iter().find(|r| r.relay_id == Some(40)).unwrap();
        assert_eq!(held.local_id, 1);
    }
}
