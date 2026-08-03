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
use crate::http::{ServerClient, TransportPolicy};
use crate::keychain::{token_account, user_key_account, Keychain};
use crate::pairing::invite::{claim_invite, persist_claimed_pairing};
use crate::pairing::payload::{
    fetch_and_decrypt_pair_payload, secret_proof_hex, start_pair, upload_pair_payload, PairClaim,
    PairTransport,
};
use crate::pairing::registry::PairingRegistry;
use crate::pairing::shortcode::{decode as decode_shortcode, group_for_display};
use crate::platform::{Clipboard, EventSink};
use crate::render;
use crate::storage::settings::Settings;
use crate::storage::{accounts, devices, entries_cache, pending, settings};
use crate::sync::session::{self, ServerSession, SessionCtx, SessionState, SessionTransport};
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
/// `require_https` is the same idea applied to a policy rather than a path. The
/// core has no opinion about relay schemes — see
/// [`ServerClient::new`] — so the shell states one.
pub struct SharepasteConfig {
    pub db_path: PathBuf,
    pub keychain: Arc<dyn Keychain>,
    pub clipboard: Arc<dyn Clipboard>,
    pub events: Arc<dyn EventSink>,
    /// Refuse to build a client for an `http://` relay at all.
    ///
    /// `true` on the mobile shells, `false` on the desktop. It is a shell's
    /// choice rather than the core's default because a desktop already paired to
    /// a cleartext relay has to keep working: baking the answer in here would
    /// break an existing installation on upgrade, with no way for its owner to
    /// recover the pairing.
    pub require_https: bool,
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

/// Where a page of the History resumes from: the `(rank, ord, id)` of the last
/// row of the page before it.
///
/// Three parts and not an id, because the History is two regions in one order
/// (ADR 0014) and the whole of that order is `(rank ASC, ord DESC, id DESC)`.
/// `rank` says which region — refused, pending, settled — `ord` is the place
/// inside it, and `id` keeps the order total when two rows share an `ord`.
/// Keyset paging over the same tuple is what makes crossing the seam between
/// the regions no different from any other page boundary.
///
/// `id` is [`Entry::id`] — this device's own id for the row, not the relay's.
/// The relay's does not cross this facade at all, and a row that has not reached
/// the relay has none to page by.
///
/// `rank` and `ord` are facts about the *order*, not about the entry, so they are
/// deliberately not on [`Entry`]: a row says whether it is pending and why it was
/// refused, and nothing about which of three groups the query put it in. A cursor
/// is therefore built by the core from the cached row, and a shell that pages
/// gets one back rather than assembling it — no shipped shell pages yet, and the
/// one that does will be handed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct HistoryCursor {
    pub rank: i64,
    pub ord: i64,
    pub id: i64,
}

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

/// What this device already holds that matches the text just copied.
enum Held {
    Nothing,
    /// An entry, which can be used.
    Entry(i64),
    /// A queued capture, which cannot: it has no relay id yet.
    Pending(i64),
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
    /// The shell's answer to "may a relay be plain HTTP", enforced at every
    /// [`ServerClient`] this facade builds and at every one the registry builds
    /// for a session.
    transport: TransportPolicy,
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
        let transport = if cfg.require_https {
            TransportPolicy::RequireHttps
        } else {
            TransportPolicy::AllowCleartext
        };
        // The registry builds the client for every *session* request, so the
        // policy has to reach it too — otherwise a pairing stored before the
        // shell required HTTPS would keep syncing over cleartext forever.
        let registry = Arc::new(PairingRegistry::new(conn.clone(), cfg.keychain.clone(), transport));
        Ok(Arc::new(Sharepaste {
            state: SessionState::new(conn, runtime.handle().clone()),
            registry,
            keychain: cfg.keychain,
            clipboard: cfg.clipboard,
            events: cfg.events,
            transport,
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
    pub async fn start_session(&self, user_id: &str) -> Result<(), AppError> {
        let m = self.unlock(user_id).await?;
        // UserKey is Zeroizing<[u8;32]> with no Clone; clone the inner array via
        // a fresh Zeroizing wrapper so the session owns its own key.
        let user_key: UserKey = Zeroizing::new(*m.user_key);
        session::spawn_session(
            self.session_ctx(user_id),
            Arc::new(ServerSession(m.server)),
            user_key,
        );
        Ok(())
    }

    /// The same session, over a transport the caller supplies.
    ///
    /// The seam the loop's tests drive: everything above it is "which pairing,
    /// over what connection", everything below it is the protocol.
    #[cfg(any(test, feature = "testing"))]
    pub async fn start_session_over(
        &self,
        user_id: &str,
        transport: Arc<dyn SessionTransport>,
    ) -> Result<(), AppError> {
        let m = self.unlock(user_id).await?;
        let user_key: UserKey = Zeroizing::new(*m.user_key);
        session::spawn_session(self.session_ctx(user_id), transport, user_key);
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
            let pending = pending::count(&conn, &a.user_id)?;
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
        // reaches the relay perfectly well through `ServerClient` and then fails
        // to on the next launch, when it comes back out of the database.
        let server_url = server_url.trim();
        let server = ServerClient::new(server_url, self.transport)?;
        let mut claimed = claim_invite(&server, token, device_label)
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
        self.pair_start_with(user_id, Arc::new(m.server), user_key)
            .await
    }

    /// The same handshake over a transport the caller supplies — the seam the
    /// upload-before-reveal test drives.
    #[cfg(any(test, feature = "testing"))]
    pub async fn pair_start_over(
        &self,
        user_id: &str,
        transport: Arc<dyn PairTransport>,
    ) -> Result<ShortCode, AppError> {
        let m = self.registry.load_active_membership(user_id).await?;
        let user_key: UserKey = Zeroizing::new(*m.user_key);
        self.pair_start_with(user_id, transport, user_key).await
    }

    async fn pair_start_with(
        &self,
        user_id: &str,
        transport: Arc<dyn PairTransport>,
        user_key: UserKey,
    ) -> Result<ShortCode, AppError> {
        let server_url = transport.base_url();
        let started = start_pair(transport.as_ref())
            .await
            .map_err(|e| e.explain_insecure_relay(&server_url))?;
        upload_pair_payload(
            transport.as_ref(),
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

        // The watch carries the sink and the transport and never the facade: a
        // task holding the last `Arc<Sharepaste>` would drop, from inside it, the
        // very runtime it is running on.
        self.runtime.spawn(watch_for_claim(
            transport,
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
        // cleartext relay just as a typed one can. Same choke point, same answer.
        let server = ServerClient::new(&server_url, self.transport)?;
        let proof = secret_proof_hex(&decoded.pairing_secret);
        server
            .pair_claim(&decoded.pair_id.to_string(), &proof)
            .await
            .map_err(|e| e.explain_insecure_relay(&server_url))?;

        let payload =
            fetch_and_decrypt_pair_payload(&server, decoded.pair_id, &decoded.pairing_secret)
                .await
                .map_err(|e| e.explain_insecure_relay(&server_url))?;
        let device = server
            .devices(&decoded.pair_id.to_string(), &proof, device_label)
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
        let cursor = before.map(|c| (c.rank, c.ord, c.id));
        let rows = entries_cache::list_recent(&conn, user_id, cursor, limit)?;
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
        entries_cache::get_full(&conn, user_id, entry_id)
    }

    /// Put one entry back on the clipboard, and record the **Use**.
    ///
    /// Returns as soon as the clipboard has the text. The use goes to the relay
    /// on the facade's own runtime, because this is the popover's paste-and-go
    /// path: it awaits this call before it hides, and the HTTP client has no
    /// request timeout, so awaiting a round trip here would let a relay that
    /// accepts a connection and never answers hang the paste. Nothing is lost
    /// by not waiting — [`UseRecorder::record`] returns nothing, and what it
    /// changes reaches a shell on `HistoryChanged` and `PendingCount`.
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

    /// The same recall over a transport the caller supplies, awaited rather
    /// than spawned — the seam the use-recording tests drive, for the reason
    /// [`Self::recall_latest_over`] exists.
    #[cfg(any(test, feature = "testing"))]
    pub async fn recall_over(
        &self,
        user_id: &str,
        entry_id: i64,
        transport: &dyn SessionTransport,
    ) -> Result<(), AppError> {
        self.place_on_clipboard(user_id, entry_id).await?;
        self.use_recorder().record_over(user_id, entry_id, transport).await;
        Ok(())
    }

    /// Read one entry's plaintext and put it on the clipboard — everything a
    /// Recall is before the use is recorded.
    ///
    /// One body, so the seam above cannot come to differ from the operation it
    /// stands in for. The self-write marker ordering lives in `write_clipboard`
    /// and only there; nothing may reach the clipboard by another route.
    async fn place_on_clipboard(&self, user_id: &str, entry_id: i64) -> Result<(), AppError> {
        let plaintext = {
            let conn = self.state.conn.lock().await;
            entries_cache::get_full(&conn, user_id, entry_id)?
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
        let server_url = m.server.base().to_string();
        self.recall_latest_with(user_id, Arc::new(ServerSession(m.server)), &user_key, &server_url)
            .await
    }

    /// The same recall over a transport the caller supplies.
    #[cfg(any(test, feature = "testing"))]
    pub async fn recall_latest_over(
        &self,
        user_id: &str,
        transport: Arc<dyn SessionTransport>,
    ) -> Result<Recalled, AppError> {
        let m = self.registry.load_active_membership(user_id).await?;
        let user_key: UserKey = Zeroizing::new(*m.user_key);
        let server_url = m.server.base().to_string();
        self.recall_latest_with(user_id, transport, &user_key, &server_url)
            .await
    }

    async fn recall_latest_with(
        &self,
        user_id: &str,
        transport: Arc<dyn SessionTransport>,
        user_key: &UserKey,
        server_url: &str,
    ) -> Result<Recalled, AppError> {
        let drained = session::drain_pending(
            &self.state,
            self.events.clone(),
            user_id,
            transport.clone(),
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
            user_key,
            transport.as_ref(),
        )
        .await
        {
            // Not fatal: the newest cached entry is still the best answer
            // available, and the caller is told which one it got.
            tracing::warn!(
                err = %e.explain_insecure_relay(server_url), %user_id,
                "recall latest could not reach the relay; falling back to the cache"
            );
            source = RecallSource::Cache;
        }
        let newest = {
            let conn = self.state.conn.lock().await;
            entries_cache::list_recent(&conn, user_id, None, 1)?
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
        // After the clipboard, and over the transport this recall already
        // holds: the entry it handed back becomes the head of the History on
        // every device. Awaited rather than spawned, unlike `recall`: this
        // operation performs a relay round trip by design — the backfill above
        // — so its caller is already prepared for one.
        self.use_recorder()
            .record_over(user_id, entry_id, transport.as_ref())
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
        let reach = {
            let conn = self.state.conn.lock().await;
            entries_cache::reach_of(&conn, user_id, entry_id)?
        };
        let withdrawn = match reach {
            entries_cache::Reach::Absent => {
                return Err(AppError::NotFound(format!(
                    "no entry {entry_id} on this device"
                )))
            }
            entries_cache::Reach::Flushed(relay_id) => {
                let m = self.registry.load_active_membership(user_id).await?;
                let server_url = m.server.base().to_string();
                m.server
                    .delete_entry(relay_id)
                    .await
                    .map_err(|e| e.explain_insecure_relay(&server_url))?;
                let conn = self.state.conn.lock().await;
                entries_cache::delete_one(&conn, user_id, entry_id)?;
                false
            }
            entries_cache::Reach::Unflushed => {
                let conn = self.state.conn.lock().await;
                // The row and the act together, and in that order for no reason
                // that matters: nothing else can observe the gap, because both
                // writes happen under this one lock.
                entries_cache::delete_one(&conn, user_id, entry_id)?;
                pending::delete_for_entry(&conn, user_id, entry_id)?;
                true
            }
        };
        self.events.emit(CoreEvent::EntryDeleted {
            user_id: user_id.to_string(),
            entry_id,
        });
        if withdrawn {
            // The withdrawal changed the queue depth, and the count chrome is
            // the only thing that says so.
            let count = self.pending_depth(user_id).await?;
            self.events.emit(CoreEvent::PendingCount {
                user_id: user_id.to_string(),
                count,
            });
        }
        Ok(())
    }

    /// Put a **Refused** act back in the queue.
    ///
    /// A fresh act and not a retry, so it goes to the back of the queue and
    /// thereby to the head of the History, and it carries nothing forward from the
    /// refusal that preceded it — `requeue_to_back` copies the act and leaves
    /// `refused_at`, `attempts` and `last_error` behind with the row it replaces.
    ///
    /// **Not a Use.** There is no relay record to move: the relay never took this
    /// act, which is what a refusal means.
    ///
    /// If nothing about the cause has changed it will be refused again, which is
    /// honest. The verb is for after the relay's limit or the Active Pairing has
    /// changed, and there is no way for this device to know that has happened.
    pub async fn resend(&self, user_id: &str, entry_id: i64) -> Result<(), AppError> {
        {
            let conn = self.state.conn.lock().await;
            let rowid = pending::rowid_for_entry(&conn, user_id, entry_id)?.ok_or_else(|| {
                AppError::NotFound(format!("no act queued for entry {entry_id}"))
            })?;
            pending::requeue_to_back(&conn, rowid, crate::now_ms())?;
        }
        // The row left the refused region for the head of the pending one, which
        // reorders the list. The queue depth is unchanged — the act moved, it did
        // not multiply — so nothing reports one.
        self.events.emit(CoreEvent::HistoryChanged {
            user_id: user_id.to_string(),
        });
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
    pub async fn clear_history(&self, user_id: &str) -> Result<(), AppError> {
        let m = self.registry.load_active_membership(user_id).await?;
        let server_url = m.server.base().to_string();
        m.server
            .delete_all_entries()
            .await
            .map_err(|e| e.explain_insecure_relay(&server_url))?;
        self.forget_history(user_id).await
    }

    /// Everything `clear_history` does once the relay has agreed, and the seam
    /// its own tests drive.
    ///
    /// Separate because the relay half needs a relay: `delete_all_entries` is on
    /// the pairing's own `ServerClient` rather than on a `SessionTransport`, so a
    /// fake cannot stand in for it — and widening that trait for one test would
    /// put a route on it that nothing in a session ever calls.
    async fn forget_history(&self, user_id: &str) -> Result<(), AppError> {
        let count = {
            let conn = self.state.conn.lock().await;
            entries_cache::delete_all(&conn, user_id)?;
            pending::delete_all(&conn, user_id)?;
            pending::count(&conn, user_id)?
        };
        self.events.emit(CoreEvent::HistoryChanged {
            user_id: user_id.to_string(),
        });
        self.events.emit(CoreEvent::PendingCount {
            user_id: user_id.to_string(),
            count,
        });
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
        let hash = entries_cache::plaintext_sha256(&text);
        let held = {
            let conn = self.state.conn.lock().await;
            // **The queue first, and only then the cache.** An un-flushed
            // capture is now both a row in `entries_cache` and an act in the
            // queue, and only the queue answer is actionable: the entry has no
            // relay id to record a use against, and re-copying it is the same
            // act as copying it (ADR 0012). A settled entry misses here — its
            // queued act is long gone — and falls through to the cache.
            match pending::find_by_hash(&conn, user_id, &hash)? {
                Some(rowid) => Held::Pending(rowid),
                None => match entries_cache::find_by_hash(&conn, user_id, &hash)? {
                    Some(entry_id) => Held::Entry(entry_id),
                    None => Held::Nothing,
                },
            }
        };
        match held {
            Held::Nothing => self.enqueue_capture(user_id, text, &hash).await,
            Held::Entry(entry_id) => {
                self.use_recorder().record(user_id, entry_id).await;
                Ok(OfferOutcome::Recognised {
                    pending: self.pending_depth(user_id).await?,
                })
            }
            Held::Pending(rowid) => {
                let pending = {
                    let conn = self.state.conn.lock().await;
                    pending::requeue_to_back(&conn, rowid, crate::now_ms())?;
                    // Unchanged: the act moved, it did not multiply, so there is
                    // nothing to report on `PendingCount`.
                    pending::count(&conn, user_id)?
                };
                Ok(OfferOutcome::Recognised { pending })
            }
        }
    }

    async fn pending_depth(&self, user_id: &str) -> Result<i64, AppError> {
        let conn = self.state.conn.lock().await;
        pending::count(&conn, user_id)
    }

    /// Encrypt, store the Entry, queue the act, report.
    ///
    /// **The Entry and the act go in together, in one transaction.** An Entry
    /// exists from the moment of capture (ADR 0016), and the row is what the
    /// queued act is *for*: a queue row with no entry would be an act nobody can
    /// see or withdraw, and an entry with no queued act would be content that
    /// never reaches the relay. Either half alone is a worse state than neither.
    ///
    /// `EntryAdded` fires here and nowhere else for a capture. The flush that
    /// follows creates nothing and announces nothing.
    async fn enqueue_capture(
        &self,
        user_id: &str,
        text: String,
        plaintext_sha256: &str,
    ) -> Result<OfferOutcome, AppError> {
        let m = self.registry.load_active_membership(user_id).await?;
        let ciphertext = crate::crypto::encrypt(&m.user_key, user_id, text.as_bytes())?;
        let (entry, count) = {
            let conn = self.state.conn.lock().await;
            // This device is the Entry's Origin, and the pairing row is where
            // its id lives. Without one there is nothing to attribute the row
            // to, which is a broken pairing rather than a capture to guess at.
            let device_id = accounts::find(&conn, user_id)?
                .ok_or_else(|| AppError::NotFound(format!("no pairing for {user_id}")))?
                .device_id;
            let tx = conn.unchecked_transaction()?;
            let local_id = entries_cache::insert_captured(
                &tx, user_id, &ciphertext, &text, plaintext_sha256, &device_id,
            )?;
            pending::enqueue_capture(
                &tx, user_id, local_id, &ciphertext, plaintext_sha256, crate::now_ms(),
            )?;
            let count = pending::count(&tx, user_id)?;
            tx.commit()?;
            let device_label =
                devices::label_for(&conn, user_id, &device_id).unwrap_or_default();
            let entry = Entry::new(
                local_id,
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
            (entry, count)
        };
        self.events.emit(CoreEvent::EntryAdded {
            user_id: user_id.to_string(),
            entry,
        });
        self.events.emit(CoreEvent::PendingCount {
            user_id: user_id.to_string(),
            count,
        });
        self.nudge_uploader(user_id);
        Ok(OfferOutcome::Queued { pending: count })
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
        self.record_over(user_id, entry_id, &ServerSession(m.server))
            .await;
    }

    /// The same, over a transport the caller already holds — `recall_latest`
    /// has one, and building a second is a second round trip for nothing.
    ///
    /// The relay's `last_use` is applied to the local row directly rather than
    /// waited for on the echo — the rule the uploader's reconciliation states
    /// for an Entry this device created, applied to a Use this device made. The
    /// watermark is deliberately not advanced with it, for the same reason: it
    /// means "everything up to here has been fetched", and this device has
    /// fetched nothing.
    async fn record_over(&self, user_id: &str, entry_id: i64, transport: &dyn SessionTransport) {
        // `entry_id` is this device's own; the relay has never heard of it. A row
        // with no relay id has no relay record to move, so the use it just
        // received is carried by the queue instead. A row that is not here at all
        // is nothing to move either way.
        let relay_id = {
            let conn = self.state.conn.lock().await;
            match entries_cache::reach_of(&conn, user_id, entry_id) {
                Ok(entries_cache::Reach::Flushed(relay_id)) => relay_id,
                Ok(entries_cache::Reach::Unflushed) => {
                    drop(conn);
                    self.move_act_to_back(user_id, entry_id).await;
                    return;
                }
                Ok(entries_cache::Reach::Absent) => {
                    tracing::warn!(entry_id, "a use arrived for an entry this device does not hold");
                    return;
                }
                Err(e) => {
                    tracing::warn!(err = %e, entry_id, "could not resolve this entry's relay id");
                    return;
                }
            }
        };
        match transport.use_entry(relay_id).await {
            Ok(used) => {
                let moved = {
                    let conn = self.state.conn.lock().await;
                    match entries_cache::set_last_use(&conn, user_id, relay_id, used.last_use) {
                        Ok(rows) => rows > 0,
                        // Logged rather than swallowed: a cache write that
                        // failed must not be indistinguishable from a use of an
                        // entry this device no longer holds. The relay has the
                        // use either way, and the next backfill brings the row
                        // down with its new Last Use.
                        Err(e) => {
                            tracing::warn!(err = %e, entry_id, "the relay recorded this use but the cache did not");
                            false
                        }
                    }
                };
                if moved {
                    self.events.emit(CoreEvent::HistoryChanged {
                        user_id: user_id.to_string(),
                    });
                }
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
        let count = {
            let conn = self.state.conn.lock().await;
            pending::enqueue_use(&conn, user_id, entry_id, relay_id, crate::now_ms())?;
            pending::count(&conn, user_id)?
        };
        self.events.emit(CoreEvent::PendingCount {
            user_id: user_id.to_string(),
            count,
        });
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
    /// `requeue_to_back` already drops the attempt count and the last error as "a
    /// fresh act, not a retry", which is what a Recall is.
    ///
    /// The queue depth does not change, so nothing reports one. What did change is
    /// where the row sits, and `HistoryChanged` is what says so.
    async fn move_act_to_back(&self, user_id: &str, entry_id: i64) {
        let moved = {
            let conn = self.state.conn.lock().await;
            match pending::rowid_for_entry(&conn, user_id, entry_id) {
                Ok(Some(rowid)) => {
                    pending::requeue_to_back(&conn, rowid, crate::now_ms()).is_ok()
                }
                Ok(None) => {
                    // No relay id and no queued act: the row is beyond anything
                    // this can do for it, which is the state a withdrawn act
                    // leaves and nothing else reaches.
                    tracing::info!(entry_id, "no act queued for this entry; nothing to move");
                    false
                }
                Err(e) => {
                    tracing::warn!(err = %e, entry_id, "could not find the act queued for this entry");
                    false
                }
            }
        };
        if moved {
            self.events.emit(CoreEvent::HistoryChanged {
                user_id: user_id.to_string(),
            });
        }
    }
}

/// Cached rows as a shell renders them, with each Origin resolved against the
/// Device mirror and each region read off the row.
///
/// A `device_id` the mirror has never heard of keeps a `None` label rather than
/// failing the row: a device paired since the last `GET /me`, or a relay too old
/// to serve one, is expected — [`Entry::new`] falls back to a slice of the id.
fn to_entries(
    rows: Vec<entries_cache::CachedEntry>,
    labels: &HashMap<String, String>,
) -> Vec<Entry> {
    rows.into_iter()
        .map(|r| {
            let device_label = labels.get(&r.device_id).cloned();
            let queued = r.region.queued();
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
    transport: Arc<dyn PairTransport>,
    events: Arc<dyn EventSink>,
    user_id: String,
    pair_id: String,
) {
    loop {
        match transport.poll(&pair_id, PAIR_POLL_TIMEOUT_MS).await {
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
    use crate::keychain::{token_account, user_key_account, InMemoryKeychain};
    use crate::storage::accounts;
    use crate::testing::{
        encrypted_row, live_entry, FakeClipboard, FakePasteboard, RecordingSink, ScriptedRelay,
        Wire, SCRIPTED_PAIR_ID, SCRIPTED_RELAY_URL, TEST_USER_KEY_HEX,
    };

    fn facade(clipboard: Arc<FakeClipboard>) -> Arc<Sharepaste> {
        Sharepaste::open_in_memory(SharepasteConfig {
            db_path: PathBuf::from("ignored-by-open_in_memory"),
            keychain: Arc::new(InMemoryKeychain::default()),
            clipboard,
            events: Arc::new(RecordingSink::default()),
            // The desktop's policy. The scheme rule has its own tests in
            // `http::client` and `pairing::registry`; these are about everything else.
            require_https: false,
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
        let sp = Sharepaste::open_in_memory(SharepasteConfig {
            db_path: PathBuf::from("ignored-by-open_in_memory"),
            keychain: keychain.clone(),
            clipboard: clipboard.clone(),
            events: sink.clone(),
            require_https: false,
        })
        .unwrap();
        seed_pairing(&sp, &keychain, "https://srv");

        let relay = ScriptedRelay::new(
            vec![Ok(vec![encrypted_row(1, "u", "from the backfill", "d")])],
            vec![Wire::Holds(vec![live_entry(2, "u", "live one", "d")])],
        );
        sp.runtime
            .block_on(sp.start_session_over("u", relay.clone()))
            .unwrap();

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
            require_https: false,
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
        let sp = Sharepaste::open_in_memory(SharepasteConfig {
            db_path: PathBuf::from("ignored-by-open_in_memory"),
            keychain: keychain.clone(),
            clipboard: FakeClipboard::new(),
            events: sink.clone(),
            require_https: false,
        })
        .unwrap();
        seed_pairing(&sp, &keychain, "https://srv");

        let relay = ScriptedRelay::new(vec![Ok(Vec::new())], vec![Wire::Holds(Vec::new())]);
        sp.runtime
            .block_on(sp.start_session_over("u", relay.clone()))
            .unwrap();
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
        let sp = Sharepaste::open_in_memory(SharepasteConfig {
            db_path: PathBuf::from("ignored-by-open_in_memory"),
            keychain: keychain.clone(),
            clipboard: FakeClipboard::new(),
            events: sink.clone(),
            require_https: false,
        })
        .unwrap();
        seed_pairing(&sp, &keychain, "https://srv");

        let relay = ScriptedRelay::new(vec![Ok(Vec::new())], vec![Wire::Holds(Vec::new())]);
        sp.runtime
            .block_on(sp.start_session_over("u", relay.clone()))
            .unwrap();
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

    /// `server_url` decides what a relay call does: nothing here reaches a
    /// network, and every rig points at a port that refuses instantly so an
    /// operation that *does* try is a fast, deterministic failure rather than a
    /// hang.
    fn rig_at(server_url: &str) -> Rig {
        let keychain = Arc::new(InMemoryKeychain::default());
        let sink = Arc::new(RecordingSink::default());
        let clipboard = FakeClipboard::new();
        let sp = Sharepaste::open_in_memory(SharepasteConfig {
            db_path: PathBuf::from("ignored-by-open_in_memory"),
            keychain: keychain.clone(),
            clipboard: clipboard.clone(),
            events: sink.clone(),
            require_https: false,
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
        let Rig { sp, sink, .. } = rig();
        let relay = ScriptedRelay::new(Vec::new(), Vec::new()).pairing(vec![Ok(
            PairClaim::Consumed { device_label: Some("mac-B".into()) },
        )]);

        let observed: Arc<Mutex<Option<Vec<String>>>> = Arc::new(Mutex::new(None));
        let seen = observed.clone();
        let watched_sink = sink.clone();
        relay.on_put_payload(move || {
            *seen.lock() = Some(watched_sink.shortcodes());
        });

        let revealed = sp
            .runtime
            .block_on(sp.pair_start_over("u", relay.clone()))
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

    /*
     * Recall Latest always makes the round trip. Nothing is cached here, so a
     * fetch is the only way to answer at all — and the relay having answered is
     * what makes `Relay` the source.
     */
    #[test]
    fn recall_latest_fetches_and_reports_the_relay_as_the_source() {
        let Rig { sp, clipboard, .. } = rig();
        let relay = ScriptedRelay::new(
            vec![Ok(vec![encrypted_row(7, "u", "the newest", "d")])],
            Vec::new(),
        );

        let out = sp
            .runtime
            .block_on(sp.recall_latest_over("u", relay.clone()))
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
        let Rig { sp, clipboard, .. } = rig();
        seed_entry(&sp, 1, "yesterday's link");
        let relay = ScriptedRelay::new(
            vec![Ok(vec![encrypted_row(2, "u", "today's link", "d")])],
            Vec::new(),
        );

        let out = sp
            .runtime
            .block_on(sp.recall_latest_over("u", relay))
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
        let Rig { sp, clipboard, .. } = rig();
        seed_entry(&sp, 1, "the only one cached");
        let relay = ScriptedRelay::new(vec![Err(AppError::Network("no route".into()))], Vec::new());

        let out = sp
            .runtime
            .block_on(sp.recall_latest_over("u", relay))
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
        let Rig { sp, .. } = rig();
        seed_entry(&sp, 1, "already had it");
        let relay = ScriptedRelay::new(vec![Ok(Vec::new())], Vec::new());

        let out = sp
            .runtime
            .block_on(sp.recall_latest_over("u", relay))
            .unwrap();

        assert_eq!(out.text, "already had it");
        assert_eq!(out.source, RecallSource::Relay);
    }

    #[test]
    fn recall_latest_with_nothing_anywhere_is_not_found() {
        let Rig { sp, .. } = rig();
        let relay = ScriptedRelay::new(vec![Ok(Vec::new())], Vec::new());
        // `Recalled` has no `Debug` on purpose, so match rather than `unwrap_err`.
        let Err(err) = sp.runtime.block_on(sp.recall_latest_over("u", relay)) else {
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

        let conn = sp.runtime.block_on(sp.state.conn.lock());
        let head = pending::head(&conn, "u").unwrap().unwrap();
        assert!(
            matches!(&head.kind, pending::PendingKind::Capture(_)),
            "the head is still a capture"
        );
        drop(conn);
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
            pending::head(&conn, "u").unwrap().unwrap()
        });
        assert_eq!(queued.local_entry_id, Some(history[0].id));
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
        let head = sp.runtime.block_on(async {
            let conn = sp.state.conn.lock().await;
            pending::head(&conn, "u").unwrap().unwrap()
        });
        assert_eq!(
            head_plaintext(&head),
            "second",
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
        let queued = sp.runtime.block_on(async {
            let conn = sp.state.conn.lock().await;
            (pending::count(&conn, "u").unwrap(), pending::head(&conn, "u").unwrap())
        });
        assert_eq!(queued.0, 1, "the act went with the row");
        assert_eq!(
            head_plaintext(&queued.1.unwrap()),
            "keep this",
            "and what is left is the act nobody withdrew"
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
        let Rig { sp, sink, .. } = rig();
        let relay = ScriptedRelay::new(Vec::new(), Vec::new());
        sp.runtime.block_on(sp.offer("u", "captured first")).unwrap();
        sp.runtime.block_on(sp.offer("u", "captured second")).unwrap();
        let first = history_ids(&sp)[1];

        sp.runtime.block_on(sp.recall_over("u", first, relay.as_ref())).unwrap();

        assert_eq!(
            history_ids(&sp)[0], first,
            "the recalled capture leads the History"
        );
        let queued = sp.runtime.block_on(async {
            let conn = sp.state.conn.lock().await;
            (
                pending::count(&conn, "u").unwrap(),
                head_plaintext(&pending::head(&conn, "u").unwrap().unwrap()),
            )
        });
        assert_eq!(queued.0, 2, "one act moved; nothing was queued beside it");
        assert_eq!(
            queued.1, "captured second",
            "and the recalled act is behind it now: `second` is the head of the queue"
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
        let Rig { sp, .. } = rig();
        let relay = ScriptedRelay::new(Vec::new(), Vec::new());
        sp.runtime.block_on(sp.offer("u", "A")).unwrap();
        sp.runtime.block_on(sp.offer("u", "B")).unwrap();
        let a = history_ids(&sp)[1];
        sp.runtime.block_on(sp.recall_over("u", a, relay.as_ref())).unwrap();

        // The queue, drained in its own order the way the uploader drains it.
        let sent = sp.runtime.block_on(async {
            let conn = sp.state.conn.lock().await;
            let mut out = Vec::new();
            while let Some(head) = pending::head(&conn, "u").unwrap() {
                out.push(head_plaintext(&head));
                pending::ack(&conn, head.rowid).unwrap();
            }
            out
        });
        assert_eq!(sent, vec!["B".to_string(), "A".to_string()]);
    }

    /*
     * Recall Latest with acts still queued. A local act and a remote entry cannot
     * be compared while one of them has no stamp, so the queue is emptied first
     * and the relay's clock decides — which is how every device gives the same
     * answer.
     */
    #[test]
    fn recall_latest_drains_the_queue_before_it_reads_the_head() {
        let Rig { sp, clipboard, .. } = rig();
        sp.runtime.block_on(sp.offer("u", "queued while offline")).unwrap();
        let relay = ScriptedRelay::new(vec![Ok(Vec::new())], Vec::new());

        let out = sp.runtime.block_on(sp.recall_latest_over("u", relay.clone())).unwrap();

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
            pending::count(&conn, "u").unwrap()
        });
        assert_eq!(left, 0, "and the queue is empty");
    }

    /*
     * The same with the relay out of reach: the local head is still the best
     * answer available, and `Cache` is what says nobody else agrees with it.
     */
    #[test]
    fn recall_latest_with_a_queue_and_no_relay_falls_back_to_the_local_head() {
        let Rig { sp, clipboard, .. } = rig();
        sp.runtime.block_on(sp.offer("u", "still owed to the relay")).unwrap();
        let relay = ScriptedRelay::new(
            vec![Err(AppError::Network("no route".into()))],
            Vec::new(),
        )
        .answering_uploads(crate::testing::UploadAnswer::Unreachable);

        let out = sp.runtime.block_on(sp.recall_latest_over("u", relay.clone())).unwrap();

        assert_eq!(out.text, "still owed to the relay");
        assert_eq!(out.source, RecallSource::Cache);
        assert_eq!(clipboard.writes(), vec!["still owed to the relay".to_string()]);
        let left = sp.runtime.block_on(async {
            let conn = sp.state.conn.lock().await;
            pending::count(&conn, "u").unwrap()
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
        let Rig { sp, clipboard, .. } = rig();
        sp.runtime.block_on(sp.offer("u", "behind a silent relay")).unwrap();
        let relay = ScriptedRelay::new(vec![Ok(Vec::new())], Vec::new()).answering_uploads(crate::testing::UploadAnswer::Stall);

        let out = sp.runtime.block_on(sp.recall_latest_over("u", relay)).unwrap();

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
            let rowid = pending::rowid_for_entry(&conn, "u", refused).unwrap().unwrap();
            pending::record_failure(&conn, rowid, "an earlier attempt").unwrap();
            pending::refuse(&conn, rowid, 1, "payload too large").unwrap();
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
        let head = sp.runtime.block_on(async {
            let conn = sp.state.conn.lock().await;
            (
                pending::count(&conn, "u").unwrap(),
                pending::head(&conn, "u").unwrap().unwrap(),
            )
        });
        assert_eq!(head.0, 2, "the act moved; nothing was queued beside it");
        assert_eq!(head.1.attempts, 0, "and carries nothing forward from the refusal");
        assert_eq!(head.1.last_error, None);
        assert!(
            sink.saw_history_changed("u"),
            "the row left the refused region, which reorders the list"
        );
    }

    /*
     * `clear_history` used to wipe the cache and leave the queue standing, so the
     * next flush put back exactly what had just been cleared. The queue goes with
     * it.
     *
     * Driven through `forget_history`, which is everything the operation does
     * once the relay has agreed; that the relay is asked *first* is what
     * `an_unreachable_cleartext_relay_explains_itself` pins.
     */
    #[test]
    fn clearing_the_history_empties_the_queue_with_it() {
        let Rig { sp, sink, .. } = rig();
        sp.runtime.block_on(sp.offer("u", "queued and then cleared")).unwrap();

        sp.runtime.block_on(sp.forget_history("u")).unwrap();
        let after = sp.runtime.block_on(async {
            let conn = sp.state.conn.lock().await;
            (
                entries_cache::list_recent(&conn, "u", None, 50).unwrap().len(),
                pending::count(&conn, "u").unwrap(),
            )
        });
        assert_eq!(after, (0, 0), "nothing cached and nothing left to repopulate it");
        assert!(
            sink.pending_counts().ends_with(&[0]),
            "and the count says so: {:?}",
            sink.pending_counts()
        );
    }

    /// The plaintext of a queued capture, decrypted the way the relay's echo of
    /// it would be. The queue holds ciphertext, and ciphertext cannot be
    /// compared: `crypto::encrypt` draws a fresh nonce every call.
    fn head_plaintext(head: &pending::PendingUpload) -> String {
        let pending::PendingKind::Capture(ciphertext) = &head.kind else {
            panic!("expected a queued capture at the head");
        };
        let plain = crate::crypto::decrypt(&crate::testing::test_user_key(), "u", ciphertext)
            .expect("the queue holds what this pairing's key sealed");
        String::from_utf8(plain).unwrap()
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
        let Rig { sp, sink, clipboard } = rig();
        let long_ago = crate::now_ms() - 3 * 24 * 60 * 60 * 1000;
        seed_entry_at(&sp, 1, "ssh admin@10.0.0.4", long_ago);
        seed_entry_at(&sp, 2, "captured since", crate::now_ms() - 1000);
        assert_eq!(history_ids(&sp), vec![2, 1], "precondition: 1 is buried");

        let relay = ScriptedRelay::new(Vec::new(), Vec::new());
        sp.runtime
            .block_on(sp.recall_over("u", 1, relay.as_ref()))
            .unwrap();

        assert_eq!(clipboard.writes(), vec!["ssh admin@10.0.0.4".to_string()]);
        assert_eq!(relay.uses(), vec![1], "the relay was told, so every device reorders");
        assert_eq!(history_ids(&sp), vec![1, 2], "and this device did not wait to be told back");
        assert!(sink.saw_history_changed("u"));

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
        let Rig { sp, .. } = rig();
        seed_entry_at(&sp, 1, "the only one", crate::now_ms() - 1000);
        let relay = ScriptedRelay::new(Vec::new(), Vec::new());
        sp.runtime
            .block_on(sp.recall_over("u", 1, relay.as_ref()))
            .unwrap();
        assert_eq!(relay.uses(), vec![1]);
    }

    /*
     * Skew is not handled: a relay without the route answers exactly as a relay
     * without the entry does, and neither earns a queued retry that could never
     * succeed. The Recall itself never needed the relay and still stands.
     */
    #[test]
    fn a_relay_that_will_not_record_the_use_neither_fails_nor_queues_it() {
        let Rig { sp, clipboard, .. } = rig();
        seed_entry_at(&sp, 1, "recalled anyway", crate::now_ms() - 1000);
        let relay =
            ScriptedRelay::new(Vec::new(), Vec::new()).answering_uses(crate::testing::UseAnswer::Gone);

        sp.runtime
            .block_on(sp.recall_over("u", 1, relay.as_ref()))
            .unwrap();

        assert_eq!(clipboard.writes(), vec!["recalled anyway".to_string()]);
        let conn = sp.runtime.block_on(sp.state.conn.lock());
        assert_eq!(pending::count(&conn, "u").unwrap(), 0, "a 404 is not worth retrying");
    }

    /*
     * An outage does not lose the use. It queues beside the captures and reaches
     * the relay in the order it was made.
     */
    #[test]
    fn a_use_the_relay_cannot_be_told_about_is_queued() {
        let Rig { sp, sink, .. } = rig();
        seed_entry_at(&sp, 1, "recalled offline", crate::now_ms() - 1000);
        let relay = ScriptedRelay::new(Vec::new(), Vec::new())
            .answering_uses(crate::testing::UseAnswer::Unreachable);

        sp.runtime
            .block_on(sp.recall_over("u", 1, relay.as_ref()))
            .unwrap();

        let conn = sp.runtime.block_on(sp.state.conn.lock());
        let head = pending::head(&conn, "u").unwrap().unwrap();
        assert_eq!(head.kind, pending::PendingKind::Use(1));
        drop(conn);
        assert_eq!(sink.pending_counts(), vec![1], "and the queue depth is reported");
    }

    /*
     * A Recall that never reached the clipboard put nothing anywhere, and
     * nothing is not a use of anything.
     */
    #[test]
    fn a_recall_whose_clipboard_write_fails_records_no_use() {
        let Rig { sp, clipboard, .. } = rig();
        seed_entry_at(&sp, 1, "never arrived", crate::now_ms() - 1000);
        clipboard.on_write(|_| Err(AppError::Storage("clipboard failed".into())));
        let relay = ScriptedRelay::new(Vec::new(), Vec::new());

        let err = sp
            .runtime
            .block_on(sp.recall_over("u", 1, relay.as_ref()))
            .unwrap_err();

        assert!(matches!(err, AppError::Storage(_)));
        assert!(relay.uses().is_empty());
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
        let head = pending::head(&conn, "u").unwrap().unwrap();
        assert_eq!(
            head.kind,
            pending::PendingKind::Use(1),
            "the one thing queued is the use, not a duplicate capture"
        );
        assert_eq!(pending::count(&conn, "u").unwrap(), 1);
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
            entries_cache::mark_undecryptable(&conn, "u", 1).unwrap();
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

    fn cached(id: i64, device_id: &str) -> entries_cache::CachedEntry {
        entries_cache::CachedEntry {
            user_id: "u".into(),
            local_id: id,
            relay_id: Some(id),
            ciphertext: vec![1],
            plaintext: Some(format!("entry {id}")),
            created_at: 1_000 + id,
            last_use: 1_000 + id,
            device_id: device_id.into(),
            region: entries_cache::Region {
                rank: 2,
                ord: 1_000 + id,
                refused_reason: None,
            },
        }
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
        let Rig { sp, sink, .. } = rig();
        let relay = ScriptedRelay::new(
            vec![Ok(Vec::new())],
            vec![Wire::Holds(vec![live_entry(9, "somebody-else", "unreadable here", "d")])],
        );
        sp.runtime
            .block_on(sp.start_session_over("u", relay))
            .unwrap();

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

        let Rig { sp, sink, .. } = rig();
        let relay = ScriptedRelay::new(
            vec![Ok(Vec::new())],
            vec![Wire::Holds(vec![live_entry(4, "u", indented, "d1")])],
        );
        sp.runtime.block_on(sp.start_session_over("u", relay)).unwrap();
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
        // A cursor is taken from a row's region, which is a fact about the order
        // rather than about the entry, so it comes off the cache rather than off
        // `Entry` — see `HistoryCursor`.
        let cursor = rt.block_on(async {
            let conn = sp.state.conn.lock().await;
            let row = entries_cache::list_recent(&conn, "u", None, 1).unwrap().remove(0);
            HistoryCursor { rank: row.region.rank, ord: row.region.ord, id: row.local_id }
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
