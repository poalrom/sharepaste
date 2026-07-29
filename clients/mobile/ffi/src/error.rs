//! The core's error, with its variants intact.
//!
//! A single stringly-typed error would be cheaper and would throw away the one
//! thing a phone needs most. [`AppError::InsecureRelay`] exists precisely so a
//! failure that would otherwise read as "network error" can be explained: the
//! shell declines cleartext, so a relay reached over `http://` cannot answer.
//! Flattening that into a string leaves the person guessing at a wifi problem.

use sharepaste_core::errors::AppError as CoreError;

/// Every way an operation can fail, one variant per core variant.
///
/// Kotlin sees this as `AppException` — UniFFI rewrites a trailing `Error` to
/// the target language's convention — with one nested class per variant, so a
/// `when` over the sealed class is exhaustive and the compiler says so.
///
/// The payload is called `detail` rather than `message` because a Kotlin
/// exception already has a `message`, and a variant field of that name
/// generates a class whose own property shadows it — the bindings do not
/// compile. `AppException.message` still reads `detail=…`.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum AppError {
    #[error("network error: {detail}")]
    Network { detail: String },
    #[error("authentication failed: {detail}")]
    Auth { detail: String },
    #[error("not found: {detail}")]
    NotFound { detail: String },
    #[error("bad input: {detail}")]
    BadInput { detail: String },
    #[error("storage error: {detail}")]
    Storage { detail: String },
    #[error("crypto error: {detail}")]
    Crypto { detail: String },
    #[error("pair expired: {detail}")]
    PairExpired { detail: String },
    #[error("keychain error: {detail}")]
    Keychain { detail: String },
    #[error("update error: {detail}")]
    Update { detail: String },
    /// A request to a relay reached over plain `http://` failed at the
    /// transport layer. The shell blocks cleartext; the relay needs HTTPS with
    /// a publicly trusted certificate. Never render this as a generic network
    /// failure — it is the one error whose text is the whole point.
    #[error("insecure relay: {detail}")]
    InsecureRelay { detail: String },
}

impl From<CoreError> for AppError {
    fn from(e: CoreError) -> Self {
        // Matched exhaustively rather than defaulted: a new core variant must
        // fail to compile here, not silently arrive on a phone as something
        // else. That is the entire reason this is not one string.
        match e {
            CoreError::Network(detail) => AppError::Network { detail },
            CoreError::Auth(detail) => AppError::Auth { detail },
            CoreError::NotFound(detail) => AppError::NotFound { detail },
            CoreError::BadInput(detail) => AppError::BadInput { detail },
            CoreError::Storage(detail) => AppError::Storage { detail },
            CoreError::Crypto(detail) => AppError::Crypto { detail },
            CoreError::PairExpired(detail) => AppError::PairExpired { detail },
            CoreError::Keychain(detail) => AppError::Keychain { detail },
            CoreError::Update(detail) => AppError::Update { detail },
            CoreError::InsecureRelay(detail) => AppError::InsecureRelay { detail },
        }
    }
}

impl From<AppError> for CoreError {
    /// The other direction, for the three platform traits: a Kotlin
    /// implementation throws an `AppException` and the core has to receive the
    /// error it would have produced itself.
    fn from(e: AppError) -> Self {
        match e {
            AppError::Network { detail } => CoreError::Network(detail),
            AppError::Auth { detail } => CoreError::Auth(detail),
            AppError::NotFound { detail } => CoreError::NotFound(detail),
            AppError::BadInput { detail } => CoreError::BadInput(detail),
            AppError::Storage { detail } => CoreError::Storage(detail),
            AppError::Crypto { detail } => CoreError::Crypto(detail),
            AppError::PairExpired { detail } => CoreError::PairExpired(detail),
            AppError::Keychain { detail } => CoreError::Keychain(detail),
            AppError::Update { detail } => CoreError::Update(detail),
            AppError::InsecureRelay { detail } => CoreError::InsecureRelay(detail),
        }
    }
}
