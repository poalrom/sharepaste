use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub(crate) struct ClaimInviteReq<'a> { pub token: &'a str, pub device_label: &'a str }

#[derive(Deserialize)]
pub struct ClaimInviteResp { pub device_token: String, pub user_id: String, pub device_id: String }

#[derive(Serialize)]
pub(crate) struct PairStartReq<'a> { pub secret_hash: &'a str }

#[derive(Deserialize)]
pub(crate) struct PairStartResp { pub pair_id: String }

#[derive(Serialize)]
pub(crate) struct PairClaimReq<'a> { pub pair_id: &'a str, pub secret_proof: &'a str }

#[derive(Serialize)]
pub(crate) struct PairPayloadReq<'a> { pub pair_id: &'a str, pub encrypted_payload: &'a str }

#[derive(Deserialize)]
pub(crate) struct PairPayloadResp { pub encrypted_payload: String }

#[derive(Deserialize)]
pub struct PairPollResp { pub status: String, pub device_label: Option<String> }

#[derive(Serialize)]
pub(crate) struct DevicesReq<'a> { pub pair_id: &'a str, pub secret_proof: &'a str, pub label: &'a str }

#[derive(Deserialize)]
pub struct DevicesResp { pub device_token: String, pub device_id: String, pub user_id: String }

#[derive(Serialize)]
pub(crate) struct PostEntryReq<'a> { pub ciphertext: &'a str }

#[derive(Deserialize)]
pub struct PostEntryResp {
    pub id: i64,
    pub created_at: i64,
    pub seq: i64,
    pub last_use: i64,
}

/// What the relay recorded for a **Use**.
///
/// A fresh sequence, so the used entry rises above every device's watermark
/// and comes back down the one pipe there is, and the moment it stamped.
#[derive(Deserialize, Debug)]
pub struct UseEntryResp {
    pub seq: i64,
    pub last_use: i64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct EntryRow {
    pub id: i64,
    pub ciphertext: String,
    pub created_at: i64,
    pub device_id: String,
    /// This entry's place in the relay's per-user sequence, re-allocated on
    /// every use. What `GET /entries?since=` filters on, and what the sync
    /// watermark counts.
    pub seq: i64,
    pub last_use: i64,
}

#[derive(Deserialize)]
pub struct MeResp { pub user: UserDto, pub devices: Vec<DeviceDto> }

#[derive(Deserialize)]
pub struct UserDto { pub id: String, pub username: String }

#[derive(Deserialize)]
pub struct DeviceDto { pub device_id: String, pub label: Option<String>, pub created_at: i64, pub revoked_at: Option<i64> }

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the `GET /me` shape. The relay is the other half of this contract
    /// and compiles separately, so nothing but a test catches a drift here.
    #[test]
    fn me_response_parses_the_relay_shape_including_revoked_and_unlabelled() {
        let body = r#"{
            "user": { "id": "u1", "username": "alice" },
            "devices": [
              { "device_id": "d1", "label": "IPHONE-15", "created_at": 100, "revoked_at": null },
              { "device_id": "d2", "label": null, "created_at": 200, "revoked_at": 300 }
            ]
        }"#;
        let me: MeResp = serde_json::from_str(body).unwrap();
        assert_eq!(me.user.username, "alice");
        assert_eq!(me.devices[0].label.as_deref(), Some("IPHONE-15"));
        assert_eq!(me.devices[0].revoked_at, None);
        assert_eq!(me.devices[1].label, None);
        assert_eq!(me.devices[1].revoked_at, Some(300));
    }
}
