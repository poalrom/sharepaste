use crate::errors::AppError;
use data_encoding::HEXLOWER;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq)]
pub struct CachedEntry {
    pub user_id: String,
    pub id: i64,
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
    pub(crate) id: i64,
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
/// Two facts and not one, because three paths reach the same id and each owes
/// the shell a different event. `first_insert` says an Entry is new here, and
/// exactly one path may raise `EntryAdded` for it. `moved` says an Entry
/// already here changed its place, which is what `HistoryChanged` reports.
///
/// The pair is what tells a **Use** apart from the relay's echo of an Entry
/// this device uploaded: both are repeat ingests of a row the cache holds, and
/// only the use carries a later Last Use. Deriving "it moved" from
/// `!first_insert` would make every one of this device's own uploads announce a
/// reorder that did not happen, and cost both shells a full refetch each time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stored {
    pub first_insert: bool,
    pub moved: bool,
}

/// Store one Entry, then bring the user's cache back inside its caps.
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
            "SELECT last_use FROM entries_cache WHERE user_id = ?1 AND id = ?2",
            params![e.user_id, e.id],
            |r| r.get(0),
        )
        .optional()?;
    tx.execute(
        "INSERT INTO entries_cache
            (user_id, id, ciphertext, plaintext, plaintext_sha256, created_at, last_use, device_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT (user_id, id) DO UPDATE SET
            ciphertext       = excluded.ciphertext,
            plaintext        = COALESCE(excluded.plaintext, entries_cache.plaintext),
            plaintext_sha256 = COALESCE(excluded.plaintext_sha256, entries_cache.plaintext_sha256),
            created_at       = excluded.created_at,
            last_use         = excluded.last_use,
            device_id        = excluded.device_id",
        params![
            e.user_id, e.id, e.ciphertext, e.plaintext, e.plaintext_sha256,
            e.created_at, e.last_use, e.device_id
        ],
    )?;
    tx.execute(
        "DELETE FROM entries_cache
          WHERE user_id = ?1
            AND (
              last_use < (?2 - ?3)
              OR id NOT IN (
                SELECT id FROM entries_cache
                WHERE user_id = ?1
                ORDER BY last_use DESC, id DESC
                LIMIT ?4
              )
            )",
        params![e.user_id, now_ms, MAX_AGE_MS, MAX_PER_USER],
    )?;
    tx.commit()?;
    Ok(Stored {
        first_insert: held.is_none(),
        moved: held.is_some_and(|was| was != e.last_use),
    })
}

/// Record that an entry was used, without touching anything else about it.
///
/// Deliberately does not prune. A use only ever *raises* one entry's Last Use,
/// so the set of prunable rows can only shrink; prune stays on insert, where
/// something new has arrived to make room for.
pub(crate) fn set_last_use(
    conn: &Connection,
    user_id: &str,
    id: i64,
    last_use: i64,
) -> Result<usize, AppError> {
    let n = conn.execute(
        "UPDATE entries_cache SET last_use = ?3 WHERE user_id = ?1 AND id = ?2",
        params![user_id, id, last_use],
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
/// Ordered by Last Use so that, in the impossible-in-practice case of two rows
/// holding the same text, the one already at the head is the one used.
pub(crate) fn find_by_hash(
    conn: &Connection,
    user_id: &str,
    sha256: &str,
) -> Result<Option<i64>, AppError> {
    let id = conn
        .query_row(
            "SELECT id FROM entries_cache
              WHERE user_id = ?1 AND plaintext_sha256 = ?2
              ORDER BY last_use DESC, id DESC LIMIT 1",
            params![user_id, sha256],
            |r| r.get::<_, i64>(0),
        )
        .optional()?;
    Ok(id)
}

/// One page of the History, last use first.
///
/// `before` is the `(last_use, id)` of the last row of the previous page. It is
/// a tuple and not an id because id is no longer the order: paging by it alone
/// would skip and repeat rows the moment anything was used.
pub fn list_recent(conn: &Connection, user_id: &str, before: Option<(i64, i64)>, limit: i64) -> Result<Vec<CachedEntry>, AppError> {
    let limit = limit.clamp(1, MAX_PER_USER);
    let mut rows: Vec<CachedEntry> = if let Some((last_use, id)) = before {
        let mut stmt = conn.prepare(
            "SELECT user_id, id, ciphertext, plaintext, created_at, last_use, device_id
             FROM entries_cache
             WHERE user_id = ?1 AND (last_use < ?2 OR (last_use = ?2 AND id < ?3))
             ORDER BY last_use DESC, id DESC LIMIT ?4"
        )?;
        let out = stmt.query_map(params![user_id, last_use, id, limit], map_row)?.collect::<Result<Vec<_>, _>>()?;
        out
    } else {
        let mut stmt = conn.prepare(
            "SELECT user_id, id, ciphertext, plaintext, created_at, last_use, device_id
             FROM entries_cache
             WHERE user_id = ?1
             ORDER BY last_use DESC, id DESC LIMIT ?2"
        )?;
        let out = stmt.query_map(params![user_id, limit], map_row)?.collect::<Result<Vec<_>, _>>()?;
        out
    };
    rows.shrink_to_fit();
    Ok(rows)
}

pub fn get_full(conn: &Connection, user_id: &str, id: i64) -> Result<Option<String>, AppError> {
    let pt: Option<Option<String>> = conn
        .query_row(
            "SELECT plaintext FROM entries_cache WHERE user_id = ?1 AND id = ?2",
            params![user_id, id],
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
pub(crate) fn mark_undecryptable(conn: &Connection, user_id: &str, id: i64) -> Result<(), AppError> {
    conn.execute(
        "UPDATE entries_cache SET plaintext = NULL, plaintext_sha256 = NULL
          WHERE user_id = ?1 AND id = ?2",
        params![user_id, id],
    )?;
    Ok(())
}

pub fn delete_one(conn: &Connection, user_id: &str, id: i64) -> Result<usize, AppError> {
    let n = conn.execute(
        "DELETE FROM entries_cache WHERE user_id = ?1 AND id = ?2",
        params![user_id, id],
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
        id: r.get(1)?,
        ciphertext: r.get(2)?,
        plaintext: r.get(3)?,
        created_at: r.get(4)?,
        last_use: r.get(5)?,
        device_id: r.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open_in_memory;

    fn ins(c: &Connection, user: &str, id: i64, pt: Option<&str>, ts: i64, now: i64) {
        used(c, user, id, pt, ts, ts, now);
    }

    fn used(c: &Connection, user: &str, id: i64, pt: Option<&str>, ts: i64, last_use: i64, now: i64) {
        let hash = pt.map(plaintext_sha256);
        upsert_and_prune(c, NewCachedEntry {
            user_id: user, id, ciphertext: b"ct", plaintext: pt,
            plaintext_sha256: hash.as_deref(), created_at: ts, last_use, device_id: "d1"
        }, now).unwrap();
    }

    fn ids(c: &Connection, user: &str) -> Vec<i64> {
        list_recent(c, user, None, 200).unwrap().iter().map(|r| r.id).collect()
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
        assert_eq!(rows.first().unwrap().id, 105);
        assert_eq!(rows.last().unwrap().id, 6);
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
        assert_eq!(first.iter().map(|r| r.id).collect::<Vec<_>>(), vec![10, 9, 8]);
        let tail = first.last().unwrap();
        let page = list_recent(&c, "u", Some((tail.last_use, tail.id)), 3).unwrap();
        assert_eq!(page.iter().map(|r| r.id).collect::<Vec<_>>(), vec![7, 6, 5]);
    }

    /// Two entries sharing a millisecond still page without skipping or
    /// repeating, which is the whole reason the cursor carries the id.
    #[test]
    fn paging_is_total_when_two_entries_share_a_last_use() {
        let c = open_in_memory().unwrap();
        for i in 1..=4 { used(&c, "u", i, None, i, 500, 9_999); }
        let first = list_recent(&c, "u", None, 2).unwrap();
        assert_eq!(first.iter().map(|r| r.id).collect::<Vec<_>>(), vec![4, 3]);
        let tail = first.last().unwrap();
        let page = list_recent(&c, "u", Some((tail.last_use, tail.id)), 2).unwrap();
        assert_eq!(page.iter().map(|r| r.id).collect::<Vec<_>>(), vec![2, 1]);
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
}
