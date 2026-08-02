use crate::errors::{is_cleartext, AppError};
use crate::http::dto::*;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::{Client, StatusCode};

/// Whether a relay may be reached over cleartext, decided by the shell.
///
/// The core holds no opinion of its own about schemes — see the comment on
/// [`ServerClient::new`] — so the opinion arrives here as data, once per
/// process, from whichever application is above it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportPolicy {
    /// An `http://` base is refused before a single byte leaves. Android.
    RequireHttps,
    /// Any scheme reqwest can speak is accepted. The desktop.
    AllowCleartext,
}

#[derive(Clone, Debug)]
pub struct ServerClient {
    base: String,
    http: Client,
    token: Option<String>,
}

impl ServerClient {
    /// Build a client for `base`, or refuse it.
    ///
    /// This is the one choke point the transport policy is enforced at, and it
    /// is here rather than on the pairing entry points on purpose: every request
    /// this crate makes — the invite claim, the short-code claim, and every
    /// later session request, which comes through
    /// [`PairingRegistry::load_active_membership`](crate::pairing::registry::PairingRegistry::load_active_membership)
    /// — has to pass through a constructor. A check bolted onto
    /// `pair_with_code` would cover one of those and be forgotten by the next
    /// entry point somebody adds.
    ///
    /// The refusal is a construction failure rather than a per-request one so
    /// that a shell requiring HTTPS cannot end up holding a client that will
    /// fail later, opaquely, at the transport layer.
    pub fn new(base: impl Into<String>, policy: TransportPolicy) -> Result<Self, AppError> {
        let base = base.into();
        if policy == TransportPolicy::RequireHttps && is_cleartext(&base) {
            return Err(AppError::InsecureRelay(format!(
                "{base} is a plain-HTTP relay, and this app only talks to relays over HTTPS. \
                 Over cleartext, anything on the network path between here and the relay can \
                 read this device's access token and replay it. The relay needs to be served \
                 over HTTPS with a publicly trusted certificate; then pair against its \
                 https:// address."
            )));
        }
        let http = Client::builder()
            // Names the crate that actually speaks the protocol, not one of its
            // shells: this client runs on Android too, where "desktop" is a lie.
            .user_agent(concat!("sharepaste-core/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| AppError::Network(e.to_string()))?;
        Ok(Self { base, http, token: None })
    }

    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    fn auth_headers(&self) -> Result<HeaderMap, AppError> {
        let mut h = HeaderMap::new();
        if let Some(t) = &self.token {
            let v = HeaderValue::from_str(&format!("Bearer {t}"))
                .map_err(|e| AppError::Auth(e.to_string()))?;
            h.insert(AUTHORIZATION, v);
        }
        Ok(h)
    }

    fn url(&self, path: &str) -> String { format!("{}{}", self.base.trim_end_matches('/'), path) }

    fn map_status(status: StatusCode, body: String) -> AppError {
        match status.as_u16() {
            401 => AppError::Auth(body),
            403 => AppError::Auth(body),
            404 => AppError::NotFound(body),
            410 => AppError::PairExpired(body),
            413 => AppError::BadInput(format!("payload too large: {body}")),
            400..=499 => AppError::BadInput(body),
            _ => AppError::Network(format!("status {status}: {body}")),
        }
    }

    async fn json_post<TReq: serde::Serialize, TResp: for<'de> serde::Deserialize<'de>>(
        &self,
        path: &str,
        body: &TReq,
        authed: bool,
    ) -> Result<TResp, AppError> {
        let mut req = self.http.post(self.url(path)).json(body);
        if authed { req = req.headers(self.auth_headers()?); }
        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Self::map_status(status, body));
        }
        resp.json::<TResp>().await.map_err(|e| AppError::Network(e.to_string()))
    }

    pub async fn claim_invite(&self, token: &str, label: &str) -> Result<ClaimInviteResp, AppError> {
        self.json_post("/claim-invite", &ClaimInviteReq { token, device_label: label }, false).await
    }

    pub(crate) async fn pair_start(&self, secret_hash: &str) -> Result<PairStartResp, AppError> {
        self.json_post("/pair/start", &PairStartReq { secret_hash }, true).await
    }

    pub async fn pair_claim(&self, pair_id: &str, secret_proof: &str) -> Result<(), AppError> {
        let resp = self.http.post(self.url("/pair/claim"))
            .json(&PairClaimReq { pair_id, secret_proof })
            .send().await?;
        let status = resp.status();
        if status.is_success() { return Ok(()); }
        let body = resp.text().await.unwrap_or_default();
        Err(Self::map_status(status, body))
    }

    pub(crate) async fn pair_payload_put(&self, pair_id: &str, encrypted_payload: &str) -> Result<(), AppError> {
        let _: serde_json::Value = self
            .json_post("/pair/payload", &PairPayloadReq { pair_id, encrypted_payload }, true)
            .await?;
        Ok(())
    }

    pub(crate) async fn pair_payload_get(&self, pair_id: &str, secret_proof: &str) -> Result<PairPayloadResp, AppError> {
        let resp = self.http.get(self.url("/pair/payload"))
            .query(&[("id", pair_id), ("proof", secret_proof)])
            .send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Self::map_status(status, body));
        }
        resp.json().await.map_err(|e| AppError::Network(e.to_string()))
    }

    pub async fn pair_poll(&self, pair_id: &str, timeout_ms: u32) -> Result<PairPollResp, AppError> {
        let resp = self.http.get(self.url("/pair/poll"))
            .query(&[("id", pair_id), ("timeout_ms", &timeout_ms.to_string())])
            .headers(self.auth_headers()?)
            .send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Self::map_status(status, body));
        }
        resp.json().await.map_err(|e| AppError::Network(e.to_string()))
    }

    pub async fn devices(&self, pair_id: &str, secret_proof: &str, label: &str) -> Result<DevicesResp, AppError> {
        self.json_post("/devices", &DevicesReq { pair_id, secret_proof, label }, false).await
    }

    /// The caller's user and every device paired to it, including revoked ones.
    ///
    /// The only route carrying Device Labels and the username; both are
    /// mirrored locally rather than travelling on each entry.
    pub async fn me(&self) -> Result<MeResp, AppError> {
        let resp = self.http.get(self.url("/me"))
            .headers(self.auth_headers()?)
            .send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Self::map_status(status, body));
        }
        resp.json().await.map_err(|e| AppError::Network(e.to_string()))
    }

    pub async fn post_entry(&self, ciphertext_b64: &str) -> Result<PostEntryResp, AppError> {
        self.json_post("/entries", &PostEntryReq { ciphertext: ciphertext_b64 }, true).await
    }

    /// Everything past `since_seq`, in sequence order.
    ///
    /// A sequence and not an entry id: an entry moves in the order when it is
    /// used, and the relay re-allocates its sequence so it rises above every
    /// device's watermark and comes back down this one pipe.
    pub async fn list_entries(&self, since_seq: i64, limit: u32) -> Result<Vec<EntryRow>, AppError> {
        let resp = self.http.get(self.url("/entries"))
            .query(&[("since", &since_seq.to_string()), ("limit", &limit.to_string())])
            .headers(self.auth_headers()?)
            .send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Self::map_status(status, body));
        }
        resp.json().await.map_err(|e| AppError::Network(e.to_string()))
    }

    /// Record a **Use** of one entry: it becomes the head of the History on
    /// every device.
    ///
    /// A relay older than this client has no such route and answers 404, which
    /// is indistinguishable here from an entry that is gone. Both are handled
    /// the same way by the caller and deliberately so: skew is not handled
    /// (the relay is updated first), and a failed use never fails the Recall
    /// that provoked it.
    pub async fn use_entry(&self, id: i64) -> Result<UseEntryResp, AppError> {
        let resp = self.http.post(self.url(&format!("/entries/{id}/use")))
            .headers(self.auth_headers()?)
            .send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Self::map_status(status, body));
        }
        resp.json().await.map_err(|e| AppError::Network(e.to_string()))
    }

    pub async fn delete_entry(&self, id: i64) -> Result<(), AppError> {
        let resp = self.http.delete(self.url(&format!("/entries/{id}")))
            .headers(self.auth_headers()?)
            .send().await?;
        let status = resp.status();
        if status.is_success() { return Ok(()); }
        let body = resp.text().await.unwrap_or_default();
        Err(Self::map_status(status, body))
    }

    pub async fn delete_all_entries(&self) -> Result<(), AppError> {
        let resp = self.http.delete(self.url("/entries"))
            .headers(self.auth_headers()?)
            .send().await?;
        let status = resp.status();
        if status.is_success() { return Ok(()); }
        let body = resp.text().await.unwrap_or_default();
        Err(Self::map_status(status, body))
    }

    pub fn base(&self) -> &str { &self.base }
    pub(crate) fn token(&self) -> Option<&str> { self.token.as_deref() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_status_known_codes() {
        assert!(matches!(ServerClient::map_status(StatusCode::UNAUTHORIZED, "x".into()), AppError::Auth(_)));
        assert!(matches!(ServerClient::map_status(StatusCode::FORBIDDEN, "x".into()), AppError::Auth(_)));
        assert!(matches!(ServerClient::map_status(StatusCode::NOT_FOUND, "x".into()), AppError::NotFound(_)));
        assert!(matches!(ServerClient::map_status(StatusCode::GONE, "x".into()), AppError::PairExpired(_)));
        assert!(matches!(ServerClient::map_status(StatusCode::PAYLOAD_TOO_LARGE, "x".into()), AppError::BadInput(_)));
        assert!(matches!(ServerClient::map_status(StatusCode::BAD_REQUEST, "x".into()), AppError::BadInput(_)));
        assert!(matches!(ServerClient::map_status(StatusCode::INTERNAL_SERVER_ERROR, "x".into()), AppError::Network(_)));
    }

    #[test]
    fn url_strips_trailing_slash() {
        let c = ServerClient::new("https://srv/", TransportPolicy::AllowCleartext).unwrap();
        assert_eq!(c.url("/x"), "https://srv/x");
    }

    #[test]
    fn auth_header_present_when_token_set() {
        let c = ServerClient::new("https://srv", TransportPolicy::AllowCleartext)
            .unwrap()
            .with_token("abc");
        let h = c.auth_headers().unwrap();
        assert_eq!(h.get(AUTHORIZATION).unwrap(), "Bearer abc");
    }

    #[test]
    fn auth_header_absent_otherwise() {
        let c = ServerClient::new("https://srv", TransportPolicy::AllowCleartext).unwrap();
        assert!(c.auth_headers().unwrap().get(AUTHORIZATION).is_none());
    }

    /// The mobile shells' policy: cleartext is refused, and the refusal says
    /// why rather than surfacing later as a transport error.
    #[test]
    fn require_https_refuses_a_cleartext_relay_and_names_the_reason() {
        let err = ServerClient::new("http://relay.example", TransportPolicy::RequireHttps)
            .expect_err("an http:// base must not produce a client under RequireHttps");
        let AppError::InsecureRelay(detail) = err else {
            panic!("expected InsecureRelay, got {err:?}");
        };
        assert!(detail.contains("http://relay.example"), "names the address: {detail}");
        assert!(detail.contains("HTTPS"), "names what is required: {detail}");
        assert!(detail.contains("token"), "names why it matters: {detail}");
    }

    #[test]
    fn require_https_accepts_a_tls_relay() {
        assert!(ServerClient::new("https://relay.example", TransportPolicy::RequireHttps).is_ok());
    }

    /// A hand-typed address reaches here padded and in whatever case somebody
    /// used, and neither is a way around the policy.
    #[test]
    fn require_https_is_not_evaded_by_case_or_padding() {
        for base in ["  http://relay.example", "HTTP://relay.example", "HtTp://relay.example"] {
            assert!(
                matches!(
                    ServerClient::new(base, TransportPolicy::RequireHttps),
                    Err(AppError::InsecureRelay(_))
                ),
                "{base} slipped through",
            );
        }
    }

    /// The desktop's policy, unchanged by any of the above: it keeps working for
    /// anyone already paired to a cleartext relay.
    #[test]
    fn allow_cleartext_accepts_both_schemes() {
        assert!(ServerClient::new("http://relay.example", TransportPolicy::AllowCleartext).is_ok());
        assert!(ServerClient::new("https://relay.example", TransportPolicy::AllowCleartext).is_ok());
    }
}
