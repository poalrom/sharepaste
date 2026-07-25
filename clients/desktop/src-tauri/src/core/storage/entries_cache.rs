use crate::errors::AppError;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CachedEntry {
    pub(crate) user_id: String,
    pub(crate) id: i64,
    pub(crate) ciphertext: Vec<u8>,
    pub(crate) plaintext: Option<String>,
    pub(crate) created_at: i64,
    pub(crate) device_id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct NewCachedEntry<'a> {
    pub(crate) user_id: &'a str,
    pub(crate) id: i64,
    pub(crate) ciphertext: &'a [u8],
    pub(crate) plaintext: Option<&'a str>,
    pub(crate) created_at: i64,
    pub(crate) device_id: &'a str,
}

pub(crate) const MAX_PER_USER: i64 = 100;
pub(crate) const MAX_AGE_MS: i64 = 30 * 24 * 60 * 60 * 1000;

pub(crate) fn upsert_and_prune(conn: &Connection, e: NewCachedEntry<'_>, now_ms: i64) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO entries_cache (user_id, id, ciphertext, plaintext, created_at, device_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (user_id, id) DO UPDATE SET
            ciphertext = excluded.ciphertext,
            plaintext  = COALESCE(excluded.plaintext, entries_cache.plaintext),
            created_at = excluded.created_at,
            device_id  = excluded.device_id",
        params![e.user_id, e.id, e.ciphertext, e.plaintext, e.created_at, e.device_id],
    )?;
    tx.execute(
        "DELETE FROM entries_cache
          WHERE user_id = ?1
            AND (
              created_at < (?2 - ?3)
              OR id NOT IN (
                SELECT id FROM entries_cache
                WHERE user_id = ?1
                ORDER BY id DESC
                LIMIT ?4
              )
            )",
        params![e.user_id, now_ms, MAX_AGE_MS, MAX_PER_USER],
    )?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn list_recent(conn: &Connection, user_id: &str, before_id: Option<i64>, limit: i64) -> Result<Vec<CachedEntry>, AppError> {
    let limit = limit.clamp(1, MAX_PER_USER);
    let mut rows: Vec<CachedEntry> = if let Some(before) = before_id {
        let mut stmt = conn.prepare(
            "SELECT user_id, id, ciphertext, plaintext, created_at, device_id
             FROM entries_cache
             WHERE user_id = ?1 AND id < ?2
             ORDER BY id DESC LIMIT ?3"
        )?;
        let out = stmt.query_map(params![user_id, before, limit], map_row)?.collect::<Result<Vec<_>, _>>()?;
        out
    } else {
        let mut stmt = conn.prepare(
            "SELECT user_id, id, ciphertext, plaintext, created_at, device_id
             FROM entries_cache
             WHERE user_id = ?1
             ORDER BY id DESC LIMIT ?2"
        )?;
        let out = stmt.query_map(params![user_id, limit], map_row)?.collect::<Result<Vec<_>, _>>()?;
        out
    };
    rows.shrink_to_fit();
    Ok(rows)
}

pub(crate) fn get_full(conn: &Connection, user_id: &str, id: i64) -> Result<Option<String>, AppError> {
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
pub(crate) fn mark_undecryptable(conn: &Connection, user_id: &str, id: i64) -> Result<(), AppError> {
    conn.execute(
        "UPDATE entries_cache SET plaintext = NULL WHERE user_id = ?1 AND id = ?2",
        params![user_id, id],
    )?;
    Ok(())
}

pub(crate) fn delete_one(conn: &Connection, user_id: &str, id: i64) -> Result<usize, AppError> {
    let n = conn.execute(
        "DELETE FROM entries_cache WHERE user_id = ?1 AND id = ?2",
        params![user_id, id],
    )?;
    Ok(n)
}

pub(crate) fn delete_all(conn: &Connection, user_id: &str) -> Result<usize, AppError> {
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
        device_id: r.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::storage::open_in_memory;

    fn ins(c: &Connection, user: &str, id: i64, pt: Option<&str>, ts: i64, now: i64) {
        upsert_and_prune(c, NewCachedEntry {
            user_id: user, id, ciphertext: b"ct", plaintext: pt, created_at: ts, device_id: "d1"
        }, now).unwrap();
    }

    #[test]
    fn list_returns_newest_first() {
        let c = open_in_memory().unwrap();
        for i in 1..=3 { ins(&c, "u", i, Some(&format!("p{i}")), 1000 + i, 9_999); }
        let rows = list_recent(&c, "u", None, 10).unwrap();
        assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![3, 2, 1]);
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

    #[test]
    fn evicts_old_by_age() {
        let c = open_in_memory().unwrap();
        let now = 100_000_000_000_i64;
        ins(&c, "u", 1, None, now - MAX_AGE_MS - 1, now);
        ins(&c, "u", 2, None, now, now);
        let rows = list_recent(&c, "u", None, 10).unwrap();
        assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![2]);
    }

    #[test]
    fn paging_with_before_id() {
        let c = open_in_memory().unwrap();
        for i in 1..=10 { ins(&c, "u", i, None, i, 9_999); }
        let page = list_recent(&c, "u", Some(8), 3).unwrap();
        assert_eq!(page.iter().map(|r| r.id).collect::<Vec<_>>(), vec![7, 6, 5]);
    }

    #[test]
    fn upsert_preserves_plaintext_when_new_one_is_null() {
        let c = open_in_memory().unwrap();
        ins(&c, "u", 1, Some("first"), 1, 9);
        ins(&c, "u", 1, None, 2, 9);
        assert_eq!(get_full(&c, "u", 1).unwrap().as_deref(), Some("first"));
    }

    #[test]
    fn mark_undecryptable_clears_plaintext() {
        let c = open_in_memory().unwrap();
        ins(&c, "u", 1, Some("secret"), 1, 9);
        mark_undecryptable(&c, "u", 1).unwrap();
        assert_eq!(get_full(&c, "u", 1).unwrap(), None);
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
