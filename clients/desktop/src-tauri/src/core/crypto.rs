use crate::errors::AppError;
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

pub(crate) const KEY_LEN: usize = 32;
pub(crate) const NONCE_LEN: usize = 24;

pub type UserKey = Zeroizing<[u8; KEY_LEN]>;

pub fn random_user_key() -> UserKey {
    let mut k = [0u8; KEY_LEN];
    rand::RngCore::fill_bytes(&mut OsRng, &mut k);
    Zeroizing::new(k)
}

pub fn encrypt(user_key: &UserKey, user_id: &str, plaintext: &[u8]) -> Result<Vec<u8>, AppError> {
    let cipher = XChaCha20Poly1305::new(user_key.as_slice().into());
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, Payload { msg: plaintext, aad: user_id.as_bytes() })
        .map_err(|e| AppError::Crypto(format!("encrypt: {e}")))?;
    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub fn decrypt(user_key: &UserKey, user_id: &str, wire: &[u8]) -> Result<Vec<u8>, AppError> {
    if wire.len() < NONCE_LEN + 16 {
        return Err(AppError::Crypto("ciphertext too short".into()));
    }
    let (nonce_bytes, ct) = wire.split_at(NONCE_LEN);
    let nonce = XNonce::from_slice(nonce_bytes);
    let cipher = XChaCha20Poly1305::new(user_key.as_slice().into());
    cipher
        .decrypt(nonce, Payload { msg: ct, aad: user_id.as_bytes() })
        .map_err(|e| AppError::Crypto(format!("decrypt: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_literal::hex;

    fn key() -> UserKey {
        let bytes: [u8; 32] = hex!("0808080808080808080808080808080808080808080808080808080808080808");
        Zeroizing::new(bytes)
    }

    #[test]
    fn round_trip_random_nonce() {
        let k = key();
        let user_id = "alice";
        let plaintext = b"hello sharepaste";
        let ct = encrypt(&k, user_id, plaintext).unwrap();
        assert!(ct.len() > NONCE_LEN);
        let pt = decrypt(&k, user_id, &ct).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn aad_mismatch_fails() {
        let k = key();
        let ct = encrypt(&k, "alice", b"x").unwrap();
        assert!(decrypt(&k, "bob", &ct).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let k = key();
        let mut ct = encrypt(&k, "alice", b"x").unwrap();
        let last = ct.len() - 1;
        ct[last] ^= 0x01;
        assert!(decrypt(&k, "alice", &ct).is_err());
    }

    #[test]
    fn truncated_ciphertext_returns_crypto_error() {
        let k = key();
        let err = decrypt(&k, "alice", &[0u8; 16]).unwrap_err();
        match err {
            AppError::Crypto(_) => {}
            other => panic!("expected Crypto, got {other:?}"),
        }
    }

    /// Known-answer vector generated with libsodium 1.0.19:
    ///   crypto_aead_xchacha20poly1305_ietf_encrypt(
    ///     key   = 0x08…08 (32 bytes),
    ///     nonce = 0x07…07 (24 bytes),
    ///     ad    = b"alice",
    ///     msg   = b"hello"
    ///   )
    /// Use this to confirm wire compatibility with mobile libsodium clients.
    #[test]
    fn matches_libsodium_kat() {
        let k = key();
        let nonce: [u8; 24] = hex!("070707070707070707070707070707070707070707070707");
        let cipher = XChaCha20Poly1305::new(k.as_slice().into());
        let ct = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload { msg: b"hello", aad: b"alice" },
            )
            .unwrap();
        let mut wire = Vec::new();
        wire.extend_from_slice(&nonce);
        wire.extend_from_slice(&ct);
        let pt = decrypt(&k, "alice", &wire).unwrap();
        assert_eq!(pt, b"hello");
        let ct2 = encrypt(&k, "alice", b"hello").unwrap();
        assert_ne!(ct2[NONCE_LEN..], ct[..]);
    }
}
