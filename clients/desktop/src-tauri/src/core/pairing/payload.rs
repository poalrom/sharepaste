use crate::core::crypto::{decrypt, encrypt, UserKey};
use crate::core::http::ServerClient;
use crate::core::pairing::shortcode::{encode, ShortcodePayload};
use crate::errors::AppError;
use data_encoding::{BASE64, HEXLOWER};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PairPayload {
    pub user_id: String,
    pub user_key: String,        // hex
    pub server_url: String,
}

pub struct PairStarted {
    pub pair_id: Uuid,
    pub pairing_secret: Zeroizing<[u8; 32]>,
    pub shortcode: String,
}

pub async fn start_pair(server: &ServerClient) -> Result<PairStarted, AppError> {
    let mut secret = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut secret);
    let secret_hex = HEXLOWER.encode(&secret);
    let secret_hash = sha256_hex(secret_hex.as_bytes());

    let resp = server.pair_start(&secret_hash).await?;
    let pair_id = Uuid::parse_str(&resp.pair_id)
        .map_err(|e| AppError::BadInput(format!("server returned malformed pair_id: {e}")))?;
    let payload = ShortcodePayload {
        server_url: server.base().to_string(),
        pair_id,
        pairing_secret: secret,
    };
    let shortcode = encode(&payload)?;
    Ok(PairStarted {
        pair_id,
        pairing_secret: Zeroizing::new(secret),
        shortcode,
    })
}

pub async fn upload_pair_payload(
    server: &ServerClient,
    pair_id: Uuid,
    pairing_secret: &[u8; 32],
    user_id: &str,
    user_key: &UserKey,
    server_url: &str,
) -> Result<(), AppError> {
    let payload = PairPayload {
        user_id: user_id.into(),
        user_key: crate::core::pairing::invite::hex::encode_user_key(user_key).to_string(),
        server_url: server_url.into(),
    };
    let plaintext = serde_json::to_vec(&payload).map_err(|e| AppError::Crypto(e.to_string()))?;
    let key: UserKey = Zeroizing::new(*pairing_secret);
    let wire = encrypt(&key, &pair_id.to_string(), &plaintext)?;
    let b64 = base64_encode(&wire);
    server.pair_payload_put(&pair_id.to_string(), &b64).await
}

pub async fn fetch_and_decrypt_pair_payload(
    server: &ServerClient,
    pair_id: Uuid,
    pairing_secret: &[u8; 32],
) -> Result<PairPayload, AppError> {
    let secret_hex = HEXLOWER.encode(pairing_secret);
    let resp = server.pair_payload_get(&pair_id.to_string(), &secret_hex).await?;
    let wire = base64_decode(&resp.encrypted_payload)?;
    let key: UserKey = Zeroizing::new(*pairing_secret);
    let plaintext = decrypt(&key, &pair_id.to_string(), &wire)?;
    serde_json::from_slice(&plaintext).map_err(|e| AppError::Crypto(e.to_string()))
}

pub fn secret_proof_hex(pairing_secret: &[u8; 32]) -> String {
    HEXLOWER.encode(pairing_secret)
}

fn sha256_hex(input: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(input);
    HEXLOWER.encode(&h.finalize())
}

pub fn base64_encode(bytes: &[u8]) -> String {
    BASE64.encode(bytes)
}

pub fn base64_decode(s: &str) -> Result<Vec<u8>, AppError> {
    BASE64.decode(s.as_bytes()).map_err(|e| AppError::BadInput(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_payload_round_trips_through_aead() {
        let secret = [9u8; 32];
        let pair_id = Uuid::new_v4();
        let payload = PairPayload {
            user_id: "u".into(),
            user_key: "ab".repeat(32),
            server_url: "https://srv".into(),
        };
        let plaintext = serde_json::to_vec(&payload).unwrap();
        let key: UserKey = Zeroizing::new(secret);
        let wire = encrypt(&key, &pair_id.to_string(), &plaintext).unwrap();
        let back = decrypt(&key, &pair_id.to_string(), &wire).unwrap();
        let parsed: PairPayload = serde_json::from_slice(&back).unwrap();
        assert_eq!(parsed, payload);
    }

    #[test]
    fn secret_proof_is_lowercase_hex() {
        let s = secret_proof_hex(&[0xAB; 32]);
        assert_eq!(s.len(), 64);
        assert!(s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }

    #[test]
    fn hex_output_matches_wire_golden_values() {
        assert_eq!(
            secret_proof_hex(&[0xAB; 32]),
            "abababababababababababababababababababababababababababababababab"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(secret_proof_hex(&[0x0A; 32]).as_bytes()),
            "2db6b2a7b2fbb5fbf2ea7fcbacb5e1645fb6bf3c0d606ce5b88a23533b32d4b8"
        );
    }
}
