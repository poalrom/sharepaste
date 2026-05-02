use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct ClaimInviteReq<'a> { pub token: &'a str, pub device_label: &'a str }

#[derive(Deserialize)]
pub struct ClaimInviteResp { pub device_token: String, pub user_id: String, pub device_id: String }

#[derive(Serialize)]
pub struct PairStartReq<'a> { pub secret_hash: &'a str }

#[derive(Deserialize)]
pub struct PairStartResp { pub pair_id: String }

#[derive(Serialize)]
pub struct PairClaimReq<'a> { pub pair_id: &'a str, pub secret_proof: &'a str }

#[derive(Serialize)]
pub struct PairPayloadReq<'a> { pub pair_id: &'a str, pub encrypted_payload: &'a str }

#[derive(Deserialize)]
pub struct PairPayloadResp { pub encrypted_payload: String }

#[derive(Deserialize)]
pub struct PairPollResp { pub status: String }

#[derive(Serialize)]
pub struct DevicesReq<'a> { pub pair_id: &'a str, pub secret_proof: &'a str, pub label: &'a str }

#[derive(Deserialize)]
pub struct DevicesResp { pub device_token: String, pub device_id: String, pub user_id: String }

#[derive(Serialize)]
pub struct PostEntryReq<'a> { pub ciphertext: &'a str }

#[derive(Deserialize)]
pub struct PostEntryResp { pub id: i64, pub created_at: i64 }

#[derive(Deserialize, Debug, Clone)]
pub struct EntryRow {
    pub id: i64,
    pub ciphertext: String,
    pub created_at: i64,
    pub device_id: String,
}
