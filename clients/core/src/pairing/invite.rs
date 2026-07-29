use crate::crypto::{random_user_key, UserKey};
use crate::http::ServerClient;
use crate::keychain::{token_account, user_key_account, Keychain};
use crate::storage::accounts::{upsert as upsert_account, Account};
use crate::errors::AppError;
use rusqlite::Connection;

pub struct ClaimedPairing {
    pub user_id: String,
    pub device_id: String,
    pub device_token: String,
    pub server_url: String,
    pub user_key: UserKey,
}

pub async fn claim_invite(
    server: &ServerClient,
    token: &str,
    device_label: &str,
) -> Result<ClaimedPairing, AppError> {
    let resp = server.claim_invite(token, device_label).await?;
    Ok(ClaimedPairing {
        user_id: resp.user_id,
        device_id: resp.device_id,
        device_token: resp.device_token,
        server_url: server.base().to_string(),
        user_key: random_user_key(),
    })
}

pub fn persist_claimed_pairing(
    conn: &Connection,
    keychain: &dyn Keychain,
    claimed: &ClaimedPairing,
    device_label: &str,
    now_ms: i64,
) -> Result<(), AppError> {
    keychain.put(&user_key_account(&claimed.user_id), &hex::encode_user_key(&claimed.user_key))?;
    keychain.put(&token_account(&claimed.user_id), &claimed.device_token)?;
    upsert_account(conn, &Account {
        user_id: claimed.user_id.clone(),
        device_id: claimed.device_id.clone(),
        device_label: device_label.into(),
        server_url: claimed.server_url.clone(),
        last_seen_id: 0,
        created_at: now_ms,
        username: None,
        last_contact_at: None,
    })?;
    Ok(())
}

pub mod hex {
    use crate::crypto::UserKey;
    use zeroize::Zeroizing;

    pub(crate) fn encode_user_key(k: &UserKey) -> Zeroizing<String> {
        let mut s = String::with_capacity(k.len() * 2);
        for b in k.iter() {
            use std::fmt::Write;
            write!(&mut s, "{:02x}", b).unwrap();
        }
        Zeroizing::new(s)
    }

    pub fn decode_user_key(s: &str) -> Result<UserKey, crate::errors::AppError> {
        if s.len() != 64 {
            return Err(crate::errors::AppError::Crypto("user_key must be 64 hex chars".into()));
        }
        let mut out = [0u8; 32];
        for (i, b) in out.iter_mut().enumerate() {
            *b = u8::from_str_radix(&s[i*2..i*2+2], 16)
                .map_err(|e| crate::errors::AppError::Crypto(e.to_string()))?;
        }
        Ok(Zeroizing::new(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keychain::InMemoryKeychain;
    use crate::storage::open_in_memory;

    #[test]
    fn persist_writes_keychain_and_db() {
        let conn = open_in_memory().unwrap();
        let kc = InMemoryKeychain::default();
        let claimed = ClaimedPairing {
            user_id: "u".into(),
            device_id: "d".into(),
            device_token: "tok".into(),
            server_url: "https://srv".into(),
            user_key: crate::crypto::random_user_key(),
        };
        persist_claimed_pairing(&conn, &kc, &claimed, "mac", 12345).unwrap();
        assert_eq!(kc.get("u:token").unwrap().as_deref(), Some("tok"));
        assert!(kc.get("u:key").unwrap().is_some());
        let row = crate::storage::accounts::find(&conn, "u").unwrap().unwrap();
        assert_eq!(row.device_id, "d");
        assert_eq!(row.device_label, "mac");
    }

    #[test]
    fn user_key_hex_round_trip() {
        let k = crate::crypto::random_user_key();
        let s = hex::encode_user_key(&k);
        let back = hex::decode_user_key(&s).unwrap();
        assert_eq!(k.as_slice(), back.as_slice());
    }

    #[test]
    fn user_key_hex_rejects_bad_length() {
        assert!(hex::decode_user_key("ab").is_err());
    }
}
