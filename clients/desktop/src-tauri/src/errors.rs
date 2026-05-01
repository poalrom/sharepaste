use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("network error: {0}")]
    Network(String),
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("bad input: {0}")]
    BadInput(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("crypto error: {0}")]
    Crypto(String),
    #[error("pair expired: {0}")]
    PairExpired(String),
    #[error("keychain error: {0}")]
    Keychain(String),
}

impl AppError {
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Network(_) => "Network",
            AppError::Auth(_) => "Auth",
            AppError::NotFound(_) => "NotFound",
            AppError::BadInput(_) => "BadInput",
            AppError::Storage(_) => "Storage",
            AppError::Crypto(_) => "Crypto",
            AppError::PairExpired(_) => "PairExpired",
            AppError::Keychain(_) => "Keychain",
        }
    }
}

#[derive(Serialize)]
struct WireError<'a> {
    kind: &'a str,
    message: String,
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WireError { kind: self.kind(), message: self.to_string() }.serialize(s)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self { AppError::Storage(e.to_string()) }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_status() && e.status() == Some(reqwest::StatusCode::UNAUTHORIZED) {
            AppError::Auth(e.to_string())
        } else {
            AppError::Network(e.to_string())
        }
    }
}

impl From<keyring::Error> for AppError {
    fn from(e: keyring::Error) -> Self { AppError::Keychain(e.to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_to_kind_and_message_object() {
        let e = AppError::BadInput("missing token".into());
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(json, r#"{"kind":"BadInput","message":"bad input: missing token"}"#);
    }
}
