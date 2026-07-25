use crate::errors::AppError;
use crate::core::http::dto::*;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::{Client, StatusCode};

#[derive(Clone, Debug)]
pub struct ServerClient {
    base: String,
    http: Client,
    token: Option<String>,
}

impl ServerClient {
    pub fn new(base: impl Into<String>) -> Result<Self, AppError> {
        let http = Client::builder()
            .user_agent(concat!("sharepaste-desktop/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| AppError::Network(e.to_string()))?;
        Ok(Self { base: base.into(), http, token: None })
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

    pub(crate) async fn pair_poll(&self, pair_id: &str, timeout_ms: u32) -> Result<PairPollResp, AppError> {
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

    pub async fn post_entry(&self, ciphertext_b64: &str) -> Result<PostEntryResp, AppError> {
        self.json_post("/entries", &PostEntryReq { ciphertext: ciphertext_b64 }, true).await
    }

    pub async fn list_entries(&self, since: i64, limit: u32) -> Result<Vec<EntryRow>, AppError> {
        let resp = self.http.get(self.url("/entries"))
            .query(&[("since", &since.to_string()), ("limit", &limit.to_string())])
            .headers(self.auth_headers()?)
            .send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Self::map_status(status, body));
        }
        resp.json().await.map_err(|e| AppError::Network(e.to_string()))
    }

    pub(crate) async fn delete_entry(&self, id: i64) -> Result<(), AppError> {
        let resp = self.http.delete(self.url(&format!("/entries/{id}")))
            .headers(self.auth_headers()?)
            .send().await?;
        let status = resp.status();
        if status.is_success() { return Ok(()); }
        let body = resp.text().await.unwrap_or_default();
        Err(Self::map_status(status, body))
    }

    pub(crate) async fn delete_all_entries(&self) -> Result<(), AppError> {
        let resp = self.http.delete(self.url("/entries"))
            .headers(self.auth_headers()?)
            .send().await?;
        let status = resp.status();
        if status.is_success() { return Ok(()); }
        let body = resp.text().await.unwrap_or_default();
        Err(Self::map_status(status, body))
    }

    pub(crate) fn base(&self) -> &str { &self.base }
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
        let c = ServerClient::new("https://srv/").unwrap();
        assert_eq!(c.url("/x"), "https://srv/x");
    }

    #[test]
    fn auth_header_present_when_token_set() {
        let c = ServerClient::new("https://srv").unwrap().with_token("abc");
        let h = c.auth_headers().unwrap();
        assert_eq!(h.get(AUTHORIZATION).unwrap(), "Bearer abc");
    }

    #[test]
    fn auth_header_absent_otherwise() {
        let c = ServerClient::new("https://srv").unwrap();
        assert!(c.auth_headers().unwrap().get(AUTHORIZATION).is_none());
    }
}
