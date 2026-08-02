use crate::crypto::{decrypt, UserKey};
use crate::http::dto::EntryRow;
use crate::pairing::payload::base64_decode;
use crate::storage::entries_cache::{
    mark_undecryptable, plaintext_sha256, upsert_and_prune, NewCachedEntry, Stored,
};
use crate::errors::AppError;
use rusqlite::Connection;

pub struct DecryptOutcome {
    /// The whole decrypted text, or `None` when this row would not decrypt.
    ///
    /// Handed on whole, and the only statement of Undecryptable this makes: a
    /// separate flag beside it would be a second place for the same fact, which
    /// is what [`crate::event::Entry::new`] derives once and no caller repeats.
    /// The single-line **Preview** a list shows is [`crate::render::preview`]'s
    /// job, derived in the same place — deriving it here as well is how the two
    /// came to mean different things on the two ingest paths.
    pub plaintext: Option<String>,
    /// What the ingest did to the cache: whether this Entry is new here, and
    /// whether an Entry already here changed its place.
    ///
    /// Three paths reach the same id — a backfill, the relay's SSE echo, and
    /// the uploader caching an Entry this device created — and each owes the
    /// shell a different event. Exactly one may raise `EntryAdded`, or the list
    /// grows a duplicate row; only a **Use** may raise `HistoryChanged`, or
    /// every upload costs both shells a refetch for a reorder that did not
    /// happen.
    pub stored: Stored,
}

pub fn ingest(
    conn: &Connection,
    user_key: &UserKey,
    user_id: &str,
    row: &EntryRow,
    now_ms: i64,
) -> Result<DecryptOutcome, AppError> {
    let wire = base64_decode(&row.ciphertext)?;
    let plaintext_bytes = decrypt(user_key, user_id, &wire);
    let (plaintext_str, undecryptable) = match plaintext_bytes {
        Ok(b) => match String::from_utf8(b) {
            Ok(s) => (Some(s), false),
            Err(_) => (None, true),
        },
        Err(_) => (None, true),
    };
    // Only a plaintext this device can read has a hash, and only a hash makes a
    // repeat copy recognisable. An Undecryptable row must never match text this
    // device cannot prove it holds.
    let hash = plaintext_str.as_deref().map(plaintext_sha256);
    let stored = upsert_and_prune(conn, NewCachedEntry {
        user_id,
        id: row.id,
        ciphertext: &wire,
        plaintext: plaintext_str.as_deref(),
        plaintext_sha256: hash.as_deref(),
        created_at: row.created_at,
        last_use: row.last_use,
        device_id: &row.device_id,
    }, now_ms)?;
    if undecryptable {
        // upsert_and_prune COALESCEs a NULL plaintext onto whatever is already
        // stored, so an entry that decrypted once and no longer does would keep
        // serving its old plaintext through get_full - and therefore through
        // copy_to_clipboard - while this same ingest reports the row as
        // Undecryptable. Clear it so the cache agrees with what we tell the user.
        mark_undecryptable(conn, user_id, row.id)?;
    }
    Ok(DecryptOutcome { plaintext: plaintext_str, stored })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::encrypt;
    use crate::pairing::payload::base64_encode;
    use crate::storage::open_in_memory;
    use zeroize::Zeroizing;

    fn key() -> UserKey { Zeroizing::new([5u8; 32]) }

    fn row_for(uid: &str, id: i64, plaintext: &[u8], k: &UserKey) -> EntryRow {
        let wire = encrypt(k, uid, plaintext).unwrap();
        EntryRow {
            id,
            ciphertext: base64_encode(&wire),
            created_at: 1000,
            device_id: "d".into(),
            seq: id,
            last_use: 1000,
        }
    }

    /*
     * Whole, and not a Preview of it. An ingest that flattened the text here
     * would leave the reader pane and the search with nothing to read.
     */
    #[test]
    fn ingest_decryptable_carries_the_whole_plaintext() {
        let c = open_in_memory().unwrap();
        let k = key();
        let multiline = "ssh admin@10.0.0.4\n  -i ~/.ssh/id_ed25519\n  -p 2222";
        let r = row_for("u", 1, multiline.as_bytes(), &k);
        let out = ingest(&c, &k, "u", &r, 9_999).unwrap();
        assert_eq!(out.plaintext.as_deref(), Some(multiline));
        let pt = crate::storage::entries_cache::get_full(&c, "u", 1).unwrap();
        assert_eq!(pt.as_deref(), Some(multiline));
    }

    #[test]
    fn ingest_aad_mismatch_marks_undecryptable() {
        let c = open_in_memory().unwrap();
        let k = key();
        let r = row_for("alice", 1, b"x", &k);
        let out = ingest(&c, &k, "bob", &r, 9_999).unwrap();
        assert!(out.plaintext.is_none(), "no plaintext is what Undecryptable means here");
        let pt = crate::storage::entries_cache::get_full(&c, "bob", 1).unwrap();
        assert!(pt.is_none());
    }

    #[test]
    fn ingest_clears_stale_plaintext_when_an_entry_stops_decrypting() {
        let c = open_in_memory().unwrap();
        let good = key();
        let r = row_for("u", 1, b"secret", &good);
        ingest(&c, &good, "u", &r, 9_999).unwrap();
        assert_eq!(
            crate::storage::entries_cache::get_full(&c, "u", 1).unwrap().as_deref(),
            Some("secret"),
            "precondition: the entry decrypted and was cached"
        );

        // Same id, re-ingested under a rotated key: decryption now fails.
        let rotated: UserKey = Zeroizing::new([6u8; 32]);
        let reencrypted = row_for("u", 1, b"secret", &rotated);
        let out = ingest(&c, &good, "u", &reencrypted, 10_000).unwrap();

        assert!(out.plaintext.is_none(), "and the row this ingest reports is Undecryptable now");
        assert!(
            crate::storage::entries_cache::get_full(&c, "u", 1).unwrap().is_none(),
            "and must not keep serving the old plaintext through copy_to_clipboard"
        );
    }

    /// A **Use** arrives as the same row with a later `last_use`. It reorders
    /// the History and raises nothing: `first_insert` is false, so the caller
    /// does not announce an `EntryAdded` for an entry that was already there.
    #[test]
    fn ingesting_a_use_moves_the_entry_without_announcing_it_again() {
        let c = open_in_memory().unwrap();
        let k = key();
        let captured = row_for("u", 1, b"ssh admin@10.0.0.4", &k);
        assert!(ingest(&c, &k, "u", &captured, 9_999).unwrap().stored.first_insert);

        let mut used = row_for("u", 1, b"ssh admin@10.0.0.4", &k);
        used.seq = 55;
        used.last_use = 8_000;
        let out = ingest(&c, &k, "u", &used, 9_999).unwrap();

        assert!(!out.stored.first_insert);
        let head = crate::storage::entries_cache::list_recent(&c, "u", None, 1).unwrap();
        assert_eq!(head[0].last_use, 8_000);
        assert_eq!(head[0].created_at, 1000, "a use leaves identity alone");
    }

    /// The hash the matcher searches is written by the one ingest path there
    /// is, so a repeat copy is recognised however the entry arrived.
    #[test]
    fn ingest_records_the_hash_of_what_it_decrypted() {
        use crate::storage::entries_cache::{find_by_hash, plaintext_sha256};
        let c = open_in_memory().unwrap();
        let k = key();
        ingest(&c, &k, "u", &row_for("u", 1, b"npm run dev", &k), 9_999).unwrap();
        assert_eq!(find_by_hash(&c, "u", &plaintext_sha256("npm run dev")).unwrap(), Some(1));

        // And an Undecryptable one records none: it cannot prove what it holds.
        let rotated: UserKey = Zeroizing::new([6u8; 32]);
        ingest(&c, &k, "u", &row_for("u", 2, b"npm run build", &rotated), 9_999).unwrap();
        assert_eq!(find_by_hash(&c, "u", &plaintext_sha256("npm run build")).unwrap(), None);
    }
}
