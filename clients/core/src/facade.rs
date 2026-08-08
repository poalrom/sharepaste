//! The one object a shell holds.
//!
//! `Sharepaste` owns the protocol's state — the database handle, the pairing
//! registry, the live sessions — and the runtime its background work runs on.
//! A shell hands it three implementations at construction and then only ever
//! calls methods on it; nothing below this line knows what kind of application
//! is above it.
//!
//! Every operation's arguments and return value are things a foreign binding can
//! carry: plain structs, enums, `String`, `i64`, `bool`, `Option`, `Vec`. The one
//! exception is [`Sharepaste::capture_watched`], which takes a `&dyn
//! PasteboardSniff` because Watched Capture has to inspect the pasteboard's types
//! before its text — and which no phone ever calls.

use crate::capture::filter::{self, CaptureContext, FilterDecision, PasteboardSniff, SkipReason};
use crate::crypto::UserKey;
use crate::errors::AppError;
use crate::event::{CoreEvent, Entry, Queued};
use crate::keychain::{token_account, user_key_account, Keychain};
use crate::pairing::invite::{claim_invite, persist_claimed_pairing};
use crate::pairing::payload::{
    fetch_and_decrypt_pair_payload, secret_proof_hex, start_pair, upload_pair_payload, PairClaim,
};
use crate::pairing::registry::PairingRegistry;
use crate::pairing::shortcode::{decode as decode_shortcode, group_for_display};
use crate::platform::{Clipboard, EventSink};
use crate::relay::{Relay, RelayDial};
use crate::render;
use crate::storage::history::Held;
use crate::storage::settings::Settings;
use crate::storage::{accounts, devices, history, settings};
use crate::sync::session::{self, SessionCtx, SessionState};
use crate::sync::ConnectionState;
use parking_lot::Mutex;
use rusqlite::Connection;
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

/// How long a revealed short code stays claimable.
///
/// The relay is what actually expires it; this is the same window, restated so a
/// shell can run a countdown without asking.
const PAIR_CODE_TTL_MS: i64 = 2 * 60 * 1000;

/// The long-poll window for one `/pair/poll`, kept under the relay's own so the
/// request returns rather than being cut off.
const PAIR_POLL_TIMEOUT_MS: u32 = 25_000;

/// Everything the core cannot work out for itself.
///
/// `db_path` is data, not a lookup: the core never asks the OS where an
/// application's data lives, because the answer differs per shell and on
/// Android is handed down by the framework.
///
/// `relay` is the same idea applied to the network. The core has no opinion
/// about relay schemes — see [`ServerClient::new`](crate::http::ServerClient::new)
/// — so the shell states one by
/// handing over the dial it wants:
/// [`RelayDial::over_http`](crate::relay::RelayDial::over_http) with
/// `TransportPolicy::RequireHttps` on the mobile shells,
/// `TransportPolicy::AllowCleartext` on the desktop, because a desktop
/// already paired to a cleartext relay has to keep working and baking the
/// answer in here would break an existing installation on upgrade, with no way
/// for its owner to recover the pairing.
///
/// It is also the whole of the transport seam. Every route this core calls goes
/// to a [`Relay`] this dial produced, so a test that hands over a scripted one
/// drives the real operations with no relay in reach — and no operation needs a
/// twin taking a transport as an argument.
pub struct SharepasteConfig {
    pub db_path: PathBuf,
    pub keychain: Arc<dyn Keychain>,
    pub clipboard: Arc<dyn Clipboard>,
    pub events: Arc<dyn EventSink>,
    pub relay: RelayDial,
}

/// One Pairing as a shell lists it.
///
/// `status` and `pending` are live readings rather than stored columns: a list is
/// the surface that has to show a pairing going red.
#[derive(Debug, Clone)]
pub struct PairingSummary {
    pub user_id: String,
    pub device_id: String,
    pub label: String,
    /// The User's name on the relay, mirrored by `GET /me`. `None` until this
    /// device has reached a relay that serves it.
    pub username: Option<String>,
    /// The relay's address as stored: scheme, host, whatever path it was
    /// paired at.
    pub server_url: String,
    /// The same relay as a person reads it — [`render::relay_host`]. Resolved
    /// here because both shells needed it and each parsed the URL its own way.
    pub relay_host: String,
    pub status: ConnectionState,
    pub pending: i64,
    pub is_active: bool,
}

/// The Pairing a completed handshake produced.
#[derive(Debug, Clone)]
pub struct PairedDevice {
    pub user_id: String,
    pub device_id: String,
}

/// A short code, grouped for someone to read aloud, and when it stops working.
///
/// No `Debug`, deliberately. The code *is* the pairing secret for the next two
/// minutes, and a struct that formats itself is one `tracing::debug!` away from
/// putting a live invitation in a log file.
pub struct ShortCode {
    pub code: String,
    pub expires_at: i64,
}

/// The last time anything at all arrived from the relay for one Pairing.
///
/// `None` means this device has never heard from it, which is not the same as
/// contact at the epoch.
#[derive(Debug, Clone)]
pub struct Contact {
    pub user_id: String,
    pub last_contact_at: Option<i64>,
}

/// What [`Sharepaste::recall_latest`] put on the clipboard, and where it came
/// from.
///
/// No `Debug`, for the same reason as [`ShortCode`]: `text` is whatever the
/// person copied, and that is the last thing that should be one careless format
/// string away from a log.
pub struct Recalled {
    pub text: String,
    pub entry_id: i64,
    pub created_at: i64,
    pub source: RecallSource,
}

/// Whether a recall's answer was confirmed by the relay or read off a cache that
/// could not be brought current.
///
/// A shell must surface `Cache` visibly. It is the difference between "this is
/// the newest thing you copied" and "this is the newest thing this device knows
/// about", and a silent fallback hands over yesterday's link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecallSource {
    Relay,
    Cache,
}

/// Where a page of the History resumes from: the place of the last row of the
/// page before it.
///
/// Three parts and not an id, because the History is two regions in one order
/// (ADR 0014): which region a row is in, its place inside that region, and the
/// row's own id to keep the order total when two rows share a place. Keyset
/// paging over the same three parts is what makes crossing the seam between the
/// regions no different from any other page boundary. What the parts *are* is
/// [`history`]'s to decide and nobody's to read — see [`history::Cursor`],
/// which is this type.
///
/// The id part is [`Entry::id`] — this device's own id for the row, not the
/// relay's. The relay's does not cross this facade at all, and a row that has
/// not reached the relay has none to page by.
///
/// **Nothing of this is on [`Entry`], and the reason is mechanical rather than a
/// matter of principle: a shell already holds the whole of the region.** The
/// projection is total — the History query's rank becomes a [`Queued`] and
/// `Entry::new` turns that into two fields, so `refused_reason != None` is the
/// refused region, `pending && refused_reason == None` is the pending one, and
/// `!pending` is the settled one. Adding the rank back would give a shell a
/// second way to ask the same question, and two ways is how they come to
/// disagree.
///
/// The place inside a region is the part that genuinely does not cross: it is a
/// queue position for the two device-side regions and a Last Use for the settled
/// one, and a shell needs neither, because it receives rows already in the order
/// they compose. A cursor is therefore taken off a row the core handed out, and a
/// shell that pages gets one back rather than assembling it — no shipped shell
/// pages yet, and the one that does will be handed it.
pub use crate::storage::history::Cursor as HistoryCursor;

/// How many rows the relay has ordered one Pairing keeps on this device.
///
/// The one number a shell needs from the retention rules, and the only one that
/// crosses: the age cap and the page ceiling take rows away without a surface
/// ever having to say so, but a list standing at *this* one owes the person the
/// boundary — what is not on screen may still be on the relay. Both desktop
/// surfaces draw a list-end sentinel naming it, and Android's `NO MATCHES` says
/// the same thing in a sentence.
///
/// Exported rather than mirrored, because a mirror is a second owner. This is a
/// re-export and not the declaration: [`crate::storage::history::MAX_PER_USER`]
/// owns the number, and says there how the one shell that cannot import it is
/// held to it instead.
pub use crate::storage::history::MAX_PER_USER;

/// What became of text handed to [`Sharepaste::offer`] or
/// [`Sharepaste::capture_watched`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OfferOutcome {
    /// Queued for upload. `pending` is the depth of the queue afterwards.
    Queued { pending: i64 },
    /// This device already held the same text, so nothing was captured: the
    /// entry it matched is now the head of the History. `pending` is the depth
    /// of the queue afterwards, exactly as on [`Self::Queued`] — recognition
    /// itself queues nothing, but the **Use** it records does when the relay is
    /// out of reach.
    ///
    /// Distinct from [`Self::Queued`] because a shell that reported them alike
    /// would claim content was saved when nothing was, on a list the person can
    /// immediately check.
    Recognised { pending: i64 },
    Rejected(SkipReason),
}

/// A change to the Settings, one `Option` per field: `None` leaves what is
/// stored alone.
///
/// A typed patch rather than a JSON object, because the core has no business
/// parsing arbitrary JSON and a foreign binding cannot carry it. `hotkey` is
/// doubly optional on purpose — the outer `None` means "not in this patch", the
/// inner `None` means "clear the hotkey", and a shell needs to be able to say
/// both.
///
/// `last_active_user_id` is deliberately absent. It lives in the same row but it
/// belongs to the pairing registry, and [`Sharepaste::set_active_pairing`] is the
/// only thing allowed to move it.
#[derive(Debug, Clone, Default)]
pub struct SettingsPatch {
    pub capture_enabled: Option<bool>,
    pub deny_list: Option<Vec<String>>,
    pub autostart: Option<bool>,
    pub hotkey: Option<Option<String>>,
    pub update_check_enabled: Option<bool>,
}

pub struct Sharepaste {
    state: SessionState,
    registry: Arc<PairingRegistry>,
    keychain: Arc<dyn Keychain>,
    clipboard: Arc<dyn Clipboard>,
    events: Arc<dyn EventSink>,
    /// How this facade reaches a Relay it has no Pairing for yet — an invite
    /// claim, and the claimer's half of a short code.
    ///
    /// The same dial the registry holds, so a pairing already on this device
    /// and one being made right now cross the same seam under the same policy.
    relay: RelayDial,
    last_self_write: Arc<Mutex<Option<(Instant, String)>>>,
    /// The runtime every background task runs on.
    ///
    /// Private and owned: `async fn` operations are awaited on whatever runtime
    /// the caller has, but session loops, uploaders and pair polls go here. It
    /// is what removed the shell runtime's `spawn` from the session loop, and
    /// it is why a shell with no async runtime at all — an Android activity, an
    /// FFI caller — can still hold a live session.
    ///
    /// Nothing spawned on it may hold an `Arc<Sharepaste>`: dropping the last
    /// reference from inside a worker would drop this runtime from within
    /// itself. `SessionState` exists so tasks can carry the state without the
    /// facade.
    runtime: tokio::runtime::Runtime,
}

impl Sharepaste {
    pub fn open(cfg: SharepasteConfig) -> Result<Arc<Sharepaste>, AppError> {
        let conn = crate::storage::open(&cfg.db_path)?;
        Self::over(conn, cfg)
    }

    /// Test-only: a facade over an in-memory database. `db_path` is ignored.
    #[cfg(any(test, feature = "testing"))]
    pub fn open_in_memory(cfg: SharepasteConfig) -> Result<Arc<Sharepaste>, AppError> {
        Self::over(crate::storage::open_in_memory()?, cfg)
    }

    fn over(conn: Connection, cfg: SharepasteConfig) -> Result<Arc<Sharepaste>, AppError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("sharepaste-core")
            .build()
            // `Storage` is this crate's established catch-all for a host
            // resource that will not open — the same mapping the clipboard uses.
            .map_err(|e| AppError::Storage(format!("core runtime: {e}")))?;
        let conn = Arc::new(tokio::sync::Mutex::new(conn));
        // The registry dials for every *session* request, so the same dial has
        // to reach it — otherwise a pairing stored before the shell required
        // HTTPS would keep syncing over cleartext forever.
        let registry = Arc::new(PairingRegistry::new(
            conn.clone(),
            cfg.keychain.clone(),
            cfg.relay.clone(),
        ));
        Ok(Arc::new(Sharepaste {
            state: SessionState::new(conn, runtime.handle().clone()),
            registry,
            keychain: cfg.keychain,
            clipboard: cfg.clipboard,
            events: cfg.events,
            relay: cfg.relay,
            last_self_write: Arc::new(Mutex::new(None)),
            runtime,
        }))
    }

    fn session_ctx(&self, user_id: &str) -> SessionCtx {
        SessionCtx::new(self.events.clone(), self.state.clone(), user_id.to_string())
    }

    // -- sessions ---------------------------------------------------------

    /// Start (or restart) the sync session for `user_id`.
    ///
    /// Returns once the pairing is unlocked and the two background tasks are on
    /// the runtime; it does not wait for the relay. A session already running
    /// for this user is cancelled first.
    ///
    /// One body, and the seam is under it: choosing the pairing and unlocking
    /// its key is this facade's business, and everything the protocol does
    /// afterwards goes to the [`Relay`] the pairing dialled.
    pub async fn start_session(&self, user_id: &str) -> Result<(), AppError> {
        let m = self.unlock(user_id).await?;
        // UserKey is Zeroizing<[u8;32]> with no Clone; clone the inner array via
        // a fresh Zeroizing wrapper so the session owns its own key.
        let user_key: UserKey = Zeroizing::new(*m.user_key);
        session::spawn_session(self.session_ctx(user_id), m.relay, user_key);
        Ok(())
    }

    /// Load a pairing's key and token, reporting a failure as a connection
    /// state as well as returning it: a shell that ignores the error still has
    /// to see the pairing go red.
    async fn unlock(
        &self,
        user_id: &str,
    ) -> Result<crate::pairing::registry::ActiveMembership, AppError> {
        match self.registry.load_active_membership(user_id).await {
            Ok(m) => Ok(m),
            Err(e) => {
                self.session_ctx(user_id)
                    .set_conn_state(ConnectionState::AuthFailed, Some(e.to_string()));
                Err(e)
            }
        }
    }

    /// Cancel one pairing's session.
    ///
    /// A bare cancel, unlike [`Self::stop_all_sessions`]: this is the shape
    /// `start_session` needs to replace a session with a fresh one, and the shape
    /// [`Self::forget_pairing`] needs on its way to deleting the row the Contact
    /// write would land in. Both leave `conn_states` to their own next step —
    /// `start_session` overwrites it a moment later, `forget_pairing` clears it —
    /// so neither wants a `Disconnected` transition, and neither can afford a
    /// blocking database write: `forget_pairing` is awaited on the facade's own
    /// runtime, where blocking on that lock would panic.
    pub fn stop_session(&self, user_id: &str) {
        if let Some(cancel) = self.state.sync_tasks.lock().remove(user_id) {
            cancel.cancel();
        }
        // The trigger belonged to the uploader just cancelled; `start_session`
        // registers a fresh one. Left behind, a later capture would notify a
        // task that is never going to wake.
        self.state.upload_triggers.lock().remove(user_id);
    }

    /// Stop every session without forgetting anything — what an Android
    /// `onStop` calls. `resume_active_pairing` + `start_session` undoes it.
    ///
    /// Cancelling the tasks is not enough on its own. Every cancellation arm in
    /// the session loop returns without a transition, so nothing below this
    /// method takes the edge out of `Online` — and that edge is the only thing
    /// that writes Contact to `accounts.last_contact_at` and the only thing that
    /// stops [`Self::connection_state`] answering `Online` for a session that no
    /// longer exists. A Pairing row renders that answer. So each stopped session
    /// is walked to `Disconnected` through the one choke point, here.
    ///
    /// The Contact write is **synchronous**, unlike every other transition's.
    /// This is the one lifecycle moment where the OS may stop the process
    /// immediately afterwards, and until the write lands the live `AtomicI64` is
    /// the only copy of the reading — a flush spawned onto the runtime races
    /// that kill and loses the reading whenever it loses the race, which is not
    /// meaningfully better than never writing at all. `onStop` pays a database
    /// write to make the reading survive.
    pub fn stop_all_sessions(&self) {
        let stopped: Vec<(String, CancellationToken)> =
            self.state.sync_tasks.lock().drain().collect();
        {
            let mut triggers = self.state.upload_triggers.lock();
            for (user_id, cancel) in &stopped {
                cancel.cancel();
                // The trigger belonged to the uploader just cancelled, for the
                // same reason `stop_session` drops it.
                triggers.remove(user_id);
            }
        }
        // Outside the `triggers` guard: the transition emits, and a sink is
        // foreign code — see [`EventSink`].
        for (user_id, _) in stopped {
            self.session_ctx(&user_id)
                .set_conn_state_before_returning(ConnectionState::Disconnected, None);
        }
    }

    pub fn active_pairing(&self) -> Option<String> {
        self.registry.active_user_id()
    }

    /// Where a pairing's connection stands right now.
    ///
    /// `Disconnected` for a pairing no session has ever run for — the map only
    /// holds users a session has reported on.
    pub fn connection_state(&self, user_id: &str) -> ConnectionState {
        self.state
            .conn_states
            .lock()
            .get(user_id)
            .copied()
            .unwrap_or(ConnectionState::Disconnected)
    }

    /// The live Contact reading, if a session ever took one for this user.
    ///
    /// A cell still at zero means not one byte has arrived from the relay, which
    /// is not the same as contact at the epoch; the caller falls back to the
    /// persisted value.
    fn live_contact(&self, user_id: &str) -> Option<i64> {
        self.state
            .last_contact
            .lock()
            .get(user_id)
            .map(|c| c.load(Ordering::Relaxed))
            .filter(|at| *at != 0)
    }

    /// Wake this user's uploader, because something was just queued.
    fn nudge_uploader(&self, user_id: &str) {
        match self.state.upload_triggers.lock().get(user_id) {
            Some(trigger) => trigger.notify_one(),
            None => tracing::warn!(%user_id, "no uploader trigger registered"),
        }
    }

    /// Drop the per-session bookkeeping for a pairing that is going away.
    ///
    /// Separate from [`Self::stop_session`] on purpose: a backgrounded app stops
    /// its sessions but must keep the last Contact reading answerable, and only
    /// forgetting the pairing entirely makes it meaningless.
    fn forget_session_state(&self, user_id: &str) {
        self.state.conn_states.lock().remove(user_id);
        self.state.last_contact.lock().remove(user_id);
    }

    // -- pairings ---------------------------------------------------------

    /// Every Pairing on this device, with its live connection state and queue
    /// depth.
    pub async fn list_pairings(&self) -> Result<Vec<PairingSummary>, AppError> {
        let accts = self.registry.list().await?;
        let active = self.registry.active_user_id();
        let mut out = Vec::with_capacity(accts.len());
        let conn = self.state.conn.lock().await;
        for a in accts {
            let pending = history::depth(&conn, &a.user_id)?;
            let is_active = active.as_deref() == Some(a.user_id.as_str());
            let status = self.connection_state(&a.user_id);
            out.push(PairingSummary {
                relay_host: render::relay_host(&a.server_url),
                user_id: a.user_id,
                device_id: a.device_id,
                label: a.device_label,
                username: a.username,
                server_url: a.server_url,
                status,
                pending,
                is_active,
            });
        }
        Ok(out)
    }

    /// Claim an invite and keep the Pairing it hands back.
    ///
    /// The first device onto a relay: it is the only path that mints a user key,
    /// because after this one every device is paired *to* an existing User by
    /// short code.
    pub async fn pair_with_invite(
        &self,
        server_url: &str,
        token: &str,
        device_label: &str,
    ) -> Result<PairedDevice, AppError> {
        // Trimmed once, here, and stored trimmed: an address with a stray space
        // reaches the relay perfectly well through the dial and then fails
        // to on the next launch, when it comes back out of the database.
        let server_url = server_url.trim();
        // No token: this device is not one of that relay's devices until the
        // claim it is about to make succeeds.
        let relay = self.relay.at(server_url, None)?;
        let mut claimed = claim_invite(relay.as_ref(), token, device_label)
            .await
            .map_err(|e| e.explain_insecure_relay(server_url))?;
        // The address that reached the relay from *here* is the one to keep; a
        // relay behind a reverse proxy has no reliable idea of its own.
        claimed.server_url = server_url.to_string();
        {
            let conn = self.state.conn.lock().await;
            persist_claimed_pairing(
                &conn,
                self.keychain.as_ref(),
                &claimed,
                device_label,
                crate::now_ms(),
            )?;
        }
        self.events.emit(CoreEvent::PairingAdded {
            user_id: claimed.user_id.clone(),
            device_id: claimed.device_id.clone(),
            label: device_label.to_string(),
        });
        self.activate_and_sync(&claimed.user_id).await;
        Ok(PairedDevice {
            user_id: claimed.user_id,
            device_id: claimed.device_id,
        })
    }

    /// Reveal a short code for a second device, and watch for the claim.
    ///
    /// **One** operation, and it has to stay one. It uploads the payload —
    /// encrypted to the pairing secret — *before* the code exists anywhere a
    /// shell could see it, so the claimer's fetch can never race the inviter's
    /// upload. Split into a start, a reveal and a poll for a shell to sequence,
    /// that race comes straight back: someone types the code the instant it
    /// appears, and the payload they need is not there yet. It is the class of
    /// bug you see monthly and never reproduce, which is precisely what a shared
    /// core is for.
    ///
    /// One call in, events out: the code is also reported on
    /// [`CoreEvent::PairShortcode`], and the claim or the expiry arrives later on
    /// [`CoreEvent::PairClaimed`] / [`CoreEvent::PairExpired`].
    pub async fn pair_start(&self, user_id: &str) -> Result<ShortCode, AppError> {
        let m = self.registry.load_active_membership(user_id).await?;
        let user_key: UserKey = Zeroizing::new(*m.user_key);
        let server_url = m.relay.base_url();
        let started = start_pair(m.relay.as_ref())
            .await
            .map_err(|e| e.explain_insecure_relay(&server_url))?;
        upload_pair_payload(
            m.relay.as_ref(),
            started.pair_id,
            &started.pairing_secret,
            user_id,
            &user_key,
            &server_url,
        )
        .await
        .map_err(|e| e.explain_insecure_relay(&server_url))?;

        // Only past this line does the code exist outside this function.
        let expires_at = crate::now_ms() + PAIR_CODE_TTL_MS;
        let code = group_for_display(&started.shortcode);
        self.events.emit(CoreEvent::PairShortcode {
            code: code.clone(),
            expires_at,
        });

        // The watch carries the sink and the relay and never the facade: a task
        // holding the last `Arc<Sharepaste>` would drop, from inside it, the
        // very runtime it is running on.
        self.runtime.spawn(watch_for_claim(
            m.relay,
            self.events.clone(),
            user_id.to_string(),
            started.pair_id.to_string(),
        ));

        Ok(ShortCode { code, expires_at })
    }

    /// Join an existing User by typing the short code its other device showed.
    pub async fn pair_with_code(
        &self,
        code: &str,
        device_label: &str,
    ) -> Result<PairedDevice, AppError> {
        let decoded = decode_shortcode(code)?;
        let server_url = decoded.server_url;
        // A short code carries the relay's address, so a scanned code can name a
        // cleartext relay just as a typed one can. Same choke point, same answer
        // — and no token, because this device becomes one of that relay's
        // devices only at the last of the three calls below.
        let relay = self.relay.at(&server_url, None)?;
        let proof = secret_proof_hex(&decoded.pairing_secret);
        relay
            .pair_claim(&decoded.pair_id.to_string(), &proof)
            .await
            .map_err(|e| e.explain_insecure_relay(&server_url))?;

        let payload = fetch_and_decrypt_pair_payload(
            relay.as_ref(),
            decoded.pair_id,
            &decoded.pairing_secret,
        )
        .await
        .map_err(|e| e.explain_insecure_relay(&server_url))?;
        let device = relay
            .pair_devices(&decoded.pair_id.to_string(), &proof, device_label)
            .await
            .map_err(|e| e.explain_insecure_relay(&server_url))?;

        self.keychain
            .put(&user_key_account(&payload.user_id), &payload.user_key)?;
        self.keychain
            .put(&token_account(&payload.user_id), &device.device_token)?;
        {
            let conn = self.state.conn.lock().await;
            accounts::upsert(
                &conn,
                &accounts::Account {
                    user_id: payload.user_id.clone(),
                    device_id: device.device_id.clone(),
                    device_label: device_label.to_string(),
                    // The relay's address as the *inviter* knows it, which is
                    // what travelled inside the code.
                    server_url: payload.server_url.clone(),
                    last_seen_seq: 0,
                    created_at: crate::now_ms(),
                    username: None,
                    last_contact_at: None,
                },
            )?;
        }
        self.events.emit(CoreEvent::PairingAdded {
            user_id: payload.user_id.clone(),
            device_id: device.device_id.clone(),
            label: device_label.to_string(),
        });
        self.activate_and_sync(&payload.user_id).await;
        Ok(PairedDevice {
            user_id: payload.user_id,
            device_id: device.device_id,
        })
    }

    /// Forget a Pairing: its session, its key, its token, its cache and its row.
    pub async fn forget_pairing(&self, user_id: &str) -> Result<(), AppError> {
        let was_active = self.registry.active_user_id().as_deref() == Some(user_id);
        self.stop_session(user_id);
        self.forget_session_state(user_id);

        let result = self.registry.forget(user_id).await;
        let new_active = match &result {
            Ok(next) => next.clone(),
            Err(_) => None,
        };
        // Reported before the error is propagated. Whatever went wrong, the key
        // and the token are already gone from the keychain, and a shell still
        // showing the row would offer an action nothing can carry out.
        if was_active {
            self.events.emit(CoreEvent::ActivePairingChanged {
                user_id: new_active.clone(),
            });
        }
        self.events.emit(CoreEvent::PairingRemoved {
            user_id: user_id.to_string(),
        });

        result?;

        if let Some(next) = new_active {
            self.activate_session(&next).await;
        }
        Ok(())
    }

    /// Make `user_id` the Active Pairing and bring its session up.
    pub async fn set_active_pairing(&self, user_id: &str) -> Result<(), AppError> {
        self.registry
            .set_active_persisted(Some(user_id.to_string()))
            .await?;
        self.events.emit(CoreEvent::ActivePairingChanged {
            user_id: Some(user_id.to_string()),
        });
        self.activate_session(user_id).await;
        Ok(())
    }

    /// Pick the Active Pairing back up: what a launch, or an Android `onStart`,
    /// calls before [`Self::start_session`].
    ///
    /// Falls back to the first Pairing on record when nothing was persisted, so a
    /// profile written before the choice was persisted still comes up on
    /// something. Returns the Pairing it settled on, or `None` when this device
    /// has none.
    ///
    /// Deliberately does not start the session. Foreground work is the shell's
    /// call — the core never decides for itself that now is a good time to open a
    /// socket on a metered connection.
    pub async fn resume_active_pairing(&self) -> Result<Option<String>, AppError> {
        let chosen = match self.registry.load_persisted_active().await? {
            Some(user_id) => Some(user_id),
            None => self
                .registry
                .list()
                .await?
                .into_iter()
                .next()
                .map(|a| a.user_id),
        };
        let Some(user_id) = chosen else {
            return Ok(None);
        };
        self.registry
            .set_active_persisted(Some(user_id.clone()))
            .await?;
        self.events.emit(CoreEvent::ActivePairingChanged {
            user_id: Some(user_id.clone()),
        });
        Ok(Some(user_id))
    }

    /// Persist the Active Pairing, tell the shell, and bring its session up.
    async fn activate_and_sync(&self, user_id: &str) {
        if let Err(e) = self
            .registry
            .set_active_persisted(Some(user_id.to_string()))
            .await
        {
            tracing::warn!(err = %e, "persisting the Active Pairing failed");
        }
        self.events.emit(CoreEvent::ActivePairingChanged {
            user_id: Some(user_id.to_string()),
        });
        self.activate_session(user_id).await;
    }

    /// Start the session for `user_id`, logging a pairing that will not unlock
    /// rather than failing the operation that asked for it.
    ///
    /// `start_session` already reported it as `AuthFailed` on the
    /// connection-state event, which is what a shell renders; the caller has
    /// nothing to add.
    async fn activate_session(&self, user_id: &str) {
        if let Err(e) = self.start_session(user_id).await {
            tracing::warn!(err = %e, %user_id, "starting the session failed");
        }
    }

    // -- history ----------------------------------------------------------

    /// One page of the History, last use first.
    ///
    /// `before` resumes after the last row of the previous page; `limit` is
    /// clamped to the largest page the cache will serve.
    pub async fn list_history(
        &self,
        user_id: &str,
        before: Option<HistoryCursor>,
        limit: i64,
    ) -> Result<Vec<Entry>, AppError> {
        let conn = self.state.conn.lock().await;
        let rows = history::page(&conn, user_id, before, limit)?;
        let labels = devices::map_for(&conn, user_id)?;
        Ok(to_entries(rows, &labels))
    }

    /// The full plaintext of one entry, with no clipboard involvement.
    ///
    /// `None` covers both "no such entry" and "this device cannot decrypt it":
    /// neither has a plaintext to hand over, and the entry's `undecryptable` flag
    /// is what tells them apart.
    pub async fn read_entry(
        &self,
        user_id: &str,
        entry_id: i64,
    ) -> Result<Option<String>, AppError> {
        let conn = self.state.conn.lock().await;
        history::plaintext_of(&conn, user_id, entry_id)
    }

    /// Put one entry back on the clipboard, and record the **Use**.
    ///
    /// **The use is spawned, not awaited, and that is now the whole of the
    /// decision.** This is the popover's paste-and-go path: it awaits this call
    /// before it hides, and the HTTP client has no request timeout, so awaiting
    /// a round trip here would let a relay that accepts a connection and never
    /// answers hang the paste. Nothing is lost by not waiting —
    /// [`UseRecorder::record`] returns nothing, and what it changes reaches a
    /// shell on `HistoryChanged` and `PendingCount`.
    ///
    /// There used to be a second body for tests that awaited instead, which is
    /// how a Recall came to mean one thing in the app and another in the tests
    /// that were meant to pin it. A test now waits for the effect, exactly as a
    /// shell does.
    ///
    /// The **Offer** path awaits its use instead, and says why: see
    /// [`Self::capture_or_use`].
    pub async fn recall(&self, user_id: &str, entry_id: i64) -> Result<(), AppError> {
        self.place_on_clipboard(user_id, entry_id).await?;
        // Only once the clipboard actually took it. A Recall that failed put
        // nothing anywhere, and nothing is not a use of anything.
        self.spawn_use(user_id, entry_id);
        Ok(())
    }

    /// Read one entry's plaintext and put it on the clipboard — everything a
    /// Recall is before the use is recorded.
    ///
    /// Its own function because Recall Latest performs the same write for its
    /// own reasons. The self-write marker ordering lives in `write_clipboard`
    /// and only there; nothing may reach the clipboard by another route.
    async fn place_on_clipboard(&self, user_id: &str, entry_id: i64) -> Result<(), AppError> {
        let plaintext = {
            let conn = self.state.conn.lock().await;
            history::plaintext_of(&conn, user_id, entry_id)?
                .ok_or_else(|| AppError::NotFound("plaintext unavailable".into()))?
        };
        self.write_clipboard(&plaintext)
    }

    /// Everything [`UseRecorder`] needs, taken off this facade.
    fn use_recorder(&self) -> UseRecorder {
        UseRecorder {
            state: self.state.clone(),
            registry: self.registry.clone(),
            events: self.events.clone(),
        }
    }

    /// Record a use on the facade's runtime, so the caller does not wait for a
    /// relay it has no answer to act on.
    ///
    /// The task carries a [`UseRecorder`] and never an `Arc<Sharepaste>`: this
    /// facade owns the runtime, and a task holding the last reference to it
    /// would drop that runtime from inside one of its own workers.
    fn spawn_use(&self, user_id: &str, entry_id: i64) {
        let recorder = self.use_recorder();
        let user_id = user_id.to_string();
        self.runtime.spawn(async move {
            recorder.record(&user_id, entry_id).await;
        });
    }

    /// Put the newest entry on the clipboard, fetching first.
    ///
    /// The operation that has to be right every single time, so it **always**
    /// performs the round trip and never short-circuits to the cache. The relay
    /// as built has no "newest N" route — `GET /entries?since=&limit=` answers
    /// `id > since` ascending — and widening the wire protocol for one caller is
    /// out of scope, so the shape is: bring the cache current, then read its
    /// head.
    ///
    /// [`RecallSource::Relay`] means the round trip **succeeded**, which makes the
    /// answer authoritative even when the fetch returned no rows at all: the
    /// relay has confirmed nothing newer exists. [`RecallSource::Cache`] means
    /// nobody else would give the same answer, and it is the caller's job to say
    /// so out loud.
    ///
    /// **The queue is drained first, bounded.** With acts still owed to the relay
    /// a local one and a remote entry cannot be compared — one of them has no
    /// stamp — so the answer would depend on which device was asked. Draining
    /// first puts everything on the relay's one clock and lets
    /// `(last_use DESC, id DESC)` decide, which is how every device gives the same
    /// answer. A drain that times out or fails is not fatal: the local head is
    /// still the best answer available and `Cache` is what says nobody else agrees.
    ///
    /// Known limit: an entry **deleted** on the relay while this device was away
    /// is not revealed by a `since=` fetch, so it can still be the head of the
    /// cache here and get recalled. A live session's SSE `delete` frames are what
    /// close that window, and widening the protocol to close it for a cold start
    /// is out of scope.
    pub async fn recall_latest(&self, user_id: &str) -> Result<Recalled, AppError> {
        let m = self.registry.load_active_membership(user_id).await?;
        let user_key: UserKey = Zeroizing::new(*m.user_key);
        let server_url = m.relay.base_url();
        let relay = m.relay;
        let drained = session::drain_pending(
            &self.state,
            self.events.clone(),
            user_id,
            relay.clone(),
        )
        .await;
        let mut source = RecallSource::Relay;
        if let Err(e) = drained {
            tracing::warn!(
                err = %e, %user_id,
                "recall latest could not empty the queue; its answer is this device's own"
            );
            source = RecallSource::Cache;
        }
        if let Err(e) = session::backfill(
            &self.state,
            self.events.as_ref(),
            user_id,
            &user_key,
            relay.as_ref(),
        )
        .await
        {
            // Not fatal: the newest cached entry is still the best answer
            // available, and the caller is told which one it got.
            tracing::warn!(
                err = %e.explain_insecure_relay(&server_url), %user_id,
                "recall latest could not reach the relay; falling back to the cache"
            );
            source = RecallSource::Cache;
        }
        let newest = {
            let conn = self.state.conn.lock().await;
            history::page(&conn, user_id, None, 1)?
                .into_iter()
                .next()
                .ok_or_else(|| AppError::NotFound(format!("no entries for {user_id}")))?
        };
        let entry_id = newest.local_id;
        let created_at = newest.created_at;
        // A NULL plaintext is an entry this device cannot decrypt. Handing back
        // the entry before it would be worse than failing: the person asked for
        // the newest thing they copied and would silently get an older one.
        let text = newest
            .plaintext
            .ok_or_else(|| AppError::NotFound("plaintext unavailable".into()))?;
        self.write_clipboard(&text)?;
        // After the clipboard, and over the Relay this recall already holds: the
        // entry it handed back becomes the head of the History on every device.
        // Awaited rather than spawned, unlike `recall`: this operation performs
        // a relay round trip by design — the backfill above — so its caller is
        // already prepared for one.
        self.use_recorder()
            .record_with(user_id, entry_id, relay.as_ref())
            .await;
        Ok(Recalled {
            text,
            entry_id,
            created_at,
            source,
        })
    }

    /// Delete one entry, and withdraw the act that was going to publish it.
    ///
    /// `entry_id` is this device's own id for the row; the relay is told about the
    /// id it assigned, when it has one.
    ///
    /// **A row the relay has not named needs no relay call at all.** No other
    /// device knows of it, so there is nothing out there to take back — and the
    /// queue is durable across a force-quit, so without this branch there is no
    /// way to stop a mistaken copy reaching the relay when it comes back (ADR
    /// 0016). It is the one delete that works with nothing in reach.
    ///
    /// A row that is not here is [`AppError::NotFound`] and not a quiet `Ok`: the
    /// branch above means this operation no longer consults the pairing on the way
    /// past, so nothing else would notice that there was never anything to delete.
    pub async fn delete_entry(&self, user_id: &str, entry_id: i64) -> Result<(), AppError> {
        // Three answers and not two: the relay's id, no relay id at all, or
        // `NotFound` for a row that is not here. Only the first needs the relay.
        let named = {
            let conn = self.state.conn.lock().await;
            history::relay_id_of(&conn, user_id, entry_id)?
        };
        if let Some(relay_id) = named {
            let m = self.registry.load_active_membership(user_id).await?;
            let server_url = m.relay.base_url();
            m.relay
                .delete_entry(relay_id)
                .await
                .map_err(|e| e.explain_insecure_relay(&server_url))?;
        }
        let change = {
            let conn = self.state.conn.lock().await;
            history::forget_entry(&conn, user_id, entry_id)?
        };
        self.events.emit(CoreEvent::EntryDeleted {
            user_id: user_id.to_string(),
            entry_id,
        });
        // A withdrawal changed the queue depth, and the count chrome is the only
        // thing that says so; a row the relay already took left the queue alone.
        history::announce(self.events.as_ref(), user_id, &change);
        Ok(())
    }

    /// Put a **Refused** act back in the queue.
    ///
    /// A fresh act and not a retry, so it goes to the back of the queue and
    /// thereby to the head of the History, and it carries nothing forward from the
    /// refusal that preceded it — the move copies the act and leaves
    /// `refused_at`, `attempts` and `last_error` behind with the row it replaces.
    ///
    /// **Not a Use.** There is no relay record to move: the relay never took this
    /// act, which is what a refusal means.
    ///
    /// If nothing about the cause has changed it will be refused again, which is
    /// honest. The verb is for after the relay's limit or the Active Pairing has
    /// changed, and there is no way for this device to know that has happened.
    pub async fn resend(&self, user_id: &str, entry_id: i64) -> Result<(), AppError> {
        let change = {
            let conn = self.state.conn.lock().await;
            history::resend(&conn, user_id, entry_id, crate::now_ms())?.ok_or_else(|| {
                AppError::NotFound(format!("no act queued for entry {entry_id}"))
            })?
        };
        // The row left the refused region for the head of the pending one, which
        // reorders the list. The queue depth is unchanged — the act moved, it did
        // not multiply — so nothing reports one.
        history::announce(self.events.as_ref(), user_id, &change);
        self.nudge_uploader(user_id);
        Ok(())
    }

    /// Delete every entry for one Pairing, on the relay and locally, and empty
    /// the queue with them.
    ///
    /// The relay first: a local wipe that outlived a failed remote one would put
    /// every entry straight back on the next backfill. The queue goes for the
    /// same reason — left standing it would repopulate exactly what was just
    /// cleared, which is what it used to do.
    ///
    /// One body. `delete_all_entries` is on [`Relay`] like every other route, so
    /// the local half no longer needs a private seam of its own for a test to
    /// call: a scripted relay stands in for the remote half, and the ordering
    /// between them is what a test can now see.
    pub async fn clear_history(&self, user_id: &str) -> Result<(), AppError> {
        let m = self.registry.load_active_membership(user_id).await?;
        let server_url = m.relay.base_url();
        m.relay
            .delete_all_entries()
            .await
            .map_err(|e| e.explain_insecure_relay(&server_url))?;
        let change = {
            let conn = self.state.conn.lock().await;
            history::forget_all(&conn, user_id)?
        };
        history::announce(self.events.as_ref(), user_id, &change);
        Ok(())
    }

    /// The last time anything arrived from the relay for one Pairing.
    ///
    /// Reads the live cell while a session holds one and falls back to the
    /// persisted value otherwise: a surface can open long after the last Contact
    /// event fired and still has to render something.
    pub async fn get_contact(&self, user_id: &str) -> Result<Contact, AppError> {
        let last_contact_at = match self.live_contact(user_id) {
            Some(at) => Some(at),
            None => {
                let conn = self.state.conn.lock().await;
                accounts::find(&conn, user_id)?.and_then(|a| a.last_contact_at)
            }
        };
        Ok(Contact {
            user_id: user_id.to_string(),
            last_contact_at,
        })
    }

    // -- capture ----------------------------------------------------------

    /// Offered Capture: take text the person handed over deliberately.
    ///
    /// Honoured whether or not capture is enabled: refusing content someone just
    /// chose to share is indefensible, and `capture_enabled` governs Watched
    /// Capture — which a phone never performs. Over-size and non-text still
    /// reject, because those are properties of the text itself rather than of a
    /// watcher.
    pub async fn offer(&self, user_id: &str, text: &str) -> Result<OfferOutcome, AppError> {
        let ctx = CaptureContext {
            // An Offered Capture is honoured regardless of the setting; see above.
            capture_enabled: true,
            // Inert: the deny-list exists so a watcher does not sweep up a
            // password manager's clipboard, and it is matched against the
            // frontmost application — of which there is none to attribute an
            // Offered Capture to.
            deny_list: &[],
            frontmost_bundle_id: None,
            // Inert: self-write suppression stops a watcher re-capturing this
            // app's own clipboard write — see `write_clipboard`. With nothing
            // watching there is nothing to suppress.
            last_self_write: None,
        };
        // Written out rather than forked away, so there is exactly one filter:
        // `NonText` and `TooLarge` stay reachable here, and `Disabled`,
        // `DenyList` and `SelfWrite` are unreachable by construction — as is
        // `Transient`, because nothing sniffs a pasteboard on this path.
        match filter::evaluate_text(&ctx, text.to_string(), Instant::now()) {
            FilterDecision::Skip(reason) => Ok(OfferOutcome::Rejected(reason)),
            FilterDecision::Capture(text) => self.capture_or_use(user_id, text).await,
        }
    }

    /// Take whatever the watcher just saw on the pasteboard.
    ///
    /// Desktop-only, and the one operation that takes a trait object rather than
    /// data: `PasteboardSniff` reads the available *types* first and the text only
    /// if none of them is transient or concealed. A pre-read snapshot struct would
    /// have pulled a concealed password's plaintext into memory before the filter
    /// could reject it. It is never exported over FFI, because no phone watches a
    /// clipboard.
    pub async fn capture_watched(
        &self,
        user_id: &str,
        sniff: &dyn PasteboardSniff,
        frontmost: Option<&str>,
    ) -> Result<OfferOutcome, AppError> {
        let settings = {
            let conn = self.state.conn.lock().await;
            settings::load(&conn)?
        };
        let last_self_write = self.last_self_write.lock().clone();
        let ctx = CaptureContext {
            capture_enabled: settings.capture_enabled,
            deny_list: &settings.deny_list,
            frontmost_bundle_id: frontmost,
            last_self_write: last_self_write.as_ref().map(|(at, text)| (*at, text.as_str())),
        };
        match filter::evaluate(&ctx, sniff, Instant::now()) {
            FilterDecision::Skip(reason) => Ok(OfferOutcome::Rejected(reason)),
            FilterDecision::Capture(text) => self.capture_or_use(user_id, text).await,
        }
    }

    /// Recognise, or take — the tail Offered and Watched Capture share.
    ///
    /// Runs after the filter has said `Capture` and **before** anything is
    /// encrypted, because the whole point is to spend nothing on text this
    /// device already holds. The relay cannot make this judgement:
    /// `crypto::encrypt` draws a fresh nonce every call, so the same plaintext
    /// never produces the same ciphertext. See ADR 0012.
    ///
    /// Entries first, then pendings. A matched pending has no relay id and so
    /// cannot be used; moving it to the back of the queue is what "re-copying
    /// it is the same act as copying it" looks like when the order is the only
    /// record there is.
    ///
    /// **The use is awaited here, unlike [`Self::recall`]'s.** Both outcomes
    /// report `pending`, and a depth read while a use was still being queued
    /// behind it would be a number the very next `PendingCount` contradicts —
    /// on a phone whose Share Sheet then drains the queue and stops when it
    /// reads zero, leaving the use behind. An Offer is already a slow path:
    /// `enqueue_capture` unlocks the pairing and encrypts, and the Android
    /// caller drains the queue over the network immediately afterwards. A
    /// Recall is not, which is why the two differ.
    async fn capture_or_use(
        &self,
        user_id: &str,
        text: String,
    ) -> Result<OfferOutcome, AppError> {
        let held = {
            let conn = self.state.conn.lock().await;
            history::recognise(&conn, user_id, &text)?
        };
        match held {
            Held::Nothing => self.enqueue_capture(user_id, text).await,
            Held::Entry(entry_id) => {
                self.use_recorder().record(user_id, entry_id).await;
                Ok(OfferOutcome::Recognised {
                    pending: self.pending_depth(user_id).await?,
                })
            }
            Held::Queued(entry_id) => {
                let pending = {
                    let conn = self.state.conn.lock().await;
                    history::resend(&conn, user_id, entry_id, crate::now_ms())?;
                    history::depth(&conn, user_id)?
                };
                // Nothing is announced, and that is preserved rather than
                // chosen: the move reorders the list exactly as `resend` does,
                // and every other caller of it says so. Saying so here too is a
                // behaviour change and belongs to its own ticket.
                Ok(OfferOutcome::Recognised { pending })
            }
        }
    }

    async fn pending_depth(&self, user_id: &str) -> Result<i64, AppError> {
        let conn = self.state.conn.lock().await;
        history::depth(&conn, user_id)
    }

    /// Encrypt, store the Entry, queue the act, report.
    ///
    /// The Entry and the act go in together — one transaction, in
    /// [`history::capture`], which is the only door to either half.
    ///
    /// `EntryAdded` fires here and nowhere else for a capture. The flush that
    /// follows creates nothing and announces nothing.
    async fn enqueue_capture(
        &self,
        user_id: &str,
        text: String,
    ) -> Result<OfferOutcome, AppError> {
        let m = self.registry.load_active_membership(user_id).await?;
        let ciphertext = crate::crypto::encrypt(&m.user_key, user_id, text.as_bytes())?;
        let (entry, captured) = {
            let conn = self.state.conn.lock().await;
            // This device is the Entry's Origin, and the pairing row is where
            // its id lives. Without one there is nothing to attribute the row
            // to, which is a broken pairing rather than a capture to guess at.
            let device_id = accounts::find(&conn, user_id)?
                .ok_or_else(|| AppError::NotFound(format!("no pairing for {user_id}")))?
                .device_id;
            let captured = history::capture(
                &conn, user_id, &ciphertext, &text, &device_id, crate::now_ms(),
            )?;
            let device_label =
                devices::label_for(&conn, user_id, &device_id).unwrap_or_default();
            let entry = Entry::new(
                captured.local_id,
                user_id.to_string(),
                Some(text),
                // No relay stamp yet, and no device clock standing in for one:
                // what orders this row is its place in the queue.
                0,
                0,
                device_id,
                device_label,
                Queued::Pending,
            );
            (entry, captured)
        };
        self.events.emit(CoreEvent::EntryAdded {
            user_id: user_id.to_string(),
            entry,
        });
        history::announce(self.events.as_ref(), user_id, &captured.change);
        self.nudge_uploader(user_id);
        Ok(OfferOutcome::Queued {
            pending: captured.change.depth.unwrap_or_default(),
        })
    }

    // -- settings ---------------------------------------------------------

    pub async fn get_settings(&self) -> Result<Settings, AppError> {
        let conn = self.state.conn.lock().await;
        settings::load(&conn)
    }

    /// Apply a patch and hand back what is now stored.
    ///
    /// The side effects a change implies — re-registering a global hotkey,
    /// writing a login item — belong to the shell that owns those mechanisms. It
    /// works out which of them fired by comparing this return value against what
    /// it read before, which is also the only way that comparison stays honest
    /// when a field is patched to the value it already had.
    pub async fn update_settings(&self, patch: SettingsPatch) -> Result<Settings, AppError> {
        let conn = self.state.conn.lock().await;
        let mut s = settings::load(&conn)?;
        if let Some(v) = patch.capture_enabled {
            s.capture_enabled = v;
        }
        if let Some(v) = patch.deny_list {
            s.deny_list = v;
        }
        if let Some(v) = patch.autostart {
            s.autostart = v;
        }
        if let Some(v) = patch.update_check_enabled {
            s.update_check_enabled = v;
        }
        // `Some(None)` clears the binding; an absent `hotkey` leaves it alone.
        if let Some(v) = patch.hotkey {
            s.hotkey = v;
        }
        settings::save(&conn, &s)?;
        Ok(s)
    }

    // -- clipboard --------------------------------------------------------

    /// Put `text` on the clipboard, behind the self-write marker.
    ///
    /// **The self-write invariant, stated once.** Every clipboard write the app
    /// performs goes through here, and the marker is recorded *before* the
    /// write: the watcher can observe the new clipboard contents the instant the
    /// write lands, so a marker recorded afterwards leaves a window in which the
    /// app's own clipboard write is captured, uploaded and handed straight back.
    /// A write that fails clears the marker again — but only if it still names
    /// this text, so a concurrent write's marker is not thrown away.
    ///
    /// A shell that reached for [`Clipboard::write_text`] directly would skip
    /// all of it. See `capture::filter`'s `SelfWrite` skip for the half that
    /// reads the marker.
    pub fn write_clipboard(&self, text: &str) -> Result<(), AppError> {
        *self.last_self_write.lock() = Some((Instant::now(), text.to_string()));
        if let Err(err) = self.clipboard.write_text(text) {
            let mut marker = self.last_self_write.lock();
            if marker.as_ref().is_some_and(|(_, marked)| marked == text) {
                *marker = None;
            }
            return Err(err);
        }
        Ok(())
    }

    /// Run a facade operation to completion from a caller that has no async
    /// runtime of its own — the UniFFI boundary and the core's own integration
    /// tests, both of which call in from a plain thread.
    ///
    /// The runtime handle itself stays private. A caller that could reach it
    /// could spawn onto it, and what runs on the facade's runtime is the
    /// facade's business.
    ///
    /// Panics if called from a thread already inside this runtime; every caller
    /// is a foreign thread by construction.
    pub fn block_on<F: Future>(&self, fut: F) -> F::Output {
        self.runtime.block_on(fut)
    }
}

/// Recording a **Use**, apart from the facade that provokes one.
///
/// Its own type for two reasons. It is spawned: `Sharepaste` owns the runtime
/// these tasks run on, and a task holding the last `Arc<Sharepaste>` would drop
/// that runtime from inside one of its own workers — the same reason
/// [`SessionState`] exists. And there are three provokers — a Recall, a Recall
/// Latest, and a copy of text the device already holds — so a second copy of
/// these rules is how they would come to disagree about what a use costs when
/// it fails.
#[derive(Clone)]
struct UseRecorder {
    state: SessionState,
    registry: Arc<PairingRegistry>,
    events: Arc<dyn EventSink>,
}

impl UseRecorder {
    /// Record a **Use** of one entry: it becomes the head of the History on
    /// every device.
    ///
    /// **Never fails the operation that provoked it**, which is why it returns
    /// nothing. A Recall has already reached the clipboard by the time this
    /// runs and a recognised copy has already decided nothing needs storing;
    /// neither is undone by a relay that will not answer.
    async fn record(&self, user_id: &str, entry_id: i64) {
        let m = match self.registry.load_active_membership(user_id).await {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(err = %e, %user_id, "no pairing to record a use against");
                return;
            }
        };
        self.record_with(user_id, entry_id, m.relay.as_ref()).await;
    }

    /// The same, over a Relay the caller already holds — `recall_latest` has
    /// one, and dialling a second is a second round trip for nothing.
    ///
    /// The relay's `last_use` is applied to the local row directly rather than
    /// waited for on the echo — the rule the uploader's reconciliation states
    /// for an Entry this device created, applied to a Use this device made. The
    /// watermark is deliberately not advanced with it, for the same reason: it
    /// means "everything up to here has been fetched", and this device has
    /// fetched nothing.
    async fn record_with(&self, user_id: &str, entry_id: i64, relay: &dyn Relay) {
        // `entry_id` is this device's own; the relay has never heard of it. A row
        // with no relay id has no relay record to move, so the use it just
        // received is carried by the queue instead. A row that is not here at all
        // is nothing to move either way.
        let relay_id = {
            let conn = self.state.conn.lock().await;
            match history::relay_id_of(&conn, user_id, entry_id) {
                Ok(Some(relay_id)) => relay_id,
                Ok(None) => {
                    drop(conn);
                    self.move_act_to_back(user_id, entry_id).await;
                    return;
                }
                Err(AppError::NotFound(_)) => {
                    tracing::warn!(entry_id, "a use arrived for an entry this device does not hold");
                    return;
                }
                Err(e) => {
                    tracing::warn!(err = %e, entry_id, "could not resolve this entry's relay id");
                    return;
                }
            }
        };
        match relay.use_entry(relay_id).await {
            Ok(used) => {
                let change = {
                    let conn = self.state.conn.lock().await;
                    match history::record_use(&conn, user_id, relay_id, used.last_use) {
                        Ok(change) => change,
                        // Logged rather than swallowed: a cache write that
                        // failed must not be indistinguishable from a use of an
                        // entry this device no longer holds. The relay has the
                        // use either way, and the next backfill brings the row
                        // down with its new Last Use.
                        Err(e) => {
                            tracing::warn!(err = %e, entry_id, "the relay recorded this use but the cache did not");
                            history::Change::reorder(false)
                        }
                    }
                };
                history::announce(self.events.as_ref(), user_id, &change);
            }
            // The relay does not have this entry — or is older than this client
            // and does not have the route. Neither earns a queued retry: skew is
            // not handled here, the relay is updated first, and an entry that is
            // gone has nothing left to reorder.
            Err(AppError::NotFound(e)) => {
                tracing::info!(err = %e, entry_id, "the relay did not record this use");
            }
            Err(e) => {
                tracing::warn!(err = %e, entry_id, "queuing a use the relay could not be told about");
                if let Err(e) = self.queue(user_id, entry_id, relay_id).await {
                    tracing::warn!(err = %e, entry_id, "and it could not be queued either");
                }
            }
        }
    }

    /// Put a use in the pending queue, beside the captures, so an outage cannot
    /// reorder what happened during it.
    ///
    /// Both ids travel: the relay is told about `relay_id`, and `entry_id` is the
    /// row this device shows and orders the act against.
    async fn queue(&self, user_id: &str, entry_id: i64, relay_id: i64) -> Result<(), AppError> {
        let change = {
            let conn = self.state.conn.lock().await;
            history::queue_use(&conn, user_id, entry_id, relay_id, crate::now_ms())?
        };
        history::announce(self.events.as_ref(), user_id, &change);
        match self.state.upload_triggers.lock().get(user_id) {
            Some(trigger) => trigger.notify_one(),
            None => tracing::warn!(%user_id, "no uploader trigger registered"),
        }
        Ok(())
    }

    /// Move the act an un-flushed entry already owes the relay to the back of the
    /// queue.
    ///
    /// **No new queue item and no third kind of act.** A row with no relay id has
    /// nothing the relay could record a use against, and using it ends in exactly
    /// the state re-copying it ends in (ADR 0012): this text is the most recent
    /// thing this device did, and it is still owed. The queue's order is the only
    /// record of that, so the act moves rather than multiplying.
    ///
    /// The move already drops the attempt count and the last error as "a fresh
    /// act, not a retry", which is what a Recall is.
    ///
    /// The queue depth does not change, so nothing reports one. What did change is
    /// where the row sits, and `HistoryChanged` is what says so.
    async fn move_act_to_back(&self, user_id: &str, entry_id: i64) {
        let change = {
            let conn = self.state.conn.lock().await;
            match history::resend(&conn, user_id, entry_id, crate::now_ms()) {
                Ok(Some(change)) => change,
                Ok(None) => {
                    // No relay id and no queued act: the row is beyond anything
                    // this can do for it, which is the state a withdrawn act
                    // leaves and nothing else reaches.
                    tracing::info!(entry_id, "no act queued for this entry; nothing to move");
                    return;
                }
                Err(e) => {
                    tracing::warn!(err = %e, entry_id, "could not find the act queued for this entry");
                    return;
                }
            }
        };
        history::announce(self.events.as_ref(), user_id, &change);
    }
}

/// Rows of the History as a shell renders them, with each Origin resolved
/// against the Device mirror.
///
/// A `device_id` the mirror has never heard of keeps a `None` label rather than
/// failing the row: a device paired since the last `GET /me`, or a relay too old
/// to serve one, is expected — [`Entry::new`] falls back to a slice of the id.
fn to_entries(
    rows: Vec<history::CachedEntry>,
    labels: &HashMap<String, String>,
) -> Vec<Entry> {
    rows.into_iter()
        .map(|r| {
            let device_label = labels.get(&r.device_id).cloned();
            let queued = r.queued;
            Entry::new(
                r.local_id,
                r.user_id,
                r.plaintext,
                r.created_at,
                r.last_use,
                r.device_id,
                device_label,
                queued,
            )
        })
        .collect()
}

/// Poll until the short code is claimed or expires, then report it once.
///
/// A failure that is not an expiry is retried rather than reported: a phone that
/// loses the network while its code is on screen has until the relay expires the
/// code to get it back, and the relay is what decides when that is.
async fn watch_for_claim(
    relay: Arc<dyn Relay>,
    events: Arc<dyn EventSink>,
    user_id: String,
    pair_id: String,
) {
    loop {
        match relay.pair_poll(&pair_id, PAIR_POLL_TIMEOUT_MS).await {
            Ok(PairClaim::Consumed { device_label }) => {
                events.emit(CoreEvent::PairClaimed {
                    user_id,
                    device_label,
                });
                return;
            }
            Ok(PairClaim::Expired) | Err(AppError::PairExpired(_)) => {
                events.emit(CoreEvent::PairExpired);
                return;
            }
            Ok(PairClaim::Waiting) => continue,
            Err(e) => {
                tracing::warn!(err = %e, "pair poll errored");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::filter::MAX_BYTES;
    use crate::http::TransportPolicy;
    use crate::keychain::{token_account, user_key_account, InMemoryKeychain};
    use crate::storage::accounts;
    use crate::testing::{
        encrypted_row, live_entry, FakeClipboard, FakePasteboard, RecordingSink, ScriptedRelay,
        Wire, SCRIPTED_DEVICE_ID, SCRIPTED_PAIR_ID, SCRIPTED_RELAY_URL, SCRIPTED_USER_ID,
        TEST_USER_KEY_HEX,
    };

    /// The desktop's dial: a real HTTP client that permits cleartext.
    ///
    /// What a test that is about an *address* wants — a refused port, a
    /// cleartext relay nobody is serving. The scheme rule itself has its own
    /// tests in `http::client` and `pairing::registry`.
    fn over_http() -> RelayDial {
        RelayDial::over_http(TransportPolicy::AllowCleartext)
    }

    fn facade(clipboard: Arc<FakeClipboard>) -> Arc<Sharepaste> {
        Sharepaste::open_in_memory(SharepasteConfig {
            db_path: PathBuf::from("ignored-by-open_in_memory"),
            keychain: Arc::new(InMemoryKeychain::default()),
            clipboard,
            events: Arc::new(RecordingSink::default()),
            relay: over_http(),
        })
        .unwrap()
    }

    #[test]
    fn self_write_guard_sets_marker_before_clipboard_write() {
        let clipboard = FakeClipboard::new();
        let sp = facade(clipboard.clone());
        let marker = sp.last_self_write.clone();
        let plaintext = "secret".to_string();

        let expected = plaintext.clone();
        clipboard.on_write(move |text| {
            let marker = marker.lock();
            let (_, marked_text) = marker.as_ref().expect("self-write marker should be set");
            assert_eq!(marked_text, &expected);
            assert_eq!(text, expected);
            Ok(())
        });

        sp.write_clipboard(&plaintext).unwrap();
        assert_eq!(clipboard.writes(), vec![plaintext], "the text must reach the clipboard");
    }

    #[test]
    fn self_write_guard_clears_matching_marker_when_clipboard_write_fails() {
        let clipboard = FakeClipboard::new();
        let sp = facade(clipboard.clone());
        let marker = sp.last_self_write.clone();
        let plaintext = "secret".to_string();

        let expected = plaintext.clone();
        clipboard.on_write(move |text| {
            let marker = marker.lock();
            let (_, marked_text) = marker.as_ref().expect("self-write marker should be set");
            assert_eq!(marked_text, &expected);
            assert_eq!(text, expected);
            // Released before the failing write returns: the facade retakes
            // this same lock to clear the marker.
            drop(marker);
            Err(AppError::Storage("clipboard failed".into()))
        });

        let err = sp.write_clipboard(&plaintext).unwrap_err();
        assert!(matches!(err, AppError::Storage(_)));
        assert!(sp.last_self_write.lock().is_none());
    }

    /// A pairing whose key and token live in the fake keychain, exactly as a
    /// real one's live in the system's.
    fn seed_pairing(sp: &Sharepaste, keychain: &InMemoryKeychain, server_url: &str) {
        keychain.put(&user_key_account("u"), TEST_USER_KEY_HEX).unwrap();
        keychain.put(&token_account("u"), "device-token").unwrap();
        sp.runtime.block_on(async {
            let conn = sp.state.conn.lock().await;
            accounts::upsert(
                &conn,
                &accounts::Account {
                    user_id: "u".into(),
                    device_id: "d".into(),
                    device_label: "mac".into(),
                    server_url: server_url.into(),
                    last_seen_seq: 0,
                    created_at: 1,
                    username: None,
                    last_contact_at: None,
                },
            )
            .unwrap();
        });
    }

    /// Poll until `pred` holds. The session runs on the facade's own runtime,
    /// which is the point of this test — so the test thread watches rather than
    /// awaits.
    fn until(pred: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if pred() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("timed out waiting for the session");
    }

    /*
     * The whole facade over the three seams and nothing else: a fake keychain
     * unlocks the pairing, a scripted relay stands in for the network, a fake
     * clipboard takes the write, and every report arrives on the sink. No
     * Tauri, no relay, no runtime supplied by the caller.
     */
    #[test]
    fn a_session_runs_over_the_three_seams_with_no_relay() {
        let clipboard = FakeClipboard::new();
        let keychain = Arc::new(InMemoryKeychain::default());
        let sink = Arc::new(RecordingSink::default());
        let relay = ScriptedRelay::new(
            vec![Ok(vec![encrypted_row(1, "u", "from the backfill", "d")])],
            vec![Wire::Holds(vec![live_entry(2, "u", "live one", "d")])],
        );
        let sp = Sharepaste::open_in_memory(SharepasteConfig {
            db_path: PathBuf::from("ignored-by-open_in_memory"),
            keychain: keychain.clone(),
            clipboard: clipboard.clone(),
            events: sink.clone(),
            relay: relay.dial(),
        })
        .unwrap();
        seed_pairing(&sp, &keychain, "https://srv");

        sp.runtime.block_on(sp.start_session("u")).unwrap();

        until(|| sink.entry_previews() == ["live one"]);
        assert_eq!(sp.connection_state("u"), ConnectionState::Online);
        assert!(sp.live_contact("u").is_some(), "the byte tap stamped Contact");
        assert!(sink.saw_history_changed("u"), "the backfill wrote rows");

        // The clipboard seam, through the one path that is allowed to use it.
        sp.write_clipboard("recalled").unwrap();
        assert_eq!(clipboard.writes(), vec!["recalled".to_string()]);
        assert_eq!(
            sp.last_self_write.lock().as_ref().map(|(_, t)| t.clone()),
            Some("recalled".to_string()),
            "and the marker the watcher reads is set"
        );

        // Cancellation reaches all the way down into the stream.
        sp.stop_session("u");
        until(|| relay.streams_closed() == 1);
    }

    /*
     * A pairing the keychain cannot unlock is a red pairing, not a silent one:
     * a shell that discards the error still has to see the state change.
     */
    #[test]
    fn a_pairing_with_no_key_reports_auth_failed_as_well_as_erroring() {
        let sink = Arc::new(RecordingSink::default());
        let sp = Sharepaste::open_in_memory(SharepasteConfig {
            db_path: PathBuf::from("ignored-by-open_in_memory"),
            keychain: Arc::new(InMemoryKeychain::default()),
            clipboard: FakeClipboard::new(),
            events: sink.clone(),
            relay: over_http(),
        })
        .unwrap();

        let err = sp
            .runtime
            .block_on(sp.start_session("nobody"))
            .unwrap_err();

        assert!(matches!(err, AppError::NotFound(_)));
        assert_eq!(sink.connection_states(), vec![ConnectionState::AuthFailed]);
        assert_eq!(sp.connection_state("nobody"), ConnectionState::AuthFailed);
    }

    /*
     * What an Android `onStop` calls, on the "forgets nothing" side. The streams
     * end and the Contact reading stays answerable, because the next `onStart`
     * has to render something before the relay answers; only forgetting the
     * Pairing itself takes it away.
     */
    #[test]
    fn stop_all_sessions_ends_the_streams_but_forgets_nothing() {
        let (sp, _sink, relay) = backgrounded_session();

        until(|| relay.streams_closed() == 1);
        assert!(sp.live_contact("u").is_some(), "the last reading is still answerable");

        sp.forget_session_state("u");
        assert_eq!(sp.connection_state("u"), ConnectionState::Disconnected);
        assert_eq!(sp.live_contact("u"), None);
    }

    /*
     * The other side, and the bug: `onStop` used to only cancel. Every
     * cancellation arm in the session loop returns without a transition, so the
     * edge out of `Online` — the one thing that writes Contact and the one thing
     * that stops `connection_state` claiming `Online` — never happened. That left
     * the only copy of the reading in an `AtomicI64` at the exact moment the OS
     * may stop the process, and left a Pairing row rendering `Online` for a
     * session that no longer existed.
     */
    #[test]
    fn stop_all_sessions_writes_contact_and_stops_claiming_online() {
        let keychain = Arc::new(InMemoryKeychain::default());
        let sink = Arc::new(RecordingSink::default());
        let relay = ScriptedRelay::new(vec![Ok(Vec::new())], vec![Wire::Holds(Vec::new())]);
        let sp = Sharepaste::open_in_memory(SharepasteConfig {
            db_path: PathBuf::from("ignored-by-open_in_memory"),
            keychain: keychain.clone(),
            clipboard: FakeClipboard::new(),
            events: sink.clone(),
            relay: relay.dial(),
        })
        .unwrap();
        seed_pairing(&sp, &keychain, "https://srv");

        sp.runtime.block_on(sp.start_session("u")).unwrap();
        until(|| sp.connection_state("u") == ConnectionState::Online);
        assert_eq!(
            persisted_contact(&sp),
            None,
            "a healthy session keeps the reading in the cell and off SQLite"
        );

        sp.stop_all_sessions();

        // Deliberately not wrapped in `until`. The write is synchronous, and
        // asserting it the statement after the call is what distinguishes that
        // from a flush handed to the runtime: polling would pass either way, and
        // against a process the OS is about to stop, only one of them is true.
        let live = sp.live_contact("u").expect("the byte tap took a reading");
        assert_eq!(
            persisted_contact(&sp),
            Some(live),
            "the last reading is in the database before onStop returns"
        );
        assert_eq!(
            sp.connection_state("u"),
            ConnectionState::Disconnected,
            "and no Pairing row can still render Online"
        );
        assert_eq!(
            sink.connection_states().last().copied(),
            Some(ConnectionState::Disconnected),
            "reported, because the transition went through the one choke point"
        );
        assert_eq!(
            sink.events().into_iter().find_map(|e| match e {
                CoreEvent::Contact { last_contact_at, .. } => last_contact_at,
                _ => None,
            }),
            Some(live),
            "with the same number the database now holds"
        );
    }

    /// A session that reached `Online` and has since been backgrounded.
    fn backgrounded_session() -> (Arc<Sharepaste>, Arc<RecordingSink>, Arc<ScriptedRelay>) {
        let keychain = Arc::new(InMemoryKeychain::default());
        let sink = Arc::new(RecordingSink::default());
        let relay = ScriptedRelay::new(vec![Ok(Vec::new())], vec![Wire::Holds(Vec::new())]);
        let sp = Sharepaste::open_in_memory(SharepasteConfig {
            db_path: PathBuf::from("ignored-by-open_in_memory"),
            keychain: keychain.clone(),
            clipboard: FakeClipboard::new(),
            events: sink.clone(),
            relay: relay.dial(),
        })
        .unwrap();
        seed_pairing(&sp, &keychain, "https://srv");

        sp.runtime.block_on(sp.start_session("u")).unwrap();
        until(|| sp.connection_state("u") == ConnectionState::Online);
        sp.stop_all_sessions();
        (sp, sink, relay)
    }

    /// What `accounts.last_contact_at` holds, which is the only copy that
    /// survives the process.
    fn persisted_contact(sp: &Sharepaste) -> Option<i64> {
        sp.runtime.block_on(async {
            let conn = sp.state.conn.lock().await;
            accounts::find(&conn, "u").unwrap().unwrap().last_contact_at
        })
    }

    // -- the operation surface --------------------------------------------

    /// A facade with one unlocked pairing on it, and the two fakes its reports
    /// and its clipboard writes land on.
    struct Rig {
        sp: Arc<Sharepaste>,
        sink: Arc<RecordingSink>,
        clipboard: Arc<FakeClipboard>,
    }

    /// A rig whose relay is the shell's own HTTP dial, aimed at `server_url`.
    ///
    /// Nothing here reaches a network, and every rig points at a port that
    /// refuses instantly, so an operation that *does* try is a fast,
    /// deterministic failure rather than a hang. What a test about an address
    /// wants; [`rig_over`] is what a test about an answer wants.
    fn rig_at(server_url: &str) -> Rig {
        rig_dialling(server_url, over_http())
    }

    /// A rig whose every address reaches one scripted relay.
    ///
    /// The seam under the facade: the operations below are the ones a shell
    /// calls, driven with no relay in reach and no twin standing in for them.
    fn rig_over(relay: &Arc<ScriptedRelay>) -> Rig {
        rig_dialling(SCRIPTED_RELAY_URL, relay.dial())
    }

    fn rig_dialling(server_url: &str, dial: RelayDial) -> Rig {
        let keychain = Arc::new(InMemoryKeychain::default());
        let sink = Arc::new(RecordingSink::default());
        let clipboard = FakeClipboard::new();
        let sp = Sharepaste::open_in_memory(SharepasteConfig {
            db_path: PathBuf::from("ignored-by-open_in_memory"),
            keychain: keychain.clone(),
            clipboard: clipboard.clone(),
            events: sink.clone(),
            relay: dial,
        })
        .unwrap();
        seed_pairing(&sp, &keychain, server_url);
        Rig { sp, sink, clipboard }
    }

    fn rig() -> Rig {
        rig_at("http://127.0.0.1:1")
    }

    /// Put one entry in the cache exactly the way a backfill would, and hand back
    /// this device's id for it — `id` is the relay's, which no longer crosses the
    /// facade.
    fn seed_entry(sp: &Sharepaste, id: i64, plaintext: &str) -> i64 {
        sp.runtime.block_on(async {
            let conn = sp.state.conn.lock().await;
            crate::sync::decryptor::ingest(
                &conn,
                &crate::testing::test_user_key(),
                "u",
                &encrypted_row(id, "u", plaintext, "d"),
                crate::now_ms(),
            )
            .unwrap()
            .stored
            .local_id
        })
    }

    /*
     * Pairing start is one operation, and this is why. The hook runs *inside*
     * `put_payload`, which is the only vantage point from which a test can look
     * at everything a shell can see and check that the code is not among it yet.
     * A `pair_start` that revealed first — or that a shell could drive as three
     * calls — would have the code sitting on the sink right here, and the claimer
     * would fetch a payload that has not been uploaded.
     */
    #[test]
    fn pair_start_uploads_the_payload_before_the_code_can_be_observed() {
        let relay = ScriptedRelay::new(Vec::new(), Vec::new()).pairing(vec![Ok(
            PairClaim::Consumed { device_label: Some("mac-B".into()) },
        )]);
        let Rig { sp, sink, .. } = rig_over(&relay);

        let observed: Arc<Mutex<Option<Vec<String>>>> = Arc::new(Mutex::new(None));
        let seen = observed.clone();
        let watched_sink = sink.clone();
        relay.on_put_payload(move || {
            *seen.lock() = Some(watched_sink.shortcodes());
        });

        let revealed = sp
            .runtime
            .block_on(sp.pair_start("u"))
            .unwrap();

        let during_upload = observed
            .lock()
            .clone()
            .expect("the payload upload must have happened at all");
        assert!(
            during_upload.is_empty(),
            "no code may be observable until the payload upload has completed, saw {during_upload:?}"
        );
        assert_eq!(relay.pair_payloads().len(), 1, "and the payload really was uploaded");
        assert_eq!(
            sink.shortcodes(),
            vec![revealed.code.clone()],
            "the code is revealed exactly once, afterwards, and matches what was returned"
        );
        assert!(revealed.expires_at > crate::now_ms(), "with a window left to type it in");

        // What the claimer types has to lead back to the payload just uploaded.
        let decoded = decode_shortcode(&revealed.code).unwrap();
        assert_eq!(decoded.pair_id.to_string(), SCRIPTED_PAIR_ID);
        assert_eq!(decoded.server_url, SCRIPTED_RELAY_URL);

        // And the claim arrives as an event, not as a return value: one call in,
        // events out.
        until(|| sink.pair_claimed() == vec![Some("mac-B".to_string())]);
    }

    /// A facade with no Pairing at all, dialling one scripted relay.
    ///
    /// The two pairing operations are the ones that *make* a Pairing, so they
    /// cannot start from a rig that already has one.
    fn unpaired_over(relay: &Arc<ScriptedRelay>) -> (Arc<Sharepaste>, Arc<RecordingSink>) {
        let sink = Arc::new(RecordingSink::default());
        let sp = Sharepaste::open_in_memory(SharepasteConfig {
            db_path: PathBuf::from("ignored-by-open_in_memory"),
            keychain: Arc::new(InMemoryKeychain::default()),
            clipboard: FakeClipboard::new(),
            events: sink.clone(),
            relay: relay.dial(),
        })
        .unwrap();
        (sp, sink)
    }

    /*
     * The first device onto a relay, all the way through, with no relay in
     * reach. Only the failure path could be driven before the seam moved, which
     * left the operation's whole *effect* — a key and a token in the keychain, a
     * Pairing row, the Active Pairing, the report — resting on the flow test and
     * on a live relay.
     *
     * `pair_with_invite` is also the only path that mints a user key, so the
     * key it stores is this device's own and not one the relay handed over.
     */
    #[test]
    fn pairing_with_an_invite_keeps_the_pairing_it_was_handed() {
        let relay = ScriptedRelay::new(Vec::new(), Vec::new());
        let (sp, sink) = unpaired_over(&relay);

        let paired = sp
            .runtime
            .block_on(sp.pair_with_invite("  https://relay.example  ", "an-invite", "mac-A"))
            .unwrap();

        assert_eq!(relay.invites(), vec!["an-invite".to_string()], "the token was redeemed");
        assert_eq!(paired.user_id, SCRIPTED_USER_ID);
        assert_eq!(paired.device_id, SCRIPTED_DEVICE_ID);

        let pairings = sp.runtime.block_on(sp.list_pairings()).unwrap();
        assert_eq!(pairings.len(), 1, "one Pairing, kept");
        assert_eq!(pairings[0].label, "mac-A");
        assert_eq!(
            pairings[0].server_url, "https://relay.example",
            "trimmed once, here, and stored trimmed"
        );
        assert!(pairings[0].is_active, "and it is the Active Pairing");

        let keychain_holds = |account: String| {
            sp.keychain.get(&account).unwrap().is_some_and(|v| !v.is_empty())
        };
        assert!(keychain_holds(token_account(SCRIPTED_USER_ID)), "the device token is kept");
        assert!(
            keychain_holds(user_key_account(SCRIPTED_USER_ID)),
            "and a user key this device minted, because an invite claim is the only path that does"
        );
        assert!(
            sink.events().iter().any(|e| matches!(
                e,
                CoreEvent::PairingAdded { user_id, .. } if user_id == SCRIPTED_USER_ID
            )),
            "and the shell was told"
        );
        sp.stop_all_sessions();
    }

    /*
     * The claimer's half of a short code, all the way through, with no relay in
     * reach — the other operation whose success path had no test of its own.
     *
     * The user key comes out of the payload the inviter sealed to the pairing
     * secret rather than being minted here: that is the whole difference between
     * the two doors, and it is why a device paired this way can read the other
     * one's entries at all.
     */
    #[test]
    fn pairing_with_a_code_takes_the_user_key_out_of_the_payload() {
        let relay = ScriptedRelay::new(Vec::new(), Vec::new());
        let (sp, sink) = unpaired_over(&relay);
        let secret = [3u8; 32];
        let pair_id = uuid::Uuid::parse_str(SCRIPTED_PAIR_ID).unwrap();
        let code = crate::pairing::shortcode::encode(&crate::pairing::shortcode::ShortcodePayload {
            server_url: SCRIPTED_RELAY_URL.into(),
            pair_id,
            pairing_secret: secret,
        })
        .unwrap();

        let paired = sp
            .runtime
            .block_on(sp.pair_with_code(&code, "mac-B"))
            .unwrap();

        assert_eq!(
            relay.pair_claims(),
            vec![pair_id.to_string(), pair_id.to_string()],
            "the secret is proved once to claim the slot and again to become a device"
        );
        assert_eq!(relay.joined(), vec!["mac-B".to_string()]);
        assert_eq!(paired.user_id, SCRIPTED_USER_ID);
        assert_eq!(paired.device_id, SCRIPTED_DEVICE_ID);

        let pairings = sp.runtime.block_on(sp.list_pairings()).unwrap();
        assert_eq!(pairings.len(), 1);
        assert_eq!(
            pairings[0].server_url, SCRIPTED_RELAY_URL,
            "the relay's address as the inviter knows it, which is what travelled in the code"
        );
        assert!(pairings[0].is_active);
        assert_eq!(
            sp.keychain.get(&user_key_account(SCRIPTED_USER_ID)).unwrap().as_deref(),
            Some(TEST_USER_KEY_HEX),
            "the key came out of the payload, so both devices can read one History"
        );
        assert_eq!(
            sp.keychain.get(&token_account(SCRIPTED_USER_ID)).unwrap().as_deref(),
            Some("scripted-token")
        );
        assert!(
            sink.events().iter().any(|e| matches!(
                e,
                CoreEvent::PairingAdded { user_id, .. } if user_id == SCRIPTED_USER_ID
            )),
            "and the shell was told"
        );
        sp.stop_all_sessions();
    }

    /*
     * Recall Latest always makes the round trip. Nothing is cached here, so a
     * fetch is the only way to answer at all — and the relay having answered is
     * what makes `Relay` the source.
     */
    #[test]
    fn recall_latest_fetches_and_reports_the_relay_as_the_source() {
        let relay = ScriptedRelay::new(
            vec![Ok(vec![encrypted_row(7, "u", "the newest", "d")])],
            Vec::new(),
        );
        let Rig { sp, clipboard, .. } = rig_over(&relay);

        let out = sp
            .runtime
            .block_on(sp.recall_latest("u"))
            .unwrap();

        assert_eq!(out.text, "the newest");
        assert_eq!(
            out.entry_id, 1,
            "the id is this device's own; the relay's 7 does not cross the facade"
        );
        assert_eq!(out.source, RecallSource::Relay);
        assert_eq!(relay.asked_since(), vec![0], "and it really did ask");
        assert_eq!(clipboard.writes(), vec!["the newest".to_string()]);
    }

    /*
     * The one that matters. The cache already holds an entry and the relay holds
     * a newer one it has never seen, so an implementation that read the cache
     * first — or that short-circuited when the cache was non-empty — would hand
     * over yesterday's link. That is the exact failure this operation exists to
     * prevent.
     */
    #[test]
    fn recall_latest_returns_an_entry_the_cache_has_never_seen() {
        let relay = ScriptedRelay::new(
            vec![Ok(vec![encrypted_row(2, "u", "today's link", "d")])],
            Vec::new(),
        );
        let Rig { sp, clipboard, .. } = rig_over(&relay);
        // Explicitly older, rather than older by construction order: the row the
        // relay is holding is stamped from the clock when the fixture is built,
        // and "yesterday" is the fact this test is about.
        seed_entry_at(&sp, 1, "yesterday's link", crate::now_ms() - 60_000);

        let out = sp
            .runtime
            .block_on(sp.recall_latest("u"))
            .unwrap();

        assert_eq!(out.text, "today's link", "it must not have read the cache");
        assert_eq!(out.entry_id, 2);
        assert_eq!(out.source, RecallSource::Relay);
        assert_eq!(clipboard.writes(), vec!["today's link".to_string()]);
    }

    /*
     * No network: the newest cached entry is still the best answer available, but
     * the caller has to be told which one it got. A shell that cannot tell the
     * difference hands over yesterday's link silently.
     */
    #[test]
    fn recall_latest_falls_back_to_the_cache_and_says_so() {
        let relay = ScriptedRelay::new(vec![Err(AppError::Network("no route".into()))], Vec::new());
        let Rig { sp, clipboard, .. } = rig_over(&relay);
        seed_entry(&sp, 1, "the only one cached");

        let out = sp
            .runtime
            .block_on(sp.recall_latest("u"))
            .unwrap();

        assert_eq!(out.text, "the only one cached");
        assert_eq!(out.entry_id, 1);
        assert_eq!(out.source, RecallSource::Cache);
        assert_eq!(clipboard.writes(), vec!["the only one cached".to_string()]);
    }

    /*
     * A fetch that finds nothing new is still authoritative: the relay has
     * confirmed nothing newer exists, which is a different fact from not having
     * asked. Reporting `Cache` here would train a shell to warn about staleness
     * every time nothing had changed.
     */
    #[test]
    fn a_fetch_that_finds_nothing_new_is_still_the_relays_answer() {
        let relay = ScriptedRelay::new(vec![Ok(Vec::new())], Vec::new());
        let Rig { sp, .. } = rig_over(&relay);
        seed_entry(&sp, 1, "already had it");

        let out = sp
            .runtime
            .block_on(sp.recall_latest("u"))
            .unwrap();

        assert_eq!(out.text, "already had it");
        assert_eq!(out.source, RecallSource::Relay);
    }

    #[test]
    fn recall_latest_with_nothing_anywhere_is_not_found() {
        let relay = ScriptedRelay::new(vec![Ok(Vec::new())], Vec::new());
        let Rig { sp, .. } = rig_over(&relay);
        // `Recalled` has no `Debug` on purpose, so match rather than `unwrap_err`.
        let Err(err) = sp.runtime.block_on(sp.recall_latest("u")) else {
            panic!("a recall with nothing on the relay and nothing cached cannot succeed");
        };
        assert!(matches!(err, AppError::NotFound(_)), "got {err:?}");
    }

    /*
     * An Offered Capture is honoured regardless of the capture setting: refusing
     * content someone just chose to hand over is indefensible, and
     * `capture_enabled` governs Watched Capture, which a phone never performs.
     */
    #[test]
    fn an_offer_is_honoured_with_capture_disabled() {
        let Rig { sp, sink, .. } = rig();
        sp.runtime
            .block_on(sp.update_settings(SettingsPatch {
                capture_enabled: Some(false),
                ..Default::default()
            }))
            .unwrap();

        let out = sp
            .runtime
            .block_on(sp.offer("u", "handed over deliberately"))
            .unwrap();

        assert_eq!(out, OfferOutcome::Queued { pending: 1 });
        assert_eq!(sink.pending_counts(), vec![1], "and the queue depth is reported");
    }

    /*
     * The rules divide. Over-size and non-text are properties of the text and
     * still reject; the deny-list is matched against a frontmost application, of
     * which an Offered Capture has none — so it cannot refuse one. A repeat is
     * no longer a refusal at all: see `an_offer_of_something_already_pending_...`.
     */
    #[test]
    fn an_offer_rejects_over_size_and_non_text_but_never_the_deny_list() {
        let Rig { sp, .. } = rig();
        sp.runtime
            .block_on(sp.update_settings(SettingsPatch {
                deny_list: Some(vec!["com.1password.1password".into()]),
                ..Default::default()
            }))
            .unwrap();
        let offer = |text: &str| sp.runtime.block_on(sp.offer("u", text)).unwrap();

        assert_eq!(
            offer("straight out of a password manager"),
            OfferOutcome::Queued { pending: 1 },
            "the deny-list must not reject an Offered Capture"
        );
        assert_eq!(offer(""), OfferOutcome::Rejected(SkipReason::NonText));
        assert_eq!(
            offer(&"a".repeat(MAX_BYTES + 1)),
            OfferOutcome::Rejected(SkipReason::TooLarge)
        );
    }

    /*
     * A repeat copy of something still queued has no relay id to use, so the act
     * is carried by the queue's order: the pending moves to the back rather than
     * a second identical row being created.
     */
    #[test]
    fn an_offer_of_something_already_pending_moves_it_rather_than_queueing_a_second() {
        let Rig { sp, sink, .. } = rig();
        let offer = |text: &str| sp.runtime.block_on(sp.offer("u", text)).unwrap();

        assert_eq!(offer("ssh admin@10.0.0.4"), OfferOutcome::Queued { pending: 1 });
        assert_eq!(offer("a second thing"), OfferOutcome::Queued { pending: 2 });
        assert_eq!(
            offer("ssh admin@10.0.0.4"),
            OfferOutcome::Recognised { pending: 2 },
            "recognised, and the queue is no deeper for it"
        );

        assert_eq!(
            queued_texts(&sp),
            vec!["a second thing", "ssh admin@10.0.0.4"],
            "the act moved to the back rather than a second being queued beside it"
        );
        assert_eq!(
            sink.pending_counts(),
            vec![1, 2],
            "a move is not a queue depth change and must not be reported as one"
        );
    }

    /*
     * Exact bytes. A trailing newline makes it a different entry, because a
     * Recall hands back the *stored* text and in a shell that newline is the
     * difference between a command that runs and one that waits. See ADR 0012.
     */
    #[test]
    fn a_trailing_newline_is_a_different_capture() {
        let Rig { sp, .. } = rig();
        let offer = |text: &str| sp.runtime.block_on(sp.offer("u", text)).unwrap();
        assert_eq!(offer("https://example.test"), OfferOutcome::Queued { pending: 1 });
        assert_eq!(offer("https://example.test\n"), OfferOutcome::Queued { pending: 2 });
    }

    /*
     * ADR 0016, at the seam a shell sees. An Entry exists from the moment of
     * capture: it is a row in the History with its preview, its plaintext and
     * this device as its Origin, before any relay has been reached — the whole
     * reason the local history stopped lying about what the device holds.
     *
     * `EntryAdded` fires here, exactly once. Nothing about the later flush
     * creates an Entry, so nothing about it announces one.
     */
    #[test]
    fn an_offline_capture_is_in_the_history_before_any_relay_is_reached() {
        let Rig { sp, sink, .. } = rig();
        assert_eq!(
            sp.runtime.block_on(sp.offer("u", "copied with no relay in reach")).unwrap(),
            OfferOutcome::Queued { pending: 1 }
        );

        let history = sp.runtime.block_on(sp.list_history("u", None, 50)).unwrap();
        assert_eq!(history.len(), 1, "the capture is a row, not just a queued act");
        assert_eq!(history[0].preview, "copied with no relay in reach");
        assert_eq!(history[0].plaintext.as_deref(), Some("copied with no relay in reach"));
        assert!(!history[0].undecryptable);
        assert_eq!(history[0].device_id, "d", "this device is the Origin");
        assert_eq!(
            sp.runtime.block_on(sp.read_entry("u", history[0].id)).unwrap().as_deref(),
            Some("copied with no relay in reach"),
            "and it reads by the id the row carries"
        );

        let announced = sink.entries();
        assert_eq!(announced.len(), 1, "one capture is one EntryAdded, at capture");
        assert_eq!(announced[0].id, history[0].id, "under the same id the list uses");
        assert_eq!(announced[0].preview, "copied with no relay in reach");

        // The act is queued against that row, which is what lets the flush find
        // it again and what lets it be withdrawn.
        let queued = sp.runtime.block_on(async {
            let conn = sp.state.conn.lock().await;
            history::next_act(&conn, "u").unwrap().unwrap()
        });
        assert_eq!(queued.entry_id, Some(history[0].id));
    }

    /*
     * A repeat copy of something still queued is the same act as copying it
     * (ADR 0012), and that has to keep holding now the un-flushed capture is
     * also a row: the row has no relay id to record a use against, so the queue
     * carries it. One row, one act, moved rather than multiplied.
     */
    #[test]
    fn re_copying_an_un_flushed_capture_moves_its_act_and_adds_no_row() {
        let Rig { sp, .. } = rig();
        sp.runtime.block_on(sp.offer("u", "first")).unwrap();
        sp.runtime.block_on(sp.offer("u", "second")).unwrap();
        assert_eq!(
            sp.runtime.block_on(sp.offer("u", "first")).unwrap(),
            OfferOutcome::Recognised { pending: 2 },
            "recognised, and the queue is no deeper"
        );
        assert_eq!(
            sp.runtime.block_on(sp.list_history("u", None, 50)).unwrap().len(),
            2,
            "and no second row for the same text"
        );
        assert_eq!(
            queued_texts(&sp),
            vec!["second", "first"],
            "`second` is at the head now: re-copying `first` sent its act to the back"
        );
    }

    /*
     * Making a mistaken copy visible without making it withdrawable would show
     * you the mistake and hand you nothing. The queue is durable across a
     * force-quit, so this is the only thing that stops an un-flushed capture
     * reaching the relay when it comes back (ADR 0016) — and it is the one delete
     * that works with nothing in reach: no other device knows of the row, so
     * there is nothing out there to take back.
     */
    #[test]
    fn deleting_an_un_flushed_capture_withdraws_its_act_with_no_relay_in_reach() {
        let Rig { sp, sink, .. } = rig();
        sp.runtime.block_on(sp.offer("u", "a mistake")).unwrap();
        sp.runtime.block_on(sp.offer("u", "keep this")).unwrap();
        let history = sp.runtime.block_on(sp.list_history("u", None, 50)).unwrap();
        let mistake = history
            .iter()
            .find(|e| e.preview == "a mistake")
            .expect("the capture is a row")
            .id;

        // `rig()` points at a refused port. A delete that reached for the relay
        // would fail here, which is the whole point.
        sp.runtime.block_on(sp.delete_entry("u", mistake)).unwrap();

        let left = sp.runtime.block_on(sp.list_history("u", None, 50)).unwrap();
        assert_eq!(
            left.iter().map(|e| e.preview.as_str()).collect::<Vec<_>>(),
            vec!["keep this"],
            "the row is gone and the other one is not"
        );
        assert_eq!(
            queued_texts(&sp),
            vec!["keep this"],
            "the act went with the row, and what is left is the one nobody withdrew"
        );
        assert!(
            sink.pending_counts().ends_with(&[1]),
            "the count chrome is the only thing that says the queue shrank: {:?}",
            sink.pending_counts()
        );
        assert!(
            sink.events()
                .iter()
                .any(|e| matches!(e, CoreEvent::EntryDeleted { entry_id, .. } if *entry_id == mistake)),
            "and the row's own id is what the shells are told to drop"
        );
    }

    /*
     * ADR 0012's rule, reached by a Recall rather than by a re-copy: an un-flushed
     * capture has no relay id to record a use against, so the act it already owes
     * moves to the back of the queue. One act moved, not two acts queued — and it
     * is the head of the History afterwards, which is the whole point of the move.
     */
    #[test]
    fn recalling_an_un_flushed_capture_moves_its_act_and_leads_the_history() {
        let relay = ScriptedRelay::new(Vec::new(), Vec::new());
        let Rig { sp, sink, .. } = rig_over(&relay);
        sp.runtime.block_on(sp.offer("u", "captured first")).unwrap();
        sp.runtime.block_on(sp.offer("u", "captured second")).unwrap();
        let first = history_ids(&sp)[1];

        sp.runtime.block_on(sp.recall("u", first)).unwrap();

        // `recall` spawns the use, so a test waits for the effect exactly as a
        // shell does — there is no second body that awaits it.
        until(|| history_ids(&sp)[0] == first);
        assert_eq!(
            queued_texts(&sp),
            vec!["captured second", "captured first"],
            "one act moved and nothing was queued beside it: the recalled act is \
             at the back now, which is the head of the History"
        );
        assert!(
            relay.uses().is_empty(),
            "there is no relay record to move, so no use was recorded"
        );
        assert!(
            sink.saw_history_changed("u"),
            "the list reordered, and a shell has to be told"
        );
    }

    /*
     * Capture A, capture B, recall A: the flush order is B then A, so A ends up
     * above B on the relay — exactly where this device was already showing it.
     */
    #[test]
    fn a_recall_during_an_outage_reaches_the_relay_last() {
        let relay = ScriptedRelay::new(Vec::new(), Vec::new());
        let Rig { sp, .. } = rig_over(&relay);
        sp.runtime.block_on(sp.offer("u", "A")).unwrap();
        sp.runtime.block_on(sp.offer("u", "B")).unwrap();
        let a = history_ids(&sp)[1];
        sp.runtime.block_on(sp.recall("u", a)).unwrap();
        until(|| queued_texts(&sp) == vec!["B".to_string(), "A".to_string()]);

        assert_eq!(
            queued_texts(&sp),
            vec!["B".to_string(), "A".to_string()],
            "the order the relay will be told in"
        );
    }

    /*
     * Recall Latest with acts still queued. A local act and a remote entry cannot
     * be compared while one of them has no stamp, so the queue is emptied first
     * and the relay's clock decides — which is how every device gives the same
     * answer.
     */
    #[test]
    fn recall_latest_drains_the_queue_before_it_reads_the_head() {
        let relay = ScriptedRelay::new(vec![Ok(Vec::new())], Vec::new());
        let Rig { sp, clipboard, .. } = rig_over(&relay);
        sp.runtime.block_on(sp.offer("u", "queued while offline")).unwrap();

        let out = sp.runtime.block_on(sp.recall_latest("u")).unwrap();

        assert_eq!(relay.uploaded().len(), 1, "the queue went to the relay first");
        assert_eq!(out.text, "queued while offline");
        assert_eq!(
            out.source,
            RecallSource::Relay,
            "the round trip succeeded, so the answer is one every device would give"
        );
        assert_eq!(clipboard.writes(), vec!["queued while offline".to_string()]);
        let left = sp.runtime.block_on(async {
            let conn = sp.state.conn.lock().await;
            history::depth(&conn, "u").unwrap()
        });
        assert_eq!(left, 0, "and the queue is empty");
    }

    /*
     * The same with the relay out of reach: the local head is still the best
     * answer available, and `Cache` is what says nobody else agrees with it.
     */
    #[test]
    fn recall_latest_with_a_queue_and_no_relay_falls_back_to_the_local_head() {
        let relay = ScriptedRelay::new(
            vec![Err(AppError::Network("no route".into()))],
            Vec::new(),
        )
        .answering_uploads(crate::testing::UploadAnswer::Unreachable);
        let Rig { sp, clipboard, .. } = rig_over(&relay);
        sp.runtime.block_on(sp.offer("u", "still owed to the relay")).unwrap();

        let out = sp.runtime.block_on(sp.recall_latest("u")).unwrap();

        assert_eq!(out.text, "still owed to the relay");
        assert_eq!(out.source, RecallSource::Cache);
        assert_eq!(clipboard.writes(), vec!["still owed to the relay".to_string()]);
        let left = sp.runtime.block_on(async {
            let conn = sp.state.conn.lock().await;
            history::depth(&conn, "u").unwrap()
        });
        assert_eq!(left, 1, "nothing was lost trying");
    }

    /*
     * A drain that outlasts its bound falls back rather than hanging the verb. The
     * relay here accepts the connection and never answers, which is the shape the
     * bound exists for — a refused port fails fast and would not exercise it.
     */
    #[test]
    fn a_drain_that_exceeds_its_bound_falls_back_rather_than_hanging() {
        let relay = ScriptedRelay::new(vec![Ok(Vec::new())], Vec::new())
            .answering_uploads(crate::testing::UploadAnswer::Stall);
        let Rig { sp, clipboard, .. } = rig_over(&relay);
        sp.runtime.block_on(sp.offer("u", "behind a silent relay")).unwrap();

        let out = sp.runtime.block_on(sp.recall_latest("u")).unwrap();

        assert_eq!(out.text, "behind a silent relay");
        assert_eq!(out.source, RecallSource::Cache, "the bound was reached, so it is ours");
        assert_eq!(clipboard.writes(), vec!["behind a silent relay".to_string()]);
    }

    /*
     * Resend, end to end. A fresh act and not a retry: it carries nothing forward
     * from the refusal, leads the History afterwards, and flushes if the relay
     * will now take it.
     */
    #[test]
    fn resending_a_refused_act_clears_the_refusal_and_leads_the_history() {
        let Rig { sp, sink, .. } = rig();
        sp.runtime.block_on(sp.offer("u", "the relay would not take this")).unwrap();
        sp.runtime.block_on(sp.offer("u", "captured after it")).unwrap();
        let refused = history_ids(&sp)[1];
        sp.runtime.block_on(async {
            let conn = sp.state.conn.lock().await;
            let act = history::next_act(&conn, "u").unwrap().unwrap();
            assert_eq!(act.entry_id, Some(refused), "the oldest act is the head");
            history::record_failure(&conn, &act, "an earlier attempt").unwrap();
            history::refuse(&conn, "u", &act, 1, "payload too large").unwrap();
        });

        // A refusal leads the History, above every act still on its way.
        let before = sp.runtime.block_on(sp.list_history("u", None, 50)).unwrap();
        assert_eq!(before[0].id, refused);
        assert_eq!(before[0].refused_reason.as_deref(), Some("payload too large"));
        assert!(before[0].pending, "the act is still owed");

        sp.runtime.block_on(sp.resend("u", refused)).unwrap();

        let after = sp.runtime.block_on(sp.list_history("u", None, 50)).unwrap();
        assert_eq!(after[0].id, refused, "and still leads it, now as a fresh act");
        assert_eq!(after[0].refused_reason, None, "the reason is gone");
        assert!(after[0].pending);
        let queue = sp.runtime.block_on(async {
            let conn = sp.state.conn.lock().await;
            history::owed(&conn, "u").unwrap()
        });
        assert_eq!(queue.len(), 2, "the act moved; nothing was queued beside it");
        let resent = queue.last().unwrap();
        assert_eq!(
            resent.entry_id,
            Some(refused),
            "and is deliverable again, at the back — which is the head of the History"
        );
        assert_eq!(resent.attempts, 0, "carrying nothing forward from the refusal");
        assert_eq!(resent.last_error, None);
        assert!(
            sink.saw_history_changed("u"),
            "the row left the refused region, which reorders the list"
        );
    }

    /*
     * `clear_history`, whole, with no relay in reach.
     *
     * It used to wipe the cache and leave the queue standing, so the next flush
     * put back exactly what had just been cleared — the rule the second half of
     * this test pins. The first half is what could not be tested at all before
     * the seam moved: the relay really is asked to drop the User's entries, and
     * the operation the shell calls is the one being driven, rather than a
     * private half of it that skipped the relay.
     */
    #[test]
    fn clearing_the_history_asks_the_relay_and_empties_the_queue_with_it() {
        let relay = ScriptedRelay::new(Vec::new(), Vec::new());
        let Rig { sp, sink, .. } = rig_over(&relay);
        sp.runtime.block_on(sp.offer("u", "queued and then cleared")).unwrap();

        sp.runtime.block_on(sp.clear_history("u")).unwrap();

        assert_eq!(relay.cleared(), 1, "the relay was told to drop the History");
        let after = sp.runtime.block_on(async {
            let conn = sp.state.conn.lock().await;
            (
                history::page(&conn, "u", None, 50).unwrap().len(),
                history::depth(&conn, "u").unwrap(),
            )
        });
        assert_eq!(after, (0, 0), "nothing cached and nothing left to repopulate it");
        assert!(
            sink.pending_counts().ends_with(&[0]),
            "and the count says so: {:?}",
            sink.pending_counts()
        );
    }

    /// What is queued, in the order the relay will be told about it — asked of
    /// the module rather than decrypted out of the queue.
    ///
    /// The queue holds ciphertext and the Entry holds the text, and nothing used
    /// to join them, so a test that wanted to know what sat at the head reached
    /// for `testing::test_user_key` and decrypted it.
    fn queued_texts(sp: &Sharepaste) -> Vec<String> {
        sp.runtime.block_on(async {
            let conn = sp.state.conn.lock().await;
            history::owed(&conn, "u")
                .unwrap()
                .into_iter()
                .filter_map(|a| a.entry_id)
                .filter_map(|id| history::plaintext_of(&conn, "u", id).unwrap())
                .collect()
        })
    }

    /// Put one entry in the cache with an explicit age, so a use stamped from
    /// the clock can be shown to move it. Answers this device's id for it.
    fn seed_entry_at(sp: &Sharepaste, id: i64, plaintext: &str, at: i64) -> i64 {
        let mut row = encrypted_row(id, "u", plaintext, "d");
        row.created_at = at;
        row.last_use = at;
        sp.runtime.block_on(async {
            let conn = sp.state.conn.lock().await;
            crate::sync::decryptor::ingest(
                &conn,
                &crate::testing::test_user_key(),
                "u",
                &row,
                crate::now_ms(),
            )
            .unwrap()
            .stored
            .local_id
        })
    }

    fn history_ids(sp: &Sharepaste) -> Vec<i64> {
        sp.runtime
            .block_on(sp.list_history("u", None, 100))
            .unwrap()
            .iter()
            .map(|e| e.id)
            .collect()
    }

    /*
     * The decision, end to end on one device: a Recall of a buried entry puts it
     * at the head, tells the relay, and leaves the entry's identity alone. The
     * relay's answer is applied to the local row directly — no SSE frame is
     * delivered anywhere in this test.
     */
    #[test]
    fn a_recall_moves_the_entry_to_the_head_and_records_the_use() {
        let relay = ScriptedRelay::new(Vec::new(), Vec::new());
        let Rig { sp, sink, clipboard } = rig_over(&relay);
        let long_ago = crate::now_ms() - 3 * 24 * 60 * 60 * 1000;
        seed_entry_at(&sp, 1, "ssh admin@10.0.0.4", long_ago);
        seed_entry_at(&sp, 2, "captured since", crate::now_ms() - 1000);
        assert_eq!(history_ids(&sp), vec![2, 1], "precondition: 1 is buried");

        sp.runtime.block_on(sp.recall("u", 1)).unwrap();

        assert_eq!(clipboard.writes(), vec!["ssh admin@10.0.0.4".to_string()]);
        // The use is spawned, so the effect is what a test waits on.
        until(|| relay.uses() == vec![1]);
        until(|| history_ids(&sp) == vec![1, 2]);
        assert!(sink.saw_history_changed("u"), "and this device did not wait to be told back");

        let recalled = sp.runtime.block_on(sp.list_history("u", None, 1)).unwrap();
        assert_eq!(recalled[0].created_at, long_ago, "a use leaves identity alone");
        assert!(recalled[0].last_use > long_ago);
    }

    /*
     * Recalling the head is still a use. It cannot move the order; it renews
     * tenure, and an exception here would let a daily-recalled entry age out at
     * thirty days.
     */
    #[test]
    fn recalling_the_entry_already_on_top_is_still_a_use() {
        let relay = ScriptedRelay::new(Vec::new(), Vec::new());
        let Rig { sp, .. } = rig_over(&relay);
        seed_entry_at(&sp, 1, "the only one", crate::now_ms() - 1000);
        sp.runtime.block_on(sp.recall("u", 1)).unwrap();
        until(|| relay.uses() == vec![1]);
    }

    /*
     * Skew is not handled: a relay without the route answers exactly as a relay
     * without the entry does, and neither earns a queued retry that could never
     * succeed. The Recall itself never needed the relay and still stands.
     */
    #[test]
    fn a_relay_that_will_not_record_the_use_neither_fails_nor_queues_it() {
        let relay =
            ScriptedRelay::new(Vec::new(), Vec::new()).answering_uses(crate::testing::UseAnswer::Gone);
        let Rig { sp, clipboard, .. } = rig_over(&relay);
        seed_entry_at(&sp, 1, "recalled anyway", crate::now_ms() - 1000);

        sp.runtime.block_on(sp.recall("u", 1)).unwrap();

        assert_eq!(clipboard.writes(), vec!["recalled anyway".to_string()]);
        until(|| relay.uses() == vec![1]);
        let conn = sp.runtime.block_on(sp.state.conn.lock());
        assert_eq!(history::depth(&conn, "u").unwrap(), 0, "a 404 is not worth retrying");
    }

    /*
     * An outage does not lose the use. It queues beside the captures and reaches
     * the relay in the order it was made.
     */
    #[test]
    fn a_use_the_relay_cannot_be_told_about_is_queued() {
        let relay = ScriptedRelay::new(Vec::new(), Vec::new())
            .answering_uses(crate::testing::UseAnswer::Unreachable);
        let Rig { sp, sink, .. } = rig_over(&relay);
        seed_entry_at(&sp, 1, "recalled offline", crate::now_ms() - 1000);

        sp.runtime.block_on(sp.recall("u", 1)).unwrap();

        until(|| sink.pending_counts() == vec![1]);
        let conn = sp.runtime.block_on(sp.state.conn.lock());
        let head = history::next_act(&conn, "u").unwrap().unwrap();
        assert_eq!(head.kind, history::ActKind::Use(1));
    }

    /*
     * A Recall that never reached the clipboard put nothing anywhere, and
     * nothing is not a use of anything.
     */
    #[test]
    fn a_recall_whose_clipboard_write_fails_records_no_use() {
        let relay = ScriptedRelay::new(Vec::new(), Vec::new());
        let Rig { sp, clipboard, .. } = rig_over(&relay);
        seed_entry_at(&sp, 1, "never arrived", crate::now_ms() - 1000);
        clipboard.on_write(|_| Err(AppError::Storage("clipboard failed".into())));

        let err = sp.runtime.block_on(sp.recall("u", 1)).unwrap_err();

        assert!(matches!(err, AppError::Storage(_)));
        assert!(
            relay.uses().is_empty(),
            "and nothing was spawned to record one, so there is nothing to wait for"
        );
    }

    /*
     * ADR 0012's half: text this device already holds is a use of the entry that
     * is there, not a second identical row. Nothing is encrypted and nothing is
     * queued as a capture — the queued act here is the *use*, because this rig's
     * relay refuses instantly.
     */
    #[test]
    fn offering_text_already_in_the_history_is_a_use_and_not_a_capture() {
        let Rig { sp, sink, .. } = rig();
        let long_ago = crate::now_ms() - 3 * 24 * 60 * 60 * 1000;
        seed_entry_at(&sp, 1, "ssh admin@10.0.0.4", long_ago);
        seed_entry_at(&sp, 2, "captured since", crate::now_ms() - 1000);

        let out = sp
            .runtime
            .block_on(sp.offer("u", "ssh admin@10.0.0.4"))
            .unwrap();

        assert_eq!(
            out,
            OfferOutcome::Recognised { pending: 1 },
            "nothing was encrypted and nothing was queued as a capture; the one \
             queued act is the use, which this rig's relay could not be told about"
        );
        assert_eq!(
            sink.pending_counts(),
            vec![1],
            "and the depth the caller was handed is the one the sink reports"
        );
        let conn = sp.runtime.block_on(sp.state.conn.lock());
        let head = history::next_act(&conn, "u").unwrap().unwrap();
        assert_eq!(
            head.kind,
            history::ActKind::Use(1),
            "the one thing queued is the use, not a duplicate capture"
        );
        assert_eq!(history::depth(&conn, "u").unwrap(), 1);
    }

    /*
     * An Undecryptable entry has no plaintext and so no hash: it must never
     * swallow a copy of text this device cannot prove it holds.
     */
    #[test]
    fn an_undecryptable_entry_never_recognises_a_copy() {
        let Rig { sp, .. } = rig();
        seed_entry_at(&sp, 1, "readable", crate::now_ms() - 1000);
        sp.runtime.block_on(async {
            let conn = sp.state.conn.lock().await;
            history::mark_undecryptable(&conn, "u", 1).unwrap();
        });

        assert_eq!(
            sp.runtime.block_on(sp.offer("u", "readable")).unwrap(),
            OfferOutcome::Queued { pending: 1 }
        );
    }

    /*
     * The other side of that rule, so the split is pinned from both ends: the
     * setting that cannot refuse an Offered Capture is exactly the one that refuses a
     * Watched Capture.
     */
    #[test]
    fn watched_capture_is_what_the_capture_setting_governs() {
        let Rig { sp, .. } = rig();
        sp.runtime
            .block_on(sp.update_settings(SettingsPatch {
                capture_enabled: Some(false),
                ..Default::default()
            }))
            .unwrap();
        let board = FakePasteboard::holding(&["public.utf8-plain-text"], Some("swept up"));

        assert_eq!(
            sp.runtime
                .block_on(sp.capture_watched("u", &board, None))
                .unwrap(),
            OfferOutcome::Rejected(SkipReason::Disabled)
        );
        assert_eq!(
            sp.runtime.block_on(sp.offer("u", "swept up")).unwrap(),
            OfferOutcome::Queued { pending: 1 },
            "while the very same text, handed over deliberately, is taken"
        );
    }

    /*
     * A Watched Capture *is* attributable to a frontmost application, so there
     * the deny-list does its job.
     */
    #[test]
    fn watched_capture_still_honours_the_deny_list() {
        let Rig { sp, .. } = rig();
        sp.runtime
            .block_on(sp.update_settings(SettingsPatch {
                deny_list: Some(vec!["com.1Password.1Password".into()]),
                ..Default::default()
            }))
            .unwrap();
        let board = FakePasteboard::holding(&[], Some("a password"));

        assert_eq!(
            sp.runtime
                .block_on(sp.capture_watched("u", &board, Some("com.1password.1password")))
                .unwrap(),
            OfferOutcome::Rejected(SkipReason::DenyList)
        );
    }

    /// One settled row of the History, as the module hands it out. Built through
    /// it rather than by hand: the place a row holds in the order is the
    /// module's own, and a fixture that invented one would be testing nothing.
    fn cached(id: i64, device_id: &str) -> history::CachedEntry {
        let c = crate::storage::open_in_memory().unwrap();
        let text = format!("entry {id}");
        history::store(
            &c,
            history::RelayEntry {
                user_id: "u",
                relay_id: id,
                ciphertext: b"ct",
                plaintext: Some(&text),
                created_at: 1_000 + id,
                last_use: 1_000 + id,
                device_id,
            },
            1_000 + id,
        )
        .unwrap();
        history::page(&c, "u", None, 1).unwrap().remove(0)
    }

    #[test]
    fn list_history_labels_origins_from_the_device_mirror() {
        let labels = HashMap::from([("d1".to_string(), "IPHONE-15".to_string())]);
        let entries = to_entries(vec![cached(1, "d1")], &labels);
        assert_eq!(entries[0].device_label.as_deref(), Some("IPHONE-15"));
        assert_eq!(entries[0].device_id, "d1");
        assert_eq!(entries[0].preview, "entry 1");
    }

    #[test]
    fn list_history_tolerates_a_device_id_the_mirror_has_never_seen() {
        let labels = HashMap::from([("d1".to_string(), "IPHONE-15".to_string())]);
        let entries = to_entries(vec![cached(1, "d1"), cached(2, "unpaired-yesterday")], &labels);
        assert_eq!(entries[0].device_label.as_deref(), Some("IPHONE-15"));
        assert_eq!(entries[1].device_label, None, "unmirrored origin must not fail the row");
        assert_eq!(entries[1].device_id, "unpaired-yesterday");
    }

    /*
     * The flag, and why it cannot be the empty preview: the row that will not
     * decrypt produces one, and so does an entry whose plaintext is genuinely
     * empty.
     */
    #[test]
    fn list_history_flags_an_undecryptable_entry_rather_than_leaving_it_to_be_inferred() {
        let mut row = cached(1, "d1");
        row.plaintext = None;
        let entries = to_entries(vec![row], &HashMap::new());
        assert!(entries[0].undecryptable);
        assert_eq!(entries[0].preview, "");

        let mut empty = cached(2, "d1");
        empty.plaintext = Some(String::new());
        let entries = to_entries(vec![empty], &HashMap::new());
        assert!(
            !entries[0].undecryptable,
            "an empty plaintext is decryptable, which is exactly why the preview cannot be the signal"
        );
    }

    /*
     * And the same flag on the event, so no shell re-derives it on the live path
     * either. This entry was encrypted for somebody else's User, so the AAD does
     * not match and it arrives readable to nobody here.
     */
    #[test]
    fn an_entry_that_will_not_decrypt_arrives_flagged() {
        let relay = ScriptedRelay::new(
            vec![Ok(Vec::new())],
            vec![Wire::Holds(vec![live_entry(9, "somebody-else", "unreadable here", "d")])],
        );
        let Rig { sp, sink, .. } = rig_over(&relay);
        sp.runtime.block_on(sp.start_session("u")).unwrap();

        until(|| !sink.entries().is_empty());
        let entry = sink.entries().remove(0);
        assert_eq!(entry.id, 1, "this device's id for the row, not the relay's 9");
        assert!(entry.undecryptable, "explicit on the event, not inferred from the preview");
        assert_eq!(entry.preview, "");
        sp.stop_session("u");
    }

    /*
     * The defect this pair of fields exists to close.
     *
     * `preview` meant the Preview on the event and the whole plaintext out of
     * the history query, so a shell could not read one field and be right. Both
     * paths are exercised on one Entry here, deliberately: a test that pinned
     * only the query would have passed for as long as the bug existed.
     */
    #[test]
    fn preview_is_the_one_line_rendering_and_plaintext_is_whole_on_both_paths() {
        let indented = "\n\tssh admin@10.0.0.4\n  -i ~/.ssh/id_ed25519\n  -p 2222\n";
        let flattened = " ssh admin@10.0.0.4   -i ~/.ssh/id_ed25519   -p 2222";

        let relay = ScriptedRelay::new(
            vec![Ok(Vec::new())],
            vec![Wire::Holds(vec![live_entry(4, "u", indented, "d1")])],
        );
        let Rig { sp, sink, .. } = rig_over(&relay);
        sp.runtime.block_on(sp.start_session("u")).unwrap();
        until(|| !sink.entries().is_empty());

        let on_the_event = sink.entries().remove(0);
        assert_eq!(
            on_the_event.preview,
            flattened.trim(),
            "the row would be blank if the leading newline and tab survived"
        );
        assert_eq!(
            on_the_event.plaintext.as_deref(),
            Some(indented),
            "the reader pane and the search read this, and both need the third line"
        );

        let from_the_query = sp
            .runtime
            .block_on(sp.list_history("u", None, 10))
            .unwrap();
        sp.stop_session("u");

        let from_cache = from_the_query.first().expect("the live frame was cached");
        assert_eq!(from_cache.id, on_the_event.id, "one Entry, one id, both paths");
        assert_eq!(
            from_cache.preview, on_the_event.preview,
            "one Entry, two paths, one Preview — the whole point of the split"
        );
        assert_eq!(from_cache.plaintext.as_deref(), Some(indented));
    }

    /*
     * Undecryptable on the query path: no plaintext at all, rather than an
     * empty string a reader pane would render as a blank document.
     */
    #[test]
    fn an_undecryptable_entry_carries_no_plaintext() {
        let mut row = cached(1, "d1");
        row.plaintext = None;
        let entries = to_entries(vec![row], &HashMap::new());
        assert_eq!(entries[0].plaintext, None);
        assert_eq!(entries[0].preview, "");

        let mut empty = cached(2, "d1");
        empty.plaintext = Some(String::new());
        let entries = to_entries(vec![empty], &HashMap::new());
        assert_eq!(
            entries[0].plaintext.as_deref(),
            Some(""),
            "an entry whose plaintext is genuinely empty has one, and it is empty"
        );
    }

    #[test]
    fn list_history_resolves_the_origin_label_so_no_shell_has_to() {
        let labels = HashMap::from([("d1".to_string(), "IPHONE-15".to_string())]);
        let entries = to_entries(vec![cached(1, "d1"), cached(2, "abcdef123456")], &labels);
        assert_eq!(entries[0].origin_label, "IPHONE-15");
        assert_eq!(
            entries[1].origin_label, "abcd",
            "an unmirrored Origin falls back here, not in two shells"
        );
    }

    #[test]
    fn a_settings_patch_touches_only_the_fields_it_names() {
        let Rig { sp, .. } = rig();
        let before = sp.runtime.block_on(sp.get_settings()).unwrap();
        assert!(before.hotkey.is_some(), "a fresh profile ships a hotkey");

        let after = sp
            .runtime
            .block_on(sp.update_settings(SettingsPatch {
                capture_enabled: Some(false),
                ..Default::default()
            }))
            .unwrap();
        assert!(!after.capture_enabled);
        assert_eq!(after.hotkey, before.hotkey, "an absent field is left alone");
        assert_eq!(after.update_check_enabled, before.update_check_enabled);

        // The doubly-optional field. `Some(None)` clears the binding and must not
        // read as "absent" — the whole reason the patch is typed this way.
        let cleared = sp
            .runtime
            .block_on(sp.update_settings(SettingsPatch {
                hotkey: Some(None),
                ..Default::default()
            }))
            .unwrap();
        assert_eq!(cleared.hotkey, None);
        assert!(!cleared.capture_enabled, "and the previous patch survived");
        assert_eq!(
            sp.runtime.block_on(sp.get_settings()).unwrap().hotkey,
            None,
            "persisted, not just returned"
        );
    }

    /*
     * The insecure-relay mapping at the seam it exists for: an established
     * pairing whose relay is plain HTTP and does not answer. Nothing rejected the
     * scheme on the way in — the pairing was accepted and stored, which is what
     * keeps a desktop already paired to a cleartext relay working across an
     * upgrade — so the only place the reason can be stated is here, when a request
     * actually fails.
     */
    #[test]
    fn an_unreachable_cleartext_relay_explains_itself() {
        let Rig { sp, .. } = rig_at("http://127.0.0.1:1");
        let local_id = seed_entry(&sp, 1, "something to delete");
        let err = sp.runtime.block_on(sp.delete_entry("u", local_id)).unwrap_err();
        let AppError::InsecureRelay(msg) = err else {
            panic!("expected InsecureRelay, got {err:?}");
        };
        assert!(msg.contains("http://127.0.0.1:1"), "the message names the relay: {msg}");
        assert!(msg.contains("HTTPS"), "and what would fix it: {msg}");

        // The identical failure against a TLS relay stays a transport failure.
        // The variant is about the scheme and nothing else.
        let Rig { sp, .. } = rig_at("https://127.0.0.1:1");
        let err = sp.runtime.block_on(sp.clear_history("u")).unwrap_err();
        assert!(matches!(err, AppError::Network(_)), "got {err:?}");
    }

    /*
     * Every operation a shell can reach, driven over the three fakes: no Tauri,
     * no window system, no relay, and no runtime supplied by the caller. The
     * point is the reachability, not any one assertion.
     */
    #[test]
    fn every_operation_is_callable_over_the_fakes_alone() {
        let Rig { sp, sink, clipboard } = rig();
        let rt = &sp.runtime;

        assert_eq!(
            rt.block_on(sp.resume_active_pairing()).unwrap().as_deref(),
            Some("u")
        );
        assert_eq!(sp.active_pairing().as_deref(), Some("u"));

        let pairings = rt.block_on(sp.list_pairings()).unwrap();
        assert_eq!(pairings.len(), 1);
        assert_eq!(pairings[0].user_id, "u");
        assert_eq!(pairings[0].label, "mac");
        assert_eq!(pairings[0].status, ConnectionState::Disconnected);
        assert_eq!(pairings[0].pending, 0);
        assert!(pairings[0].is_active);

        assert_eq!(rt.block_on(sp.get_contact("u")).unwrap().last_contact_at, None);
        let local_id = seed_entry(&sp, 4, "a cached entry");
        let history = rt.block_on(sp.list_history("u", None, 50)).unwrap();
        assert_eq!(history.len(), 1);
        assert!(!history[0].undecryptable);
        // A cursor is a fact about the order rather than about the entry, so it
        // comes off the row the History handed out rather than off `Entry` —
        // see `HistoryCursor`.
        let cursor = rt.block_on(async {
            let conn = sp.state.conn.lock().await;
            history::page(&conn, "u", None, 1).unwrap().remove(0).resume_from
        });
        assert!(rt.block_on(sp.list_history("u", Some(cursor), 50)).unwrap().is_empty());
        assert_eq!(
            rt.block_on(sp.read_entry("u", local_id)).unwrap().as_deref(),
            Some("a cached entry")
        );
        rt.block_on(sp.recall("u", local_id)).unwrap();
        assert_eq!(clipboard.writes(), vec!["a cached entry".to_string()]);

        assert!(matches!(
            rt.block_on(sp.offer("u", "offered")).unwrap(),
            OfferOutcome::Queued { .. }
        ));
        let board = FakePasteboard::holding(&["public.utf8-plain-text"], Some("watched"));
        assert!(matches!(
            rt.block_on(sp.capture_watched("u", &board, Some("com.apple.Terminal")))
                .unwrap(),
            OfferOutcome::Queued { .. }
        ));

        // Both of these reach for the relay, which is what the refused port is
        // for: an operation that tries is a fast failure, not a hang. `local_id`
        // names a row the relay *has* taken, so the delete does try.
        assert!(rt.block_on(sp.delete_entry("u", local_id)).is_err());
        assert!(rt.block_on(sp.clear_history("u")).is_err());

        rt.block_on(sp.set_active_pairing("u")).unwrap();
        rt.block_on(sp.forget_pairing("u")).unwrap();
        assert!(rt.block_on(sp.list_pairings()).unwrap().is_empty());
        assert!(rt.block_on(sp.resume_active_pairing()).unwrap().is_none());
        assert!(
            sink.events()
                .iter()
                .any(|e| matches!(e, CoreEvent::PairingRemoved { user_id } if user_id == "u")),
            "and the shell was told the pairing went away"
        );

        sp.stop_all_sessions();
    }

    /*
     * The one operation this effort took the pairing out of, so the one that could
     * start answering `Ok` to a question about a row that does not exist.
     *
     * Withdrawing a capture the relay has never named needs nothing in reach, and
     * that is the point of it — but "needs no relay" and "asks no questions" are
     * different, and only the second is a bug. Found on an emulator, where the
     * facade's own surface test calls every operation on a user with no pairing and
     * expects each to say so in its own words.
     */
    #[test]
    fn deleting_an_entry_that_is_not_here_says_so_rather_than_succeeding() {
        let Rig { sp, sink, .. } = rig();
        let rt = &sp.runtime;

        let err = rt.block_on(sp.delete_entry("u", 9_999)).unwrap_err();
        assert!(
            matches!(err, AppError::NotFound(_)),
            "a row this device does not hold is not a silent success: {err:?}"
        );
        assert!(
            sink.events().is_empty(),
            "and nothing is announced about a row that was never there"
        );

        // The withdraw itself still needs no relay, which is what the branch above
        // exists for: an unknown *user* is as absent as an unknown row.
        let err = rt.block_on(sp.delete_entry("nobody", 1)).unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "{err:?}");

        // And the withdraw still works with nothing in reach, which is the branch
        // the guard above had to be added without breaking.
        rt.block_on(sp.offer("u", "a copy made offline")).unwrap();
        let row = rt.block_on(sp.list_history("u", None, 50)).unwrap().remove(0);
        assert!(row.pending, "precondition: nothing has flushed it");
        rt.block_on(sp.delete_entry("u", row.id)).unwrap();
        assert!(
            rt.block_on(sp.list_history("u", None, 50)).unwrap().is_empty(),
            "a row that is here is withdrawn, with no relay consulted"
        );
    }

    /*
     * The two pairing operations that take a relay address are callable with
     * nothing listening, and a cleartext one explains itself there too — which is
     * the case a phone hits, since its platform refuses the connection before it
     * leaves the device.
     */
    #[test]
    fn pairing_against_a_dead_cleartext_relay_explains_itself() {
        let Rig { sp, .. } = rig();

        let err = sp
            .runtime
            .block_on(sp.pair_with_invite("  http://127.0.0.1:1  ", "a-token", "phone"))
            .unwrap_err();
        assert!(matches!(err, AppError::InsecureRelay(_)), "got {err:?}");

        // The claimer's side reads its relay out of the code rather than from an
        // argument, so this is the only way to aim it.
        let code = crate::pairing::shortcode::encode(&crate::pairing::shortcode::ShortcodePayload {
            server_url: "http://127.0.0.1:1".into(),
            pair_id: uuid::Uuid::nil(),
            pairing_secret: [3u8; 32],
        })
        .unwrap();
        let err = sp
            .runtime
            .block_on(sp.pair_with_code(&code, "phone"))
            .unwrap_err();
        assert!(matches!(err, AppError::InsecureRelay(_)), "got {err:?}");

        // A code that is not a code at all is bad input, not a network failure.
        let err = sp
            .runtime
            .block_on(sp.pair_with_code("not a real code", "phone"))
            .unwrap_err();
        assert!(matches!(err, AppError::BadInput(_)), "got {err:?}");
    }
}
