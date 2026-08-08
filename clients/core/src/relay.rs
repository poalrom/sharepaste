//! One Relay, behind one interface.
//!
//! Every route this crate calls is on [`Relay`], and every caller reaches one
//! through a [`RelayDial`] the shell handed in. That is the whole of the seam:
//! nothing above it builds an HTTP client, and nothing below the facade takes a
//! transport as an argument.
//!
//! The split between the two exists because a Relay is bound to one address and
//! one device token, and this core talks to relays it has never seen. An invite
//! claim and a short-code claim each name their own address and carry no token;
//! a session's every request goes to the address its Pairing stored, as the
//! device that Pairing named. A dial turns "which relay, as whom" into one, and
//! [`Relay`] is what is left once that question is answered.
//!
//! There are two adapters. [`ServerClient`] speaks HTTP and is what ships;
//! `ScriptedRelay` answers from a script and is what every test below the
//! facade runs against, with no relay in reach.

use crate::errors::AppError;
use crate::http::dto::{ClaimInviteResp, DevicesResp, EntryRow, MeResp};
use crate::http::{ServerClient, TransportPolicy};
use crate::pairing::payload::PairClaim;
use crate::sync::sse;
use async_trait::async_trait;
use std::sync::atomic::AtomicI64;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// What the relay recorded for an Entry this device just uploaded.
///
/// The id alone used to be enough, and it was thrown away. It is not enough
/// now: the uploader caches the Entry itself rather than waiting for the relay
/// to echo it back, and a cached Entry needs the relay's `created_at` — the
/// number every other device will see it ordered and dated by — its `seq` and
/// its `last_use`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Uploaded {
    pub id: i64,
    pub created_at: i64,
    pub seq: i64,
    pub last_use: i64,
}

/// What the relay recorded for a **Use**.
///
/// `seq` is carried because the route answers it and this type mirrors the
/// wire, and it is deliberately never applied: a sequence is a *watermark*
/// value, and the watermark means "everything up to here has been fetched".
/// This device fetched nothing — it wrote — and the relay may hold entries from
/// other devices below this sequence that it has never seen. The same rule
/// [`Uploader::flush_once`](crate::sync::uploader::Uploader::flush_once) states
/// at length, for the same reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Used {
    pub seq: i64,
    pub last_use: i64,
}

/// Every route the core calls on one Relay, as the caller that already knows
/// which relay and which device.
///
/// One trait and not four, because there is one relay: a session, an uploader,
/// an inviter and a device claiming a code are all talking to the same service
/// over the same connection, and describing it four times is what made every
/// operation that touches it need a public twin taking the transport as an
/// argument.
///
/// Nothing here is a wire detail a caller has to know. The relay's status
/// strings become [`PairClaim`] here, in the implementation, so no poll loop
/// ever matches on wire text; an unrecognised status reads as
/// [`PairClaim::Waiting`], because a relay that grows a new intermediate state
/// must not look like an expiry to an older client.
#[async_trait]
pub trait Relay: Send + Sync {
    /// Where this relay lives.
    ///
    /// It travels inside a short code, because that address is the only thing
    /// the claiming device is given, and it is what a failure names when it has
    /// to explain that the relay is served over cleartext.
    fn base_url(&self) -> String;

    // -- entries -----------------------------------------------------------

    /// Everything past `since_seq`, in sequence order.
    ///
    /// A sequence and not an entry id: an entry moves in the order when it is
    /// used, and the relay re-allocates its sequence so it rises above every
    /// device's watermark and comes back down this one pipe.
    async fn list_entries(&self, since_seq: i64, limit: u32) -> Result<Vec<EntryRow>, AppError>;

    /// This device's User and every device paired to it, including revoked
    /// ones.
    ///
    /// The only route carrying Device Labels and the username; both are
    /// mirrored locally rather than travelling on each entry. A relay older
    /// than this client does not serve it at all — see
    /// [ADR 0001](../../../../docs/adr/0001-device-metadata-out-of-band.md).
    async fn me(&self) -> Result<MeResp, AppError>;

    /// Stream server events until cancelled or the connection drops.
    ///
    /// `contact` is stamped from *inside* the implementation, below whatever
    /// parses the frames — see [`sse::run`]. A `: heartbeat` comment dispatches
    /// no event, so only the byte stream tells a healthy idle connection from a
    /// dead one.
    async fn stream(
        &self,
        sink: mpsc::Sender<sse::ServerEvent>,
        cancel: CancellationToken,
        contact: Arc<AtomicI64>,
    ) -> Result<(), AppError>;

    /// Place one Entry's ciphertext on the relay.
    async fn upload(&self, ciphertext_b64: &str) -> Result<Uploaded, AppError>;

    /// Record a **Use**: the entry becomes the head of the History everywhere.
    ///
    /// [`AppError::NotFound`] covers both an entry that is gone and a relay too
    /// old to have the route. Neither is worth distinguishing: skew is not
    /// handled here (the relay is updated first), and a use nobody can record
    /// is a use with nothing to reorder.
    async fn use_entry(&self, entry_id: i64) -> Result<Used, AppError>;

    /// Take one entry off the relay — a delete, or an act withdrawn while it
    /// was being uploaded.
    async fn delete_entry(&self, entry_id: i64) -> Result<(), AppError>;

    /// Take every entry this User has off the relay.
    async fn delete_all_entries(&self) -> Result<(), AppError>;

    // -- pairing -----------------------------------------------------------

    /// Redeem an invite and become one of this relay's devices — the only path
    /// that reaches a relay this device has never been paired to.
    async fn claim_invite(
        &self,
        token: &str,
        device_label: &str,
    ) -> Result<ClaimInviteResp, AppError>;

    /// `POST /pair/start`, returning the pair id the relay minted.
    async fn pair_start(&self, secret_hash: &str) -> Result<String, AppError>;

    /// `PUT /pair/payload` — the payload encrypted to the pairing secret.
    async fn pair_payload_put(
        &self,
        pair_id: &str,
        encrypted_payload: &str,
    ) -> Result<(), AppError>;

    /// `GET /pair/poll` — long-poll until the claimer takes the code.
    async fn pair_poll(&self, pair_id: &str, timeout_ms: u32) -> Result<PairClaim, AppError>;

    /// `POST /pair/claim` — the claimer proving it holds the secret.
    async fn pair_claim(&self, pair_id: &str, secret_proof: &str) -> Result<(), AppError>;

    /// `GET /pair/payload` — the ciphertext the inviter uploaded, base64.
    async fn pair_payload(&self, pair_id: &str, secret_proof: &str) -> Result<String, AppError>;

    /// `POST /devices` — the claimer becoming a device of the invited User.
    async fn pair_devices(
        &self,
        pair_id: &str,
        secret_proof: &str,
        device_label: &str,
    ) -> Result<DevicesResp, AppError>;
}

/// How this core reaches a Relay at an address, as a device or as nobody.
///
/// The one thing a shell states about the network, handed in on
/// [`SharepasteConfig::relay`](crate::facade::SharepasteConfig::relay) and
/// shared with the pairing registry, which is what turns a stored Pairing into
/// a live [`Relay`]. Every request this crate makes passes through a dial, so
/// a policy applied here — [`RelayDial::over_http`] refusing a cleartext
/// address — cannot be forgotten by the next entry point somebody adds.
#[derive(Clone)]
pub struct RelayDial(
    #[allow(clippy::type_complexity)]
    Arc<dyn Fn(&str, Option<&str>) -> Result<Arc<dyn Relay>, AppError> + Send + Sync>,
);

impl RelayDial {
    pub fn new(
        dial: impl Fn(&str, Option<&str>) -> Result<Arc<dyn Relay>, AppError> + Send + Sync + 'static,
    ) -> Self {
        RelayDial(Arc::new(dial))
    }

    /// The shipped dial: a real HTTP client, under the shell's scheme policy.
    ///
    /// The policy travels with the dial rather than beside it because a Pairing
    /// stored while the shell permitted cleartext, resumed by a shell that does
    /// not, has to fail with [`AppError::InsecureRelay`] here rather than limp
    /// on over plain HTTP — and it can only do that if there is one place a
    /// client is ever built.
    pub fn over_http(policy: TransportPolicy) -> Self {
        RelayDial::new(move |base_url, token| {
            let client = ServerClient::new(base_url, policy)?;
            let client = match token {
                Some(token) => client.with_token(token),
                None => client,
            };
            Ok(Arc::new(client) as Arc<dyn Relay>)
        })
    }

    /// Reach the relay at `base_url`, as the device `token` names when there is
    /// one.
    ///
    /// `None` is not an anonymous convenience: the invite claim and both halves
    /// of a short-code claim happen before this device has a token at all.
    pub fn at(&self, base_url: &str, token: Option<&str>) -> Result<Arc<dyn Relay>, AppError> {
        (self.0)(base_url, token)
    }
}

#[async_trait]
impl Relay for ServerClient {
    fn base_url(&self) -> String {
        self.base().to_string()
    }

    async fn list_entries(&self, since_seq: i64, limit: u32) -> Result<Vec<EntryRow>, AppError> {
        ServerClient::list_entries(self, since_seq, limit).await
    }

    async fn me(&self) -> Result<MeResp, AppError> {
        ServerClient::me(self).await
    }

    async fn stream(
        &self,
        sink: mpsc::Sender<sse::ServerEvent>,
        cancel: CancellationToken,
        contact: Arc<AtomicI64>,
    ) -> Result<(), AppError> {
        sse::run(self.clone(), sink, cancel, contact).await
    }

    async fn upload(&self, ciphertext_b64: &str) -> Result<Uploaded, AppError> {
        self.post_entry(ciphertext_b64).await.map(|r| Uploaded {
            id: r.id,
            created_at: r.created_at,
            seq: r.seq,
            last_use: r.last_use,
        })
    }

    async fn use_entry(&self, entry_id: i64) -> Result<Used, AppError> {
        ServerClient::use_entry(self, entry_id)
            .await
            .map(|r| Used { seq: r.seq, last_use: r.last_use })
    }

    async fn delete_entry(&self, entry_id: i64) -> Result<(), AppError> {
        ServerClient::delete_entry(self, entry_id).await
    }

    async fn delete_all_entries(&self) -> Result<(), AppError> {
        ServerClient::delete_all_entries(self).await
    }

    async fn claim_invite(
        &self,
        token: &str,
        device_label: &str,
    ) -> Result<ClaimInviteResp, AppError> {
        ServerClient::claim_invite(self, token, device_label).await
    }

    async fn pair_start(&self, secret_hash: &str) -> Result<String, AppError> {
        Ok(ServerClient::pair_start(self, secret_hash).await?.pair_id)
    }

    async fn pair_payload_put(
        &self,
        pair_id: &str,
        encrypted_payload: &str,
    ) -> Result<(), AppError> {
        ServerClient::pair_payload_put(self, pair_id, encrypted_payload).await
    }

    async fn pair_poll(&self, pair_id: &str, timeout_ms: u32) -> Result<PairClaim, AppError> {
        let resp = ServerClient::pair_poll(self, pair_id, timeout_ms).await?;
        Ok(match resp.status.as_str() {
            "consumed" => PairClaim::Consumed { device_label: resp.device_label },
            "expired" => PairClaim::Expired,
            _ => PairClaim::Waiting,
        })
    }

    async fn pair_claim(&self, pair_id: &str, secret_proof: &str) -> Result<(), AppError> {
        ServerClient::pair_claim(self, pair_id, secret_proof).await
    }

    async fn pair_payload(&self, pair_id: &str, secret_proof: &str) -> Result<String, AppError> {
        Ok(ServerClient::pair_payload_get(self, pair_id, secret_proof)
            .await?
            .encrypted_payload)
    }

    async fn pair_devices(
        &self,
        pair_id: &str,
        secret_proof: &str,
        device_label: &str,
    ) -> Result<DevicesResp, AppError> {
        self.devices(pair_id, secret_proof, device_label).await
    }
}
