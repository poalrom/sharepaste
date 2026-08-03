//! One sync session per Active Pairing.
//!
//! A session owns two long-lived tasks that share a cancellation token: the SSE
//! loop, which backfills missed entries and then streams live ones, and the
//! uploader, which drains the pending queue. Both report their progress to the
//! host through the connection-state and entry events.
//!
//! Nothing here names a window system, an app handle or a runtime it does not
//! own. It notifies through [`EventSink`], reads and writes the facade's
//! [`SessionState`], and spawns on the handle that state carries — which is the
//! facade's private runtime, never the caller's.
//!
//! # The four invariants
//!
//! Four rules hold however the relay behaves. Each has exactly one
//! implementation, and that implementation names its own number, so
//! `grep -rn "invariant [1-4]" clients/core/src` lands on the code every
//! reference below points at. A numbered reference to a list nobody wrote is the
//! one kind of comment the code cannot contradict, which is why the list is
//! here.
//!
//! 1. **The watermark only advances past a row that stored.** `last_seen_seq`
//!    means "everything up to here has been fetched", so moving it past a row
//!    that failed to ingest loses that row for good — the next `since=` fetch
//!    starts after it. [`backfill`] stops at the first failure and the whole
//!    tail is re-fetched on the next iteration.
//! 2. **A 404 on `GET /me` latches the route off for the session.** The device
//!    mirror is on its own route by
//!    [ADR 0001](../../../../../docs/adr/0001-device-metadata-out-of-band.md),
//!    so a client can be newer than its relay, and a 404 there stays a 404
//!    until someone redeploys. [`MirrorRoute`] states that once rather than
//!    once a minute, and re-arms on the next reconnect because a redeploy is
//!    what dropped the stream.
//! 3. **Contact is tapped below the SSE parser and written only on the edge out
//!    of `Online`.** A `: heartbeat` comment dispatches no event under the
//!    WHATWG rules, so only the byte stream distinguishes a healthy idle stream
//!    from a dead one — `sse::stamp_contact` stores into a live cell from
//!    there. [`SessionCtx::set_conn_state`] is the one thing that ever moves
//!    that cell into the database, which is what keeps a heartbeat off SQLite.
//! 4. **The pending queue is an uncapped FIFO, and nothing leaves it unsent.**
//!    The uploader drains it head-first so acts reach the relay in the order they
//!    were made, and no depth evicts anything: an act this device has not
//!    delivered is undelivered clipboard content, and the queue used to discard
//!    the oldest of them silently to keep a number under a thousand (ADR 0014).
//!    Two things take an act out of it — the relay taking it, and somebody
//!    withdrawing it — and a **Refused** act stays in it, undeliverable, until it
//!    is resent or deleted (ADR 0015).
//!
//! 1 to 3 are testable with no relay running because [`SessionTransport`] is a
//! trait; 4 splits across [`Uploader`] and storage, and needs no network at all.

use crate::crypto::UserKey;
use crate::errors::AppError;
use crate::event::{CoreEvent, Entry, Queued};
use crate::http::dto::{EntryRow, MeResp};
use crate::http::ServerClient;
use crate::platform::EventSink;
use crate::storage::devices::DeviceRecord;
use crate::storage::{accounts, devices, entries_cache, pending};
use crate::sync::state::should_persist_contact;
use crate::sync::uploader::{UploadTransport, Uploaded, Uploader, UploaderExit, Used};
use crate::sync::{decryptor, sse, BackoffPlan, ConnectionState};
use async_trait::async_trait;
use parking_lot::Mutex;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Notify};
use tokio_util::sync::CancellationToken;

/// How long an entry from an unmirrored device is allowed to provoke a
/// `GET /me`. Without it, a relay that never labels a device would turn every
/// entry from that device into a round trip; one refresh a minute is still
/// prompt enough to pick up a device paired mid-session.
const MIRROR_REFRESH_DEBOUNCE: Duration = Duration::from_secs(60);

/// How many rows one backfill asks for.
///
/// The relay serves `id > since` ascending, so a window this size is a bound on
/// the batch rather than on the catch-up: a device that missed more keeps
/// fetching on the next iteration, from the watermark this one advanced to.
const BACKFILL_LIMIT: u32 = 500;

/// Everything a session needs from the relay.
///
/// The same reason `UploadTransport` exists, applied to the rest of the loop:
/// with the HTTP client behind a trait, invariants 1 to 3 in this module's
/// header are testable with no relay running, which is the only way they get a
/// test at all. There is exactly one production implementation,
/// [`ServerSession`].
#[async_trait]
pub trait SessionTransport: Send + Sync {
    async fn list_entries(&self, since_seq: i64, limit: u32) -> Result<Vec<EntryRow>, AppError>;
    async fn me(&self) -> Result<MeResp, AppError>;
    /// Stream server events until cancelled or the connection drops.
    ///
    /// `contact` is stamped from *inside* the implementation, below whatever
    /// parses the frames — see [`sse::run`].
    async fn stream(
        &self,
        sink: mpsc::Sender<sse::ServerEvent>,
        cancel: CancellationToken,
        contact: Arc<AtomicI64>,
    ) -> Result<(), AppError>;
    async fn upload(&self, ciphertext_b64: &str) -> Result<Uploaded, AppError>;
    /// Record a **Use**: the entry becomes the head of the History everywhere.
    async fn use_entry(&self, entry_id: i64) -> Result<Used, AppError>;
    /// Take one entry off the relay.
    ///
    /// On the session's transport for the withdrawal race alone: a delete issued
    /// while an upload was in flight leaves the relay holding an act nobody
    /// wants, and the uploader is the only thing that knows the relay took it.
    async fn delete_entry(&self, entry_id: i64) -> Result<(), AppError>;
}

/// The one production [`SessionTransport`]: a session over the pairing's
/// authenticated connection.
pub struct ServerSession(pub ServerClient);

#[async_trait]
impl SessionTransport for ServerSession {
    async fn list_entries(&self, since_seq: i64, limit: u32) -> Result<Vec<EntryRow>, AppError> {
        self.0.list_entries(since_seq, limit).await
    }

    async fn me(&self) -> Result<MeResp, AppError> {
        self.0.me().await
    }

    async fn stream(
        &self,
        sink: mpsc::Sender<sse::ServerEvent>,
        cancel: CancellationToken,
        contact: Arc<AtomicI64>,
    ) -> Result<(), AppError> {
        sse::run(self.0.clone(), sink, cancel, contact).await
    }

    async fn upload(&self, ciphertext_b64: &str) -> Result<Uploaded, AppError> {
        self.0
            .post_entry(ciphertext_b64)
            .await
            .map(|r| Uploaded {
                id: r.id,
                created_at: r.created_at,
                seq: r.seq,
                last_use: r.last_use,
            })
    }

    async fn use_entry(&self, entry_id: i64) -> Result<Used, AppError> {
        self.0
            .use_entry(entry_id)
            .await
            .map(|r| Used { seq: r.seq, last_use: r.last_use })
    }

    async fn delete_entry(&self, entry_id: i64) -> Result<(), AppError> {
        self.0.delete_entry(entry_id).await
    }
}

/// Lets the uploader keep its own narrow transport trait while a session hands
/// it the one it already has.
///
/// Trait upcasting would do this for free, but it landed after the rust-version
/// this crate pins.
struct UploadVia(Arc<dyn SessionTransport>);

#[async_trait]
impl UploadTransport for UploadVia {
    async fn upload(&self, ciphertext_b64: &str) -> Result<Uploaded, AppError> {
        self.0.upload(ciphertext_b64).await
    }

    async fn use_entry(&self, entry_id: i64) -> Result<Used, AppError> {
        self.0.use_entry(entry_id).await
    }

    async fn delete_entry(&self, entry_id: i64) -> Result<(), AppError> {
        self.0.delete_entry(entry_id).await
    }
}

/// Whether this session's relay serves `GET /me` at all — the one
/// implementation of invariant 2.
///
/// [ADR 0001](../../../../../docs/adr/0001-device-metadata-out-of-band.md) put
/// the device mirror on its own route, which means a client can be newer than
/// the relay it is paired to — self-hosted deployments skew by definition, and
/// the desktop app updates on its own schedule.
///
/// A 404 there is **permanent**, unlike a network blip: the route will not
/// appear until someone redeploys. Without this latch every entry from an
/// unmirrored device re-probes it (debounced to a minute) and every reconnect
/// probes it again, so a relay one version behind buries real failures under a
/// warning a minute, forever.
#[derive(Default)]
struct MirrorRoute {
    absent: AtomicBool,
}

impl MirrorRoute {
    fn present(&self) -> bool {
        !self.absent.load(Ordering::Relaxed)
    }

    /// Latch the route off. True the first time, so the reason is stated once.
    fn mark_absent(&self) -> bool {
        !self.absent.swap(true, Ordering::Relaxed)
    }

    /// Re-probe on the next attempt.
    ///
    /// Called when the relay is reached again, because redeploying it is what
    /// drops the stream in the first place: an operator who upgrades in place
    /// is picked up on the next reconnect rather than on the next app restart.
    fn rearm(&self) {
        self.absent.store(false, Ordering::Relaxed);
    }
}

/// The protocol state the facade owns and every session task shares.
///
/// Each field is an `Arc` and the struct is cheap to clone, because a session
/// outlives the call that started it. It deliberately does *not* hold the
/// facade: `Sharepaste` owns the runtime these tasks run on, and a task holding
/// the last reference to it would drop that runtime from inside itself.
#[derive(Clone)]
pub struct SessionState {
    pub conn: Arc<tokio::sync::Mutex<Connection>>,
    pub sync_tasks: Arc<Mutex<HashMap<String, CancellationToken>>>,
    pub upload_triggers: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
    pub conn_states: Arc<Mutex<HashMap<String, ConnectionState>>>,
    /// Live Contact, one cell per user with a running session.
    ///
    /// The SSE byte tap stores into these on every chunk the relay sends, so
    /// the reading stays out of SQLite entirely while the session is up.
    /// `get_contact` reads a cell directly; `set_conn_state` flushes it on the
    /// way offline. A cell outlives its session so the last reading is still
    /// answerable after the stream drops.
    pub last_contact: Arc<Mutex<HashMap<String, Arc<AtomicI64>>>>,
    /// A handle onto the facade's private runtime.
    ///
    /// Carried rather than reached for: `Handle::current()` inside the core
    /// would bind a session to whichever runtime happened to call
    /// `start_session`, which on a phone is the UI thread's.
    pub spawn: tokio::runtime::Handle,
}

impl SessionState {
    pub fn new(conn: Arc<tokio::sync::Mutex<Connection>>, spawn: tokio::runtime::Handle) -> Self {
        Self {
            conn,
            sync_tasks: Arc::new(Mutex::new(HashMap::new())),
            upload_triggers: Arc::new(Mutex::new(HashMap::new())),
            conn_states: Arc::new(Mutex::new(HashMap::new())),
            last_contact: Arc::new(Mutex::new(HashMap::new())),
            spawn,
        }
    }
}

/// When the Contact write owed by the edge out of `Online` has to have happened.
///
/// The distinction only matters because the reading lives in an `AtomicI64`
/// until it is written, so who is racing whom decides the answer: the session
/// loop races nothing and cannot afford to block, a shell being backgrounded
/// races the process being killed and cannot afford not to.
enum ContactFlush {
    /// On the facade's runtime, some time after the transition returns.
    Spawned,
    /// On the calling thread, before the transition returns.
    BeforeReturning,
}

/// The handles both session tasks share: the sink to notify through, the
/// facade's state, the pairing the session belongs to, the Device mirror's
/// refresh clock, and whether that mirror's route exists at all.
#[derive(Clone)]
pub(crate) struct SessionCtx {
    events: Arc<dyn EventSink>,
    state: SessionState,
    user_id: String,
    mirror_refreshed_at: Arc<Mutex<Option<Instant>>>,
    mirror_route: Arc<MirrorRoute>,
}

impl SessionCtx {
    pub(crate) fn new(events: Arc<dyn EventSink>, state: SessionState, user_id: String) -> Self {
        Self {
            events,
            state,
            user_id,
            mirror_refreshed_at: Arc::new(Mutex::new(None)),
            mirror_route: Arc::new(MirrorRoute::default()),
        }
    }

    /// Record a connection-state transition and tell the host about it — the
    /// write half of invariant 3.
    ///
    /// Contact is flushed here, on the edge out of `Online` and nowhere else —
    /// see [`should_persist_contact`]. This method and
    /// [`Self::set_conn_state_before_returning`] are the only two transitions
    /// there are, and they differ in nothing but when that flush has finished,
    /// so a future caller cannot forget it.
    ///
    /// For callers already on the facade's runtime — the session loop and the
    /// uploader — which is why the flush is spawned: a `select!` arm must not
    /// block on the database.
    pub(crate) fn set_conn_state(&self, new_state: ConnectionState, last_error: Option<String>) {
        self.transition(new_state, last_error, ContactFlush::Spawned);
    }

    /// [`Self::set_conn_state`], with the Contact write finished before this
    /// returns.
    ///
    /// For a shell tearing the session down rather than the session reacting to
    /// a relay. `Sharepaste::stop_all_sessions` runs on an Android `onStop`,
    /// which is precisely the moment the OS may stop the process — and until
    /// this write lands, the live `AtomicI64` is the only copy of the reading.
    /// A flush spawned onto the runtime races that kill and loses the reading
    /// when it loses, which is no better than never having written it.
    ///
    /// Blocks the calling thread on the database, so it must not be called from
    /// inside the facade's runtime — the same contract as
    /// `Sharepaste::block_on`, and satisfied for the same reason: every caller
    /// is a shell thread.
    pub(crate) fn set_conn_state_before_returning(
        &self,
        new_state: ConnectionState,
        last_error: Option<String>,
    ) {
        self.transition(new_state, last_error, ContactFlush::BeforeReturning);
    }

    fn transition(
        &self,
        new_state: ConnectionState,
        last_error: Option<String>,
        flush: ContactFlush,
    ) {
        let prev = self
            .state
            .conn_states
            .lock()
            .insert(self.user_id.clone(), new_state);
        if should_persist_contact(prev, new_state) {
            match flush {
                ContactFlush::Spawned => self.persist_contact_spawned(),
                ContactFlush::BeforeReturning => self.persist_contact_before_returning(),
            }
        }
        self.events.emit(CoreEvent::ConnectionState {
            user_id: self.user_id.clone(),
            state: new_state,
            last_error,
        });
    }

    /// The live Contact cell, if any session ever took a reading for this user.
    fn contact_cell(&self) -> Option<Arc<AtomicI64>> {
        self.state.last_contact.lock().get(&self.user_id).cloned()
    }

    /// Write the live Contact reading to the database, off-thread because the
    /// database sits behind an async lock.
    fn persist_contact_spawned(&self) {
        let Some(cell) = self.contact_cell() else {
            return; // no session ever held a cell for this user
        };
        let ctx = self.clone();
        ctx.state.spawn.clone().spawn(async move {
            let at = {
                let conn = ctx.state.conn.lock().await;
                ctx.flushed(&conn, &cell)
            };
            // Outside the guard: a sink is foreign code, and one that re-enters
            // the facade from here would deadlock on the database.
            if let Some(at) = at {
                ctx.report_contact(at);
            }
        });
    }

    /// The same write, on the calling thread and finished before returning.
    fn persist_contact_before_returning(&self) {
        let Some(cell) = self.contact_cell() else {
            return;
        };
        let at = {
            let conn = self.state.conn.blocking_lock();
            self.flushed(&conn, &cell)
        };
        if let Some(at) = at {
            self.report_contact(at);
        }
    }

    /// [`flush_contact`] reduced to the one value worth reporting. A reading
    /// that will not store is logged and dropped: it must not stop the
    /// connection-state transition it is riding on.
    fn flushed(&self, conn: &rusqlite::Connection, cell: &AtomicI64) -> Option<i64> {
        match flush_contact(conn, &self.user_id, cell) {
            Ok(at) => at,
            Err(e) => {
                tracing::warn!(err = %e, "persisting contact failed");
                None
            }
        }
    }

    fn report_contact(&self, at: i64) {
        self.events.emit(CoreEvent::Contact {
            user_id: self.user_id.clone(),
            last_contact_at: Some(at),
        });
    }

    /// True at most once per [`MIRROR_REFRESH_DEBOUNCE`].
    fn claim_mirror_refresh(&self) -> bool {
        let mut last = self.mirror_refreshed_at.lock();
        let now = Instant::now();
        if last.is_some_and(|prev| now.duration_since(prev) < MIRROR_REFRESH_DEBOUNCE) {
            return false;
        }
        *last = Some(now);
        true
    }
}

/// Write a live Contact reading to `accounts.last_contact_at`.
///
/// Returns what was written, or `None` when there is nothing worth writing: a
/// cell still at zero means not one byte has ever arrived from the relay for
/// this user, which is not the same as having been in contact at the epoch.
fn flush_contact(
    conn: &rusqlite::Connection,
    user_id: &str,
    cell: &AtomicI64,
) -> Result<Option<i64>, AppError> {
    let at = cell.load(Ordering::Relaxed);
    if at == 0 {
        return Ok(None);
    }
    accounts::set_last_contact(conn, user_id, at)?;
    Ok(Some(at))
}

/// Refresh the local mirror of the relay's user and device list.
///
/// Best-effort by design: a relay older than this client has no `/me`, and a
/// session that cannot label its Origins is still a working session — rows
/// fall back to a device-id slice and the footer to the opaque user id.
async fn mirror_me(ctx: &SessionCtx, transport: &dyn SessionTransport) {
    if !ctx.mirror_route.present() {
        return;
    }
    let me = match transport.me().await {
        Ok(me) => me,
        // 404 is the relay saying it has no such route, which no amount of
        // retrying changes. Said once, then latched off for this session.
        Err(AppError::NotFound(_)) => {
            if ctx.mirror_route.mark_absent() {
                tracing::warn!(
                    user_id = %ctx.user_id,
                    "relay has no GET /me: Device Labels and the username are unavailable \
                     until it is upgraded. Entries still sync; Origins read as short device ids."
                );
            }
            return;
        }
        Err(e) => {
            tracing::warn!(err = %e, "device mirror refresh failed");
            return;
        }
    };
    let records: Vec<DeviceRecord> = me
        .devices
        .into_iter()
        .map(|d| DeviceRecord { device_id: d.device_id, label: d.label, revoked_at: d.revoked_at })
        .collect();
    let conn = ctx.state.conn.lock().await;
    if let Err(e) = devices::upsert_many(&conn, &ctx.user_id, &records, crate::now_ms()) {
        tracing::warn!(err = %e, "device mirror write failed");
    }
    if let Err(e) = accounts::set_username(&conn, &ctx.user_id, &me.user.username) {
        tracing::warn!(err = %e, "username mirror write failed");
    }
}

/// Re-mirror when an entry names a device the mirror has never heard of.
///
/// A device paired since the last `GET /me` is the only way this happens, and
/// it means the row would otherwise render without its Origin. Rate-limited by
/// [`MIRROR_REFRESH_DEBOUNCE`]; a lookup failure is treated as "known" so a
/// broken read cannot turn into a request loop.
async fn refresh_mirror_if_unknown(
    ctx: &SessionCtx,
    transport: &dyn SessionTransport,
    device_id: &str,
) {
    let known = {
        let conn = ctx.state.conn.lock().await;
        devices::is_mirrored(&conn, &ctx.user_id, device_id).unwrap_or(true)
    };
    if !known && ctx.claim_mirror_refresh() {
        mirror_me(ctx, transport).await;
    }
}

/// Start the sync session for `ctx.user_id`, cancelling whichever one it
/// already had.
///
/// Split from `Sharepaste::start_session` at the transport: choosing the
/// pairing and unlocking its key is the facade's business, everything from here
/// down is the protocol's, and the seam is what makes the protocol testable.
pub(crate) fn spawn_session(
    ctx: SessionCtx,
    transport: Arc<dyn SessionTransport>,
    user_key: UserKey,
) {
    let cancel = CancellationToken::new();
    {
        let mut tasks = ctx.state.sync_tasks.lock();
        if let Some(prev) = tasks.insert(ctx.user_id.clone(), cancel.clone()) {
            prev.cancel();
        }
    }

    // Register the upload trigger up-front so the SSE task can notify it on each
    // successful (re)connect, even before the uploader task has started.
    let upload_trigger = Arc::new(Notify::new());
    ctx.state
        .upload_triggers
        .lock()
        .insert(ctx.user_id.clone(), upload_trigger.clone());

    let spawn = ctx.state.spawn.clone();
    spawn.spawn(run_sse_loop(
        ctx.clone(),
        transport.clone(),
        user_key,
        cancel.clone(),
        upload_trigger.clone(),
    ));
    // Pending-queue uploader on its own task.
    spawn.spawn(run_uploader(ctx, transport, cancel, upload_trigger));
}

/// How long [`drain_pending`] will wait for the queue before giving up on it.
///
/// A number and not a policy: the operation it serves is Recall Latest, which a
/// person is waiting on with the clipboard in mind. Long enough for a burst over
/// a slow link, short enough that an unreachable relay does not hang the verb —
/// and the fallback is a correct answer, just this device's own.
const DRAIN_BOUND: Duration = Duration::from_secs(5);

/// Empty this pairing's queue over one transport, bounded, and report whether it
/// emptied.
///
/// The reason it exists: `recall_latest` cannot compare a local act with a remote
/// entry while one of them has no stamp, so it puts everything on the relay's one
/// clock first. A session's own uploader does the same work on its own trigger;
/// this is the same [`Uploader`] driven once, so the two cannot come to disagree
/// about what settling an act means.
///
/// `Err` means the relay did not take everything — it was unreachable, refused
/// the head, or took longer than [`DRAIN_BOUND`]. In every one of those the queue
/// is left exactly as the uploader left it and nothing is lost.
pub(crate) async fn drain_pending(
    state: &SessionState,
    events: Arc<dyn EventSink>,
    user_id: &str,
    transport: Arc<dyn SessionTransport>,
) -> Result<(), AppError> {
    {
        let conn = state.conn.lock().await;
        if pending::count(&conn, user_id)? == 0 {
            return Ok(());
        }
    }
    let up = Uploader {
        user_id: user_id.to_string(),
        conn: state.conn.clone(),
        transport: Arc::new(UploadVia(transport)),
        // Never notified: this uploader is driven once, by hand.
        trigger: Arc::new(Notify::new()),
        events,
    };
    match tokio::time::timeout(DRAIN_BOUND, up.flush_once()).await {
        Ok(result) => result,
        Err(_) => Err(AppError::Network(format!(
            "the queue did not empty within {}s",
            DRAIN_BOUND.as_secs()
        ))),
    }
}

/// Fetch everything past this pairing's watermark and ingest it.
///
/// The one implementation of invariant 1, and the reason it is a function rather
/// than a fragment of the loop: `recall_latest` performs the same round trip for
/// its own reasons, and a second copy of this is how the watermark rule would
/// come to differ between the two. `last_seen_seq` is re-read here rather than
/// remembered by the caller, so a stream that dropped resumes from the last
/// entry actually ingested.
///
/// `Err` means the **relay** did not answer. A row that fails to ingest is not
/// an error: the watermark simply stays where it was and the whole tail is
/// re-fetched next time. That distinction is what lets `recall_latest` call a
/// completed round trip authoritative even when a row inside it would not store.
pub(crate) async fn backfill(
    state: &SessionState,
    events: &dyn EventSink,
    user_id: &str,
    user_key: &UserKey,
    transport: &dyn SessionTransport,
) -> Result<(), AppError> {
    let last_seen = {
        let conn = state.conn.lock().await;
        accounts::find(&conn, user_id)
            .ok()
            .flatten()
            .map(|a| a.last_seen_seq)
            .unwrap_or(0)
    };
    let rows = transport.list_entries(last_seen, BACKFILL_LIMIT).await?;
    // Scoped so the database guard is gone before anything reaches the sink:
    // see `EventSink`.
    let advanced = {
        let conn = state.conn.lock().await;
        let mut new_last = last_seen;
        for row in rows {
            let (id, seq) = (row.id, row.seq);
            // The watermark is a watermark, not a counter. Advancing it past a row
            // we failed to store loses that row for good, because the next `since=`
            // fetch starts after it — so the first failure ends the run and the whole
            // tail is re-fetched on the next iteration.
            if let Err(e) = decryptor::ingest(&conn, user_key, user_id, &row, crate::now_ms()) {
                tracing::warn!(
                    err = %e, entry_id = id,
                    "backfill ingest failed; leaving last_seen_seq where it was"
                );
                break;
            }
            if seq > new_last {
                new_last = seq;
            }
        }
        let advanced = new_last != last_seen;
        if advanced {
            let _ = accounts::set_last_seen(&conn, user_id, new_last);
        }
        advanced
    };
    if advanced {
        events.emit(CoreEvent::HistoryChanged { user_id: user_id.to_string() });
    }
    Ok(())
}

/// Backfill, then stream, for as long as the session lives.
///
/// Every iteration re-reads `last_seen_seq` from the database, so a stream that
/// drops resumes from the last entry actually ingested rather than replaying or
/// skipping. A failed backfill retries on the shared backoff; a successful one
/// resets it and marks the pairing online.
async fn run_sse_loop(
    ctx: SessionCtx,
    transport: Arc<dyn SessionTransport>,
    user_key: UserKey,
    cancel: CancellationToken,
    upload_trigger: Arc<Notify>,
) {
    let contact: Arc<AtomicI64> = ctx
        .state
        .last_contact
        .lock()
        .entry(ctx.user_id.clone())
        .or_default()
        .clone();
    let mut backoff = BackoffPlan::new();
    loop {
        if cancel.is_cancelled() {
            return;
        }
        ctx.set_conn_state(ConnectionState::Connecting, None);
        match backfill(
            &ctx.state,
            ctx.events.as_ref(),
            &ctx.user_id,
            &user_key,
            transport.as_ref(),
        )
        .await
        {
            Ok(()) => {
                // A backfill that answers is a relay that just came up — quite
                // possibly the redeploy that added the route we gave up on.
                ctx.mirror_route.rearm();
                // The backfill window: bytes arrived here too, and the SSE tap
                // has not opened yet.
                contact.store(crate::now_ms(), Ordering::Relaxed);
            }
            Err(AppError::Auth(s)) => {
                ctx.set_conn_state(ConnectionState::AuthFailed, Some(s));
                return;
            }
            Err(e) => {
                tracing::warn!(err = %e, "backfill failed; will retry");
                ctx.set_conn_state(ConnectionState::Connecting, Some(e.to_string()));
                let delay = backoff.next_delay_secs();
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(std::time::Duration::from_secs(delay)) => {}
                }
                continue;
            }
        }

        mirror_me(&ctx, transport.as_ref()).await;
        ctx.set_conn_state(ConnectionState::Online, None);
        backoff.reset();
        // Server reachable again — push any queued entries.
        upload_trigger.notify_one();

        let (tx, mut rx) = mpsc::channel::<sse::ServerEvent>(64);
        let transport_for_sse = transport.clone();
        let cancel_for_sse = cancel.clone();
        let contact_for_sse = contact.clone();
        let sse_handle = ctx.state.spawn.spawn(async move {
            transport_for_sse
                .stream(tx, cancel_for_sse, contact_for_sse)
                .await
        });

        'recv: loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                ev = rx.recv() => match ev {
                    None => break 'recv,
                    Some(sse::ServerEvent::Entry { id, ciphertext, created_at, device_id, seq, last_use }) => {
                        refresh_mirror_if_unknown(&ctx, transport.as_ref(), &device_id).await;
                        let row = EntryRow {
                            id, ciphertext, created_at, device_id: device_id.clone(), seq, last_use,
                        };
                        // Scoped so the database guard is gone before anything
                        // reaches the sink: see `EventSink`.
                        let (added, reordered) = {
                            let conn = ctx.state.conn.lock().await;
                            match decryptor::ingest(&conn, &user_key, &ctx.user_id, &row, crate::now_ms()) {
                                Ok(out) => {
                                    let _ = accounts::set_last_seen(&conn, &ctx.user_id, seq);
                                    let device_label =
                                        devices::label_for(&conn, &ctx.user_id, &device_id)
                                            .unwrap_or_default();
                                    // `EntryAdded` only when the cache did not
                                    // already hold this relay id, and
                                    // `HistoryChanged` only when a row it did
                                    // hold changed place. Three paths reach the
                                    // same relay id and the two facts keep them
                                    // apart: an Entry this device captured is
                                    // already a row and already announced, so
                                    // the relay's echo of one arrives here as a
                                    // repeat ingest that moved nothing —
                                    // announcing it would be a duplicate row on
                                    // screen, and reporting a reorder would cost
                                    // both shells a refetch for something that
                                    // did not happen. A **Use** is the repeat
                                    // ingest that *did* move the row. The
                                    // watermark advances for all of them: that
                                    // is about what has been *fetched*.
                                    //
                                    // The id on the event is the row's own, not
                                    // the relay's: the relay's does not cross
                                    // the facade, and both shells key their rows
                                    // on this one.
                                    let added = out.stored.first_insert.then(|| {
                                        Entry::new(
                                            out.stored.local_id,
                                            ctx.user_id.clone(),
                                            out.plaintext,
                                            created_at,
                                            last_use,
                                            device_id,
                                            device_label,
                                            // Delivered by the relay, so the
                                            // relay has it: this row owes
                                            // nothing and waits for nothing.
                                            Queued::Settled,
                                        )
                                    });
                                    (added, out.stored.moved)
                                }
                                Err(e) => {
                                    tracing::warn!(err = %e, "ingest failed");
                                    (None, false)
                                }
                            }
                        };
                        if let Some(entry) = added {
                            ctx.events.emit(CoreEvent::EntryAdded {
                                user_id: ctx.user_id.clone(), entry,
                            });
                        }
                        if reordered {
                            ctx.events.emit(CoreEvent::HistoryChanged {
                                user_id: ctx.user_id.clone(),
                            });
                        }
                    }
                    Some(sse::ServerEvent::Delete { id }) => {
                        // The frame names the relay's id; the shells know the
                        // row by its own, so it has to be resolved before the
                        // row is gone.
                        let local_id = {
                            let conn = ctx.state.conn.lock().await;
                            let local_id =
                                entries_cache::local_id_for(&conn, &ctx.user_id, id).ok().flatten();
                            if let Some(local_id) = local_id {
                                let _ = entries_cache::delete_one(&conn, &ctx.user_id, local_id);
                            }
                            local_id
                        };
                        if let Some(entry_id) = local_id {
                            ctx.events.emit(CoreEvent::EntryDeleted {
                                user_id: ctx.user_id.clone(), entry_id,
                            });
                        }
                    }
                }
            }
        }

        // SSE dropped. Surface the error if any, then back off and reconnect.
        let last_error = match sse_handle.await {
            Ok(Err(e)) => Some(e.to_string()),
            Ok(Ok(())) => None,
            Err(e) => Some(e.to_string()),
        };
        if cancel.is_cancelled() {
            return;
        }
        ctx.set_conn_state(ConnectionState::Connecting, last_error);
        let delay = backoff.next_delay_secs();
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tokio::time::sleep(std::time::Duration::from_secs(delay)) => {}
        }
    }
}

/// Drain the pending queue for as long as the session lives.
///
/// No user key: the uploader reconciles what the relay recorded onto rows that
/// already exist, and never decrypts anything.
async fn run_uploader(
    ctx: SessionCtx,
    transport: Arc<dyn SessionTransport>,
    cancel: CancellationToken,
    upload_trigger: Arc<Notify>,
) {
    let up = Uploader {
        user_id: ctx.user_id.clone(),
        conn: ctx.state.conn.clone(),
        transport: Arc::new(UploadVia(transport)),
        trigger: upload_trigger.clone(),
        events: ctx.events.clone(),
    };
    // Fire trigger once to flush whatever might already be queued from a previous run.
    upload_trigger.notify_one();
    // A rejected device token is a session-wide fact, not an uploader event: it
    // has to reach `set_conn_state` so Contact is flushed on the way out, which
    // is why the uploader reports it back rather than emitting it itself.
    if up.run(cancel).await == UploaderExit::AuthFailed {
        ctx.set_conn_state(ConnectionState::AuthFailed, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open_in_memory;
    use crate::testing::{unstorable_row, RecordingSink, ScriptedRelay, Wire};
    use rusqlite::Connection;

    fn paired(conn: &Connection) {
        accounts::upsert(
            conn,
            &accounts::Account {
                user_id: "u".into(), device_id: "d".into(), device_label: "mac".into(),
                server_url: "https://srv".into(), last_seen_seq: 0, created_at: 1,
                username: None, last_contact_at: None,
            },
        )
        .unwrap();
    }

    fn stored(conn: &Connection) -> Option<i64> {
        accounts::find(conn, "u").unwrap().unwrap().last_contact_at
    }

    /*
     * The bug this defends: a relay one version behind 404s `/me` forever, and
     * every entry from an unmirrored device re-probed it a minute apart. The
     * reason is worth stating once, not once a minute.
     */
    #[test]
    fn an_absent_me_route_is_reported_once_and_then_stops_being_probed() {
        let route = MirrorRoute::default();
        assert!(route.present(), "a fresh session must try the route");

        assert!(route.mark_absent(), "the first 404 is the one worth logging");
        assert!(!route.present(), "and no request may follow it");
        assert!(!route.mark_absent(), "a second 404 says nothing new");
        assert!(!route.mark_absent());
    }

    /*
     * Redeploying the relay is what drops the stream, so the reconnect that
     * follows is exactly when a route that was missing may have appeared.
     * Without the re-arm, an upgraded relay stays unmirrored until the app is
     * restarted.
     */
    #[test]
    fn reaching_the_relay_again_re_probes_a_route_that_was_missing() {
        let route = MirrorRoute::default();
        route.mark_absent();
        assert!(!route.present());

        route.rearm();
        assert!(route.present());
        assert!(route.mark_absent(), "and the reason is stated again for the new deployment");
    }

    #[test]
    fn heartbeats_move_the_cell_but_the_edge_out_of_online_does_the_writing() {
        let conn = open_in_memory().unwrap();
        paired(&conn);
        let cell = AtomicI64::new(0);

        // Three heartbeats. Each re-asserts Online, so nothing is flushed.
        for at in [1_000, 2_000, 3_000] {
            cell.store(at, Ordering::Relaxed);
            assert!(!should_persist_contact(Some(ConnectionState::Online), ConnectionState::Online));
        }
        assert_eq!(stored(&conn), None, "a healthy session must not touch the database");

        // The stream drops.
        assert!(should_persist_contact(Some(ConnectionState::Online), ConnectionState::Connecting));
        assert_eq!(flush_contact(&conn, "u", &cell).unwrap(), Some(3_000));
        assert_eq!(stored(&conn), Some(3_000), "the last reading, not the first");
    }

    #[test]
    fn a_session_that_never_heard_from_the_relay_writes_nothing() {
        let conn = open_in_memory().unwrap();
        paired(&conn);
        assert_eq!(flush_contact(&conn, "u", &AtomicI64::new(0)).unwrap(), None);
        assert_eq!(stored(&conn), None);
    }

    #[test]
    fn flushing_a_forgotten_account_is_an_error_not_a_silent_insert() {
        let conn = open_in_memory().unwrap();
        let cell = AtomicI64::new(42);
        assert!(matches!(flush_contact(&conn, "ghost", &cell), Err(AppError::NotFound(_))));
    }

    // ---------------------------------------------------------------------
    // The loop itself, driven with no relay.
    //
    // Every test here runs on a paused clock. That is not a speed trick: with
    // time paused tokio only advances it once every task is parked, so
    // `sleep` in a test is an exact barrier meaning "the session has consumed
    // everything it was given and is waiting for more".
    // ---------------------------------------------------------------------

    fn key() -> UserKey {
        crate::testing::test_user_key()
    }

    fn good_row(id: i64, plaintext: &str) -> EntryRow {
        crate::testing::encrypted_row(id, "u", plaintext, "d")
    }

    fn live_entry(id: i64, plaintext: &str, device_id: &str) -> sse::ServerEvent {
        crate::testing::live_entry(id, "u", plaintext, device_id)
    }

    fn ctx_over(conn: Connection) -> (SessionCtx, Arc<RecordingSink>) {
        let sink = Arc::new(RecordingSink::default());
        let state = SessionState::new(
            Arc::new(tokio::sync::Mutex::new(conn)),
            tokio::runtime::Handle::current(),
        );
        (SessionCtx::new(sink.clone(), state, "u".to_string()), sink)
    }

    async fn last_seen(ctx: &SessionCtx) -> i64 {
        let conn = ctx.state.conn.lock().await;
        accounts::find(&conn, "u").unwrap().unwrap().last_seen_seq
    }

    /// Run the loop until it is quiescent, then cancel it and wait for it to
    /// unwind — the lifecycle a shell puts it through, minus the relay.
    async fn drive(ctx: &SessionCtx, relay: Arc<ScriptedRelay>) {
        let cancel = CancellationToken::new();
        let task = tokio::spawn(run_sse_loop(
            ctx.clone(),
            relay,
            key(),
            cancel.clone(),
            Arc::new(Notify::new()),
        ));
        tokio::time::sleep(Duration::from_secs(600)).await;
        cancel.cancel();
        task.await.expect("the session loop must not panic");
    }

    /*
     * The headline: the whole protocol loop, with no Tauri, no app handle and
     * no relay — a fake keychain is not even needed down here, because nothing
     * below the facade unlocks a key. Everything it reports arrives through the
     * sink.
     */
    #[tokio::test(start_paused = true)]
    async fn the_session_loop_runs_against_fakes_with_no_relay() {
        let conn = open_in_memory().unwrap();
        paired(&conn);
        let (ctx, sink) = ctx_over(conn);
        let relay = ScriptedRelay::new(
            vec![Ok(vec![good_row(1, "from the backfill")])],
            vec![Wire::Holds(vec![live_entry(2, "live one", "d")])],
        );

        drive(&ctx, relay).await;

        assert_eq!(
            sink.connection_states(),
            vec![ConnectionState::Connecting, ConnectionState::Online],
            "connect once, go online, and stay there while the stream holds"
        );
        assert!(sink.saw_history_changed("u"), "the backfill wrote rows");
        assert_eq!(
            sink.entry_previews(),
            vec!["live one".to_string()],
            "a backfill reports one history change; only live frames emit entries"
        );
        assert_eq!(last_seen(&ctx).await, 2, "backfill then live frame, in order");
        assert!(
            ctx.state.last_contact.lock()["u"].load(Ordering::Relaxed) > 0,
            "the byte tap stamped Contact"
        );
    }

    /*
     * The invariant: `last_seen_seq` is a watermark, not a counter. A row that
     * would not store must leave it alone — otherwise the next `since=` fetch
     * starts past that row and it is lost for good.
     */
    #[tokio::test(start_paused = true)]
    async fn a_failed_ingest_does_not_advance_the_last_seen_seq() {
        let conn = open_in_memory().unwrap();
        paired(&conn);
        let (ctx, _sink) = ctx_over(conn);
        let relay = ScriptedRelay::new(
            vec![Ok(vec![
                good_row(1, "stored fine"),
                unstorable_row(2),
                good_row(3, "must be re-fetched"),
            ])],
            vec![Wire::Holds(Vec::new())],
        );

        drive(&ctx, relay).await;

        assert_eq!(
            last_seen(&ctx).await,
            1,
            "the watermark stops at the last row that actually stored"
        );
    }

    /*
     * A **Use** arrives as the row it always was, with a later Last Use and a
     * fresh sequence. Nothing was created, so no `EntryAdded`; the row moved,
     * so `HistoryChanged`. Both halves matter: announcing it would put a
     * duplicate on screen, and saying nothing would leave every other device
     * holding the right entries in the wrong order.
     */
    #[tokio::test(start_paused = true)]
    async fn a_use_arriving_over_sse_reorders_the_history_without_adding_a_row() {
        let conn = open_in_memory().unwrap();
        paired(&conn);
        let (ctx, sink) = ctx_over(conn);
        let captured = good_row(1, "ssh admin@10.0.0.4");
        let used = crate::testing::live_use(&captured, 99, captured.last_use + 60_000);
        let relay = ScriptedRelay::new(
            vec![Ok(vec![captured.clone()])],
            vec![Wire::Holds(vec![used])],
        );

        drive(&ctx, relay).await;

        assert!(
            sink.entry_previews().is_empty(),
            "a use creates nothing, so it announces nothing: {:?}",
            sink.entry_previews()
        );
        assert!(sink.saw_history_changed("u"), "but the reorder has to reach the shell");
        assert_eq!(last_seen(&ctx).await, 99, "and the watermark follows the new sequence");

        let conn = ctx.state.conn.lock().await;
        let head = entries_cache::list_recent(&conn, "u", None, 1).unwrap();
        assert_eq!(head[0].last_use, captured.last_use + 60_000);
        assert_eq!(head[0].created_at, captured.created_at, "a use leaves identity alone");
    }

    /*
     * The other repeat ingest, and the one that must *not* look like a use: the
     * relay echoing back an Entry this device uploaded and already cached. It
     * carries the same Last Use, so nothing moved — and a `HistoryChanged` here
     * would cost both shells a full refetch on every single capture.
     */
    #[tokio::test(start_paused = true)]
    async fn the_relays_echo_of_an_unchanged_entry_reports_no_reorder() {
        let conn = open_in_memory().unwrap();
        paired(&conn);
        let (ctx, sink) = ctx_over(conn);
        let row = good_row(1, "offered here");
        // Backfilled, then echoed verbatim on the stream.
        let echo = crate::testing::live_use(&row, row.seq, row.last_use);
        let relay = ScriptedRelay::new(vec![Ok(vec![row])], vec![Wire::Holds(vec![echo])]);

        drive(&ctx, relay).await;

        assert!(sink.entry_previews().is_empty(), "the echo adds nothing");
        assert_eq!(
            sink.history_changes("u"),
            1,
            "one history change, from the backfill that stored the row — not two"
        );
    }

    /*
     * The reconnect re-reads the watermark from the database rather than
     * carrying it in a local, so a stream that drops resumes from what was
     * ingested instead of replaying or skipping.
     */
    #[tokio::test(start_paused = true)]
    async fn every_reconnect_iteration_re_reads_the_last_seen_seq() {
        let conn = open_in_memory().unwrap();
        paired(&conn);
        let (ctx, _sink) = ctx_over(conn);
        let relay = ScriptedRelay::new(
            vec![Ok(vec![good_row(4, "first pass")]), Ok(vec![good_row(9, "second pass")])],
            vec![Wire::Drops(Vec::new()), Wire::Holds(Vec::new())],
        );

        drive(&ctx, relay.clone()).await;

        assert_eq!(
            relay.asked_since(),
            vec![0, 4],
            "each iteration asks from what the previous one ingested"
        );
        assert_eq!(last_seen(&ctx).await, 9);
    }

    /*
     * ADR 0001's latch, end to end. A relay one version behind 404s `/me`
     * forever; without the latch every entry from an unmirrored device probes
     * it again, so this session would have made three requests instead of two.
     */
    #[tokio::test(start_paused = true)]
    async fn a_404_on_me_latches_off_for_the_session_and_re_arms_on_the_next_backfill() {
        let conn = open_in_memory().unwrap();
        paired(&conn);
        let (ctx, _sink) = ctx_over(conn);
        let relay = ScriptedRelay::new(
            vec![Ok(Vec::new()), Ok(Vec::new())],
            vec![
                // An entry from a device the mirror has never heard of: the one
                // thing that provokes a mid-session `/me`.
                Wire::Drops(vec![live_entry(5, "from a stranger", "paired-since-last-me")]),
                Wire::Holds(Vec::new()),
            ],
        );

        drive(&ctx, relay.clone()).await;

        assert_eq!(
            relay.me_calls(),
            2,
            "one probe per reconnect: the entry from an unmirrored device found \
             the route latched off and made no request of its own"
        );
    }

    /*
     * Redeploying the relay is what drops the stream, so a backfill that
     * answers is exactly when a missing route may have appeared. Proven here at
     * the loop level rather than on the latch alone: the second `/me` above only
     * happens because the second backfill re-armed it.
     */
    #[tokio::test(start_paused = true)]
    async fn a_relay_that_gains_the_route_is_mirrored_without_an_app_restart() {
        let conn = open_in_memory().unwrap();
        paired(&conn);
        let (ctx, _sink) = ctx_over(conn);
        let relay = ScriptedRelay::new(
            vec![Ok(Vec::new()), Ok(Vec::new())],
            vec![Wire::Drops(Vec::new()), Wire::Holds(Vec::new())],
        )
        // First probe 404s; the redeployed relay answers the second.
        .answering_me(vec![
            Err(AppError::NotFound("no such route".into())),
            Ok(MeResp {
                user: crate::http::dto::UserDto { id: "u".into(), username: "alice".into() },
                devices: vec![crate::http::dto::DeviceDto {
                    device_id: "d".into(),
                    label: Some("MAC-STUDIO".into()),
                    created_at: 1,
                    revoked_at: None,
                }],
            }),
        ]);

        drive(&ctx, relay.clone()).await;

        let conn = ctx.state.conn.lock().await;
        assert_eq!(
            devices::map_for(&conn, "u").unwrap().get("d").map(String::as_str),
            Some("MAC-STUDIO"),
            "the reconnect picked up the upgraded relay's mirror"
        );
        assert_eq!(accounts::find(&conn, "u").unwrap().unwrap().username.as_deref(), Some("alice"));
    }
}
