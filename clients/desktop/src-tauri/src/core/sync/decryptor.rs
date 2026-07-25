use crate::core::crypto::{decrypt, UserKey};
use crate::core::http::dto::EntryRow;
use crate::core::pairing::payload::base64_decode;
use crate::core::storage::entries_cache::{upsert_and_prune, NewCachedEntry};
use crate::errors::AppError;
use rusqlite::Connection;

pub struct DecryptOutcome {
    pub plaintext_preview: Option<String>,
    pub undecryptable: bool,
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
    upsert_and_prune(conn, NewCachedEntry {
        user_id,
        id: row.id,
        ciphertext: &wire,
        plaintext: plaintext_str.as_deref(),
        created_at: row.created_at,
        device_id: &row.device_id,
    }, now_ms)?;
    let preview = plaintext_str.as_ref().map(|s| build_preview(s));
    Ok(DecryptOutcome { plaintext_preview: preview, undecryptable })
}

pub fn build_preview(plaintext: &str) -> String {
    let one_line: String = plaintext
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let trimmed = one_line.trim();
    let mut out = String::new();
    for (i, c) in trimmed.chars().enumerate() {
        if i >= 80 { break; }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::crypto::encrypt;
    use crate::core::pairing::payload::base64_encode;
    use crate::core::storage::open_in_memory;
    use zeroize::Zeroizing;

    fn key() -> UserKey { Zeroizing::new([5u8; 32]) }

    fn row_for(uid: &str, id: i64, plaintext: &[u8], k: &UserKey) -> EntryRow {
        let wire = encrypt(k, uid, plaintext).unwrap();
        EntryRow { id, ciphertext: base64_encode(&wire), created_at: 1000, device_id: "d".into() }
    }

    #[test]
    fn ingest_decryptable_writes_plaintext() {
        let c = open_in_memory().unwrap();
        let k = key();
        let r = row_for("u", 1, b"hello", &k);
        let out = ingest(&c, &k, "u", &r, 9_999).unwrap();
        assert_eq!(out.plaintext_preview.as_deref(), Some("hello"));
        assert!(!out.undecryptable);
        let pt = crate::core::storage::entries_cache::get_full(&c, "u", 1).unwrap();
        assert_eq!(pt.as_deref(), Some("hello"));
    }

    #[test]
    fn ingest_aad_mismatch_marks_undecryptable() {
        let c = open_in_memory().unwrap();
        let k = key();
        let r = row_for("alice", 1, b"x", &k);
        let out = ingest(&c, &k, "bob", &r, 9_999).unwrap();
        assert!(out.undecryptable);
        let pt = crate::core::storage::entries_cache::get_full(&c, "bob", 1).unwrap();
        assert!(pt.is_none());
    }

    #[test]
    fn preview_strips_controls_and_truncates_at_80() {
        let s: String = "a\nb\tc".chars().chain(std::iter::repeat('z').take(200)).collect();
        let p = build_preview(&s);
        assert!(p.len() <= 80 + 80); // 80 chars, none multi-byte here
        assert!(!p.contains('\n'));
    }
}
