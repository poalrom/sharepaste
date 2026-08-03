use crate::errors::AppError;
use crate::event::Queued;
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
    /// real (ADR 0016). So the row needs an identity the relay did not assign,
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
    /// Which region of the History this row is in, and where in it.
    ///
    /// Read by [`list_recent`] alone and carried out with the row because the
    /// facade needs both halves of it: the rank says what a shell draws, and the
    /// `(rank, ord)` pair is half of the cursor the next page resumes from.
    pub region: Region,
}

/// Where one row sits in the two-region order (ADR 0014).
///
/// The History is this device's pending acts, in the order it will send them,
/// above the entries the relay has ordered by last use. `rank` is which of the
/// three groups a row is in and `ord` is its place inside that group, so
/// `(rank ASC, ord DESC, local_id DESC)` is the whole order and the seam between
/// the regions is not a special case in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    /// 0 refused, 1 pending, 2 settled.
    pub rank: i64,
    /// The queue rowid for ranks 0 and 1; `last_use` for rank 2.
    pub ord: i64,
    /// What the relay said when it turned the act down. Rank 0 only.
    pub refused_reason: Option<String>,
}

impl Region {
    /// Whether an act against this row is still owed to the relay.
    pub fn pending(&self) -> bool {
        self.rank < RANK_SETTLED
    }

    /// What this row owes the relay, as everything above `storage/` says it.
    ///
    /// The three-rank encoding is decided by [`HISTORY`] and decoded here, in the
    /// same file, so a fourth rank cannot be added to that query and silently
    /// mis-read somewhere else. Its caller has no business knowing that rank 0 is
    /// the refused one.
    pub fn queued(self) -> Queued {
        match self.refused_reason {
            Some(reason) => Queued::Refused(reason),
            None if self.pending() => Queued::Pending,
            None => Queued::Settled,
        }
    }
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

/// The first rank the caps apply to, and the boundary [`Region::pending`] tests.
///
/// The ranks themselves are written into [`HISTORY`], one per arm, because each
/// arm also states the `ord` that rank is ordered by and the two belong together.
pub(crate) const RANK_SETTLED: i64 = 2;

/// The most rows one page may ask for.
///
/// Deliberately not [`MAX_PER_USER`]: that bounds the region the relay has
/// ordered, and the un-flushed region is unbounded on purpose — a hundred and
/// fifty offline captures are a hundred and fifty rows, and evicting one to
/// protect a number is the trade ADR 0014 refuses. A ceiling still exists so a
/// caller cannot ask for the whole table by accident.
pub(crate) const MAX_PAGE: i64 = 1_000;

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

/// How far one row has got, which is the whole of what a delete has to decide
/// from.
///
/// Three states and not `Option<i64>`: that collapses "there is no such row" into
/// "the relay has never named it", and the two want opposite treatment — one is an
/// error, the other is the withdraw that is this effort's whole point. Flattening
/// them made `delete_entry` answer `Ok(())` to a row that never existed, on a user
/// with no pairing at all.
pub(crate) enum Reach {
    /// No row with this id belongs to this user.
    Absent,
    /// This device holds it and the relay has never seen it: deleting withdraws
    /// it, and needs nothing in reach.
    Unflushed,
    /// The relay has it, under an id of its own.
    Flushed(i64),
}

/// How far one row has got. See [`Reach`].
pub(crate) fn reach_of(conn: &Connection, user_id: &str, local_id: i64) -> Result<Reach, AppError> {
    let row: Option<Option<i64>> = conn
        .query_row(
            "SELECT relay_id FROM entries_cache WHERE user_id = ?1 AND local_id = ?2",
            params![user_id, local_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(match row {
        None => Reach::Absent,
        Some(None) => Reach::Unflushed,
        Some(Some(relay_id)) => Reach::Flushed(relay_id),
    })
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
/// **Every row with an act still owed is exempt**, which is rank 2 and rank 2
/// alone competing for the hundred (ADR 0014). Two shapes qualify: a capture the
/// relay has not named, and an entry it *has* named that carries a queued **Use**.
/// The second is the one a `relay_id IS NULL` test would miss, and missing it is
/// worse than it looks: a three-week-old entry re-copied offline is at the top of
/// the list with the lowest `last_use` in the cache, so a full cache would evict
/// exactly the row the person just reached for and strand its act in the queue
/// with nothing to reconcile onto.
///
/// Both caps measure from `last_use`, and a row the relay has not stamped has none
/// — it would age out on the write that stored it. More to the point, the caps
/// exist to bound a *cache* of what the relay holds; an act still owed is
/// undelivered content, and discarding it to protect a display invariant is the
/// trade ADR 0014 refuses.
///
/// `local_id` is the tiebreak rather than the relay's id, and has to be: it is
/// the only number every row has. The migration copies an installed cache in
/// relay-id order precisely so the two agree about relative age for everything
/// that predates it.
fn prune(conn: &Connection, user_id: &str, now_ms: i64) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM entries_cache
          WHERE user_id = ?1
            AND local_id IN (
              SELECT e.local_id FROM entries_cache e
               WHERE e.user_id = ?1
                 AND e.relay_id IS NOT NULL
                 AND NOT EXISTS (
                       SELECT 1 FROM pending_uploads p
                        WHERE p.user_id = e.user_id AND p.local_entry_id = e.local_id)
            )
            AND (
              last_use < (?2 - ?3)
              OR local_id NOT IN (
                SELECT e.local_id FROM entries_cache e
                 WHERE e.user_id = ?1
                   AND e.relay_id IS NOT NULL
                   AND NOT EXISTS (
                         SELECT 1 FROM pending_uploads p
                          WHERE p.user_id = e.user_id AND p.local_entry_id = e.local_id)
                 ORDER BY e.last_use DESC, e.local_id DESC
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

/// The three regions of the History as one ordered list, by rank.
///
/// The order is `(rank ASC, ord DESC, local_id DESC)` and every arm supplies its
/// own `ord`, which is why this is a `UNION ALL` rather than a join with a
/// `CASE`: the three groups are ordered by three different facts, and each arm
/// says which one it is ordered by beside the rank that selects it.
///
/// `MAX(rowid)` for rank 1 and not the first: two offline re-copies of one entry
/// enqueue two acts, and the row has to rise on the second. The bare
/// `last_error` beside `MAX(rowid)` in rank 0 takes its value from the row that
/// produced the maximum — SQLite's documented behaviour for a single `min`/`max`
/// aggregate — which is the reason the refusal reason is readable here at all
/// without a second correlated lookup.
const HISTORY: &str = "
WITH history AS (
  SELECT e.user_id, e.local_id, e.relay_id, e.ciphertext, e.plaintext,
         e.created_at, e.last_use, e.device_id,
         0 AS rank, MAX(p.rowid) AS ord, p.last_error AS refused_reason
    FROM entries_cache e
    JOIN pending_uploads p
      ON p.user_id = e.user_id AND p.local_entry_id = e.local_id
         AND p.refused_at IS NOT NULL
   WHERE e.user_id = ?1
   GROUP BY e.local_id

  UNION ALL

  SELECT e.user_id, e.local_id, e.relay_id, e.ciphertext, e.plaintext,
         e.created_at, e.last_use, e.device_id,
         1 AS rank, MAX(p.rowid) AS ord, NULL AS refused_reason
    FROM entries_cache e
    JOIN pending_uploads p
      ON p.user_id = e.user_id AND p.local_entry_id = e.local_id
         AND p.refused_at IS NULL
   WHERE e.user_id = ?1
     AND NOT EXISTS (
           SELECT 1 FROM pending_uploads r
            WHERE r.user_id = e.user_id AND r.local_entry_id = e.local_id
              AND r.refused_at IS NOT NULL)
   GROUP BY e.local_id

  UNION ALL

  SELECT e.user_id, e.local_id, e.relay_id, e.ciphertext, e.plaintext,
         e.created_at, e.last_use, e.device_id,
         2 AS rank, e.last_use AS ord, NULL AS refused_reason
    FROM entries_cache e
   WHERE e.user_id = ?1
     AND NOT EXISTS (
           SELECT 1 FROM pending_uploads p
            WHERE p.user_id = e.user_id AND p.local_entry_id = e.local_id)
)
SELECT user_id, local_id, relay_id, ciphertext, plaintext, created_at, last_use,
       device_id, rank, ord, refused_reason
  FROM history
 WHERE ?2 = 0
    OR rank > ?3
    OR (rank = ?3 AND (ord < ?4 OR (ord = ?4 AND local_id < ?5)))
 ORDER BY rank ASC, ord DESC, local_id DESC
 LIMIT ?6
";

/// One page of the History: this device's pending acts, in the order it will
/// send them, above the entries the relay has ordered by last use (ADR 0014).
///
/// `before` is the `(rank, ord, local_id)` of the last row of the previous page.
/// Keyset paging reads the same three-part tuple the list is ordered by, so
/// crossing the seam between the regions is not a special case: the page after
/// the last pending row is the first settled one.
///
/// The seam does not move at a flush. The relay stamps a pending act on arrival
/// exactly where this device already showed it, so a row changes rank without
/// changing place.
pub fn list_recent(
    conn: &Connection,
    user_id: &str,
    before: Option<(i64, i64, i64)>,
    limit: i64,
) -> Result<Vec<CachedEntry>, AppError> {
    let limit = limit.clamp(1, MAX_PAGE);
    // `?2` is the presence of a cursor, so one statement serves both pages
    // rather than two copies of a fifty-line union drifting apart.
    let (has_cursor, rank, ord, id) = match before {
        Some((rank, ord, id)) => (1, rank, ord, id),
        None => (0, 0, 0, 0),
    };
    let mut stmt = conn.prepare(HISTORY)?;
    let mut rows: Vec<CachedEntry> = stmt
        .query_map(params![user_id, has_cursor, rank, ord, id, limit], map_row)?
        .collect::<Result<Vec<_>, _>>()?;
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
        region: Region {
            rank: r.get(8)?,
            ord: r.get(9)?,
            refused_reason: r.get(10)?,
        },
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

    fn cursor(row: &CachedEntry) -> (i64, i64, i64) {
        (row.region.rank, row.region.ord, row.local_id)
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
    fn paging_with_a_rank_ord_and_id_cursor() {
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

    /// A capture with its act queued against it, as `capture_or_use` makes one.
    fn capture(c: &Connection, user: &str, text: &str) -> i64 {
        let hash = plaintext_sha256(text);
        let local_id = insert_captured(c, user, b"sealed", text, &hash, "this-device").unwrap();
        crate::storage::pending::enqueue_capture(c, user, local_id, b"sealed", &hash, 1).unwrap();
        local_id
    }

    /// The relay taking one act, in the order the uploader does it: the act leaves
    /// the queue and only then is the relay's word attached.
    ///
    /// That order is load-bearing here as it is there — the prune inside
    /// `attach_relay_id` exempts a row with an act still owed, so attaching first
    /// would leave the row exempt for the write that is meant to bring the cache
    /// back inside its cap.
    fn flush(c: &Connection, user: &str, local_id: i64, relay_id: i64, at: i64) {
        crate::storage::pending::delete_for_entry(c, user, local_id).unwrap();
        attach_relay_id(c, user, local_id, relay_id, at, at, at).unwrap();
    }

    fn local_ids(rows: &[CachedEntry]) -> Vec<i64> {
        rows.iter().map(|r| r.local_id).collect()
    }

    /*
     * ADR 0014, as one list. The pending acts sit above the entries the relay has
     * ordered, in the order this device will send them, and the seam does not move
     * at the flush — the relay stamps a pending act exactly where the device
     * already showed it.
     */
    #[test]
    fn the_order_is_identical_either_side_of_a_flush() {
        let c = open_in_memory().unwrap();
        let now = 100_000_000_000_i64;
        // Two the relay already has, oldest first.
        let old = ins(&c, "u", 1, Some("a week ago"), now - 10_000, now);
        let newer = ins(&c, "u", 2, Some("yesterday"), now - 5_000, now);
        // And a burst made with no relay in reach.
        let one = capture(&c, "u", "burst one");
        let two = capture(&c, "u", "burst two");
        let three = capture(&c, "u", "burst three");

        let offline = list_recent(&c, "u", None, 50).unwrap();
        assert_eq!(
            local_ids(&offline),
            vec![three, two, one, newer, old],
            "the queue, newest act first, above what the relay has ordered"
        );
        assert_eq!(
            offline.iter().map(|r| r.region.rank).collect::<Vec<_>>(),
            vec![1, 1, 1, 2, 2]
        );

        // The relay takes them in queue order and stamps each one as it arrives,
        // which is what a flush is.
        for (n, local_id) in [one, two, three].into_iter().enumerate() {
            flush(&c, "u", local_id, 100 + n as i64, now + n as i64);
        }

        let flushed = list_recent(&c, "u", None, 50).unwrap();
        assert_eq!(
            local_ids(&flushed),
            local_ids(&offline),
            "the flush moved nothing: the same rows in the same places"
        );
        assert!(flushed.iter().all(|r| r.region.rank == RANK_SETTLED));
    }

    /*
     * `pending::find_by_hash` matches captures head-first, so two offline
     * re-copies of one entry enqueue two acts against it. `MAX(rowid)` and not the
     * first is what makes the row rise on the second one.
     */
    #[test]
    fn an_entry_re_copied_offline_twice_stays_at_the_top() {
        let c = open_in_memory().unwrap();
        let now = 100_000_000_000_i64;
        let old = ins(&c, "u", 1, Some("three weeks old"), now - 100_000, now);
        let later = capture(&c, "u", "captured after it");

        // Re-copying the old entry queues a use against it, which lifts it above
        // the capture made since.
        crate::storage::pending::enqueue_use(&c, "u", old, 1, 2).unwrap();
        assert_eq!(local_ids(&list_recent(&c, "u", None, 50).unwrap()), vec![old, later]);

        // Something else is copied, and then the old entry again.
        let newest = capture(&c, "u", "and something else");
        assert_eq!(
            local_ids(&list_recent(&c, "u", None, 50).unwrap()),
            vec![newest, old, later]
        );
        crate::storage::pending::enqueue_use(&c, "u", old, 1, 3).unwrap();
        assert_eq!(
            local_ids(&list_recent(&c, "u", None, 50).unwrap()),
            vec![old, newest, later],
            "the second re-copy has to lift it again, which the first act's rowid cannot say"
        );
    }

    /*
     * A hundred and fifty offline captures are a hundred and fifty rows: the caps
     * bound what the relay has ordered, and an act still owed to the relay is
     * undelivered content. After the flush the settled region is a hundred and the
     * ones that fell off are the oldest.
     */
    #[test]
    fn a_hundred_and_fifty_offline_captures_are_a_hundred_and_fifty_rows() {
        let c = open_in_memory().unwrap();
        let now = 100_000_000_000_i64;
        let ids: Vec<i64> = (0..150).map(|i| capture(&c, "u", &format!("copy {i}"))).collect();
        assert_eq!(list_recent(&c, "u", None, MAX_PAGE).unwrap().len(), 150);

        for (n, local_id) in ids.iter().enumerate() {
            flush(&c, "u", *local_id, 1_000 + n as i64, now + n as i64);
        }

        let after = list_recent(&c, "u", None, MAX_PAGE).unwrap();
        assert_eq!(after.len() as i64, MAX_PER_USER, "the settled region is capped");
        let kept = local_ids(&after);
        assert_eq!(kept.first(), Some(&ids[149]), "newest first");
        assert_eq!(kept.last(), Some(&ids[50]), "and the fifty oldest fell off");
    }

    /*
     * The seam is a page boundary like any other: a cursor taken from the last
     * pending row hands back the first settled one, with no gap and no repeat.
     */
    #[test]
    fn paging_crosses_the_seam_with_no_gap_and_no_repeat() {
        let c = open_in_memory().unwrap();
        let now = 100_000_000_000_i64;
        let settled: Vec<i64> =
            (1..=3).map(|i| ins(&c, "u", i, None, now - (10 - i) * 100, now)).collect();
        let pending: Vec<i64> = (0..2).map(|i| capture(&c, "u", &format!("queued {i}"))).collect();

        let whole = local_ids(&list_recent(&c, "u", None, 50).unwrap());
        let mut paged = Vec::new();
        let mut cursor_at = None;
        loop {
            let page = list_recent(&c, "u", cursor_at, 2).unwrap();
            if page.is_empty() {
                break;
            }
            cursor_at = Some(cursor(page.last().unwrap()));
            paged.extend(local_ids(&page));
        }
        assert_eq!(paged, whole, "paging two at a time reads exactly the same list");
        assert_eq!(paged.len(), pending.len() + settled.len());
    }

    /*
     * A refusal is not producible until the uploader can set `refused_at`, so the
     * rank the order gives it is exercised from a fixture: above every live
     * pending act, because it is the actionable one.
     */
    #[test]
    fn a_refused_act_ranks_above_the_live_ones_and_carries_its_reason() {
        let c = open_in_memory().unwrap();
        let refused = capture(&c, "u", "too large for this relay");
        let live = capture(&c, "u", "still on its way");
        c.execute(
            "UPDATE pending_uploads SET refused_at = 5, last_error = 'payload too large'
              WHERE local_entry_id = ?1",
            params![refused],
        )
        .unwrap();

        let rows = list_recent(&c, "u", None, 50).unwrap();
        assert_eq!(local_ids(&rows), vec![refused, live]);
        assert_eq!(rows[0].region.rank, 0);
        assert_eq!(rows[0].region.refused_reason.as_deref(), Some("payload too large"));
        assert!(rows[0].region.pending(), "a refused act is still owed");
        assert_eq!(rows[1].region.rank, 1);
        assert_eq!(rows[1].region.refused_reason, None);
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
     * An Entry exists before the relay names it (ADR 0016), so a capture is a
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
        assert!(matches!(reach_of(&c, "u", local_id).unwrap(), Reach::Unflushed));
        assert!(
            matches!(reach_of(&c, "u", local_id + 999).unwrap(), Reach::Absent),
            "and a row that is not here is not the same answer as one the relay has not named"
        );

        // The relay takes it: same row, same id, now stamped.
        assert_eq!(attach_relay_id(&c, "u", local_id, 77, 5_000, 5_000, 9_999).unwrap(), 1);
        let settled = list_recent(&c, "u", None, 10).unwrap();
        assert_eq!(settled.len(), 1, "reconciliation, not insertion");
        assert_eq!(settled[0].local_id, local_id);
        assert_eq!(settled[0].relay_id, Some(77));
        assert_eq!(settled[0].created_at, 5_000);
        assert!(matches!(reach_of(&c, "u", local_id).unwrap(), Reach::Flushed(77)));
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

    /*
     * The exemption a `relay_id IS NULL` test would miss, and the case that shows
     * why it matters. An entry the relay *has* named but that carries a queued
     * **Use** is rank 1, at the top of the list — and it holds the *lowest*
     * `last_use` in the cache, because that is what being three weeks old means.
     * Counting it against the hundred would evict exactly the row the person just
     * reached for, and strand its act in the queue with nothing to reconcile onto.
     */
    #[test]
    fn an_entry_with_a_queued_use_is_never_evicted_either() {
        let c = open_in_memory().unwrap();
        let now = 100_000_000_000_i64;
        // Three weeks old, and the oldest thing in the cache.
        let ancient = ins(&c, "u", 1, Some("wg genkey"), now - 21 * 86_400_000, now);
        // Re-copied with no relay in reach, which queues a use against it.
        crate::storage::pending::enqueue_use(&c, "u", ancient, 1, now).unwrap();

        // A full cache of things the relay has ordered since, then one more.
        for i in 2..=MAX_PER_USER + 1 {
            ins(&c, "u", i, None, now - MAX_PER_USER + i, now);
        }

        let rows = list_recent(&c, "u", None, MAX_PAGE).unwrap();
        assert_eq!(rows[0].local_id, ancient, "the re-copied entry leads the History");
        assert_eq!(rows[0].region.rank, 1);
        assert_eq!(
            get_full(&c, "u", ancient).unwrap().as_deref(),
            Some("wg genkey"),
            "and survived every prune the hundred rows after it provoked"
        );
        assert_eq!(
            rows.iter().filter(|r| r.region.rank == RANK_SETTLED).count() as i64,
            MAX_PER_USER,
            "while the settled region is still capped at exactly a hundred"
        );

        // Once the relay takes the use, the row is rank 2 like any other and takes
        // its chances with the cap — which is the whole point of the exemption
        // being about what is *owed* rather than about the row.
        crate::storage::pending::delete_for_entry(&c, "u", ancient).unwrap();
        set_last_use(&c, "u", 1, now).unwrap();
        ins(&c, "u", 9_000, None, now, now);
        assert!(
            get_full(&c, "u", ancient).unwrap().is_some(),
            "and the relay's stamp is what keeps it, now that it competes"
        );
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
