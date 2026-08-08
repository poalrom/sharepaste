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
    #[error("update error: {0}")]
    Update(String),
    /// A request to a relay reached over plain `http://` failed at the transport
    /// layer. See [`AppError::explain_insecure_relay`].
    #[error("insecure relay: {0}")]
    InsecureRelay(String),
}

impl AppError {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            AppError::Network(_) => "Network",
            AppError::Auth(_) => "Auth",
            AppError::NotFound(_) => "NotFound",
            AppError::BadInput(_) => "BadInput",
            AppError::Storage(_) => "Storage",
            AppError::Crypto(_) => "Crypto",
            AppError::PairExpired(_) => "PairExpired",
            AppError::Keychain(_) => "Keychain",
            AppError::Update(_) => "Update",
            AppError::InsecureRelay(_) => "InsecureRelay",
        }
    }

    /// Explain a transport failure against a cleartext relay.
    ///
    /// The core has no opinion of its own about schemes, and must not grow one:
    /// every desktop already paired to a cleartext relay would lose its pairing
    /// on the next upgrade, turning an unrelated client into collateral. The
    /// opinion belongs to the shell and arrives as the dial it hands over on
    /// [`SharepasteConfig::relay`](crate::facade::SharepasteConfig::relay),
    /// enforced once in
    /// [`ServerClient::new`](crate::http::ServerClient::new) — which is where a
    /// shell that requires HTTPS gets [`AppError::InsecureRelay`] from.
    ///
    /// This mapping is the *other* half, for a shell that permits cleartext and
    /// then cannot reach the relay anyway: the network, a proxy or a captive
    /// portal refused it, and what comes back names neither the scheme nor the
    /// reason. A desktop on a working cleartext relay never sees it, because its
    /// request succeeds.
    ///
    /// Only [`AppError::Network`] is remapped. An `Auth` or a `BadInput` from a
    /// cleartext relay means the connection worked and the scheme is beside the
    /// point.
    pub(crate) fn explain_insecure_relay(self, server_url: &str) -> Self {
        match self {
            AppError::Network(msg) if is_cleartext(server_url) => AppError::InsecureRelay(format!(
                "could not reach {server_url}: {msg}. That relay is served over plain HTTP, \
                 which a network in between — a proxy, a captive portal, a corporate \
                 firewall — is entitled to refuse; serving it over HTTPS with a publicly \
                 trusted certificate is what makes it reachable from anywhere."
            )),
            other => other,
        }
    }
}

/// Whether `url` names the `http` scheme, case-insensitively and without
/// allocating — this decides both a transport policy and an error message, so it
/// looks at the seven bytes that settle it and nothing else.
pub(crate) fn is_cleartext(url: &str) -> bool {
    let url = url.trim_start().as_bytes();
    url.len() >= 7 && url[..7].eq_ignore_ascii_case(b"http://")
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

// `AppError::Keychain` is unconditional — it is the vocabulary the Keychain trait's
// contract is written in. Only the conversion from the crate's own error is
// platform-bound, and it is gated to exactly the targets that declare `keyring`.
#[cfg(any(target_os = "macos", target_os = "windows"))]
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

    /*
     * The mapping is the whole feature: there is no scheme check anywhere on the
     * way in, so a cleartext relay that answers is a cleartext relay that works.
     */
    #[test]
    fn only_a_transport_failure_against_a_cleartext_relay_is_explained() {
        let mapped = AppError::Network("connection refused".into())
            .explain_insecure_relay("http://relay.example:8443");
        let AppError::InsecureRelay(msg) = mapped else {
            panic!("a Network failure against an http:// relay must be explained");
        };
        assert!(msg.contains("http://relay.example:8443"), "the message names the relay");
        assert!(msg.contains("HTTPS"), "and says what would fix it");

        // Same failure, TLS relay: nothing to explain.
        assert!(matches!(
            AppError::Network("connection refused".into())
                .explain_insecure_relay("https://relay.example"),
            AppError::Network(_)
        ));
        // Reached the relay and was turned away: the scheme is beside the point.
        assert!(matches!(
            AppError::Auth("401".into()).explain_insecure_relay("http://relay.example"),
            AppError::Auth(_)
        ));
        // The scheme match is case-insensitive and tolerates a padded url, both
        // of which reach `ServerClient::new` from a hand-typed relay address.
        assert!(matches!(
            AppError::Network("dns".into()).explain_insecure_relay("  HTTP://relay.example"),
            AppError::InsecureRelay(_)
        ));
        // `https` must not match on a prefix comparison.
        assert!(matches!(
            AppError::Network("dns".into()).explain_insecure_relay("HTTPS://relay.example"),
            AppError::Network(_)
        ));
    }
}
