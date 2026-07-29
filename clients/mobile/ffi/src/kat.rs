//! The crypto known-answer vector, reachable from a foreign test runner.
//!
//! `clients/core/src/crypto.rs` annotates this vector as the check for wire
//! compatibility with mobile libsodium clients. That check is worth very little
//! if it only ever runs on the developer's laptop against the developer's
//! architecture: the failure it exists to catch is a *silent protocol
//! divergence*, and the only place a divergence could hide is the build nobody
//! runs the Rust test suite on — the cross-compiled Android one.
//!
//! So the vector crosses the boundary and the assertions are made on the other
//! side, on the device. Round-tripping encrypt into decrypt would not do: that
//! passes under any self-consistent cipher, including a wrong one.
//!
//! Behind the `testing` feature, which the Android build turns on for its
//! `debug` variant and off for `release`: the emulator proves this on the same
//! sources the shipped library is built from, and the shipped APK exports none
//! of it.

use crate::error::AppError;
use sharepaste_core::crypto;
use zeroize::Zeroizing;

/// The vector's key: 32 bytes of `0x08`.
pub const KAT_KEY: [u8; 32] = [0x08; 32];

/// The vector's nonce: 24 bytes of `0x07`. Fixed, which is what makes the
/// answer knowable — [`crypto::encrypt`] draws a random one.
pub const KAT_NONCE: [u8; 24] = [0x07; 24];

/// The associated data. In the protocol this slot carries the user id, which is
/// what binds a ciphertext to the User it was written for.
pub const KAT_AAD: &str = "alice";

pub const KAT_PLAINTEXT: &[u8] = b"hello";

/// `crypto_aead_xchacha20poly1305_ietf_encrypt(KAT_KEY, KAT_NONCE, KAT_AAD,
/// KAT_PLAINTEXT)` — 5 bytes of ciphertext followed by the 16-byte Poly1305
/// tag.
///
/// Reproduced independently of the Rust crate that decrypts it: HChaCha20
/// derived from OpenSSL's raw ChaCha20 block function, then OpenSSL's
/// ChaCha20-Poly1305-IETF under the resulting subkey, produce these same 21
/// bytes. It is the algorithm's answer, not one implementation's.
pub const KAT_CIPHERTEXT: [u8; 21] = [
    0x60, 0xae, 0xe5, 0xa6, 0x09, 0x7a, 0x05, 0x67, 0xad, 0x89, 0x16, 0x2b, 0xec, 0xb8, 0x58,
    0x58, 0x98, 0x93, 0x2f, 0xaa, 0xab,
];

fn kat_key() -> crypto::UserKey {
    Zeroizing::new(KAT_KEY)
}

/// The vector on the wire, in the layout the protocol uses: nonce, then the
/// AEAD output.
#[uniffi::export]
pub fn crypto_kat_wire() -> Vec<u8> {
    let mut wire = Vec::with_capacity(KAT_NONCE.len() + KAT_CIPHERTEXT.len());
    wire.extend_from_slice(&KAT_NONCE);
    wire.extend_from_slice(&KAT_CIPHERTEXT);
    wire
}

/// The plaintext the vector must decrypt to, so the caller asserts against
/// bytes rather than against a string literal it also typed.
#[uniffi::export]
pub fn crypto_kat_plaintext() -> Vec<u8> {
    KAT_PLAINTEXT.to_vec()
}

/// [`crypto::decrypt`] under the vector's key and associated data.
///
/// This is the call that matters. Hand it [`crypto_kat_wire`] and a matching
/// implementation returns [`crypto_kat_plaintext`]; a divergent one fails tag
/// verification and returns [`AppError::Crypto`].
#[uniffi::export]
pub fn crypto_kat_decrypt(wire: Vec<u8>) -> Result<Vec<u8>, AppError> {
    Ok(crypto::decrypt(&kat_key(), KAT_AAD, &wire)?)
}

/// [`crypto::encrypt`] under the vector's key and associated data.
///
/// The nonce is random, so the answer is not known — what a caller can assert
/// is that the output differs from the pinned vector every time and still
/// decrypts, which is the same pair of facts the Rust test asserts.
#[uniffi::export]
pub fn crypto_kat_encrypt(plaintext: Vec<u8>) -> Result<Vec<u8>, AppError> {
    Ok(crypto::encrypt(&kat_key(), KAT_AAD, &plaintext)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chacha20poly1305::aead::{Aead, KeyInit, Payload};
    use chacha20poly1305::{XChaCha20Poly1305, XNonce};

    /// The pinned constant is regenerated rather than trusted. If a dependency
    /// bump ever changed what XChaCha20-Poly1305 means, this fails on the host
    /// before the emulator ever sees it.
    #[test]
    fn pinned_vector_is_what_xchacha20poly1305_produces() {
        let cipher = XChaCha20Poly1305::new(KAT_KEY.as_slice().into());
        let ct = cipher
            .encrypt(
                XNonce::from_slice(&KAT_NONCE),
                Payload { msg: KAT_PLAINTEXT, aad: KAT_AAD.as_bytes() },
            )
            .expect("encrypt the known-answer vector");
        assert_eq!(ct, KAT_CIPHERTEXT);
    }

    #[test]
    fn the_core_decrypts_the_pinned_vector() {
        assert_eq!(crypto_kat_decrypt(crypto_kat_wire()).unwrap(), KAT_PLAINTEXT);
    }

    #[test]
    fn a_flipped_bit_fails_the_tag() {
        let mut wire = crypto_kat_wire();
        let last = wire.len() - 1;
        wire[last] ^= 0x01;
        assert!(matches!(crypto_kat_decrypt(wire), Err(AppError::Crypto { .. })));
    }
}
