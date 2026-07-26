//! One sync session per active account.
//!
//! A session owns two long-lived tasks that share a cancellation token: the SSE
//! loop, which backfills missed entries and then streams live ones, and the
//! uploader, which drains the pending queue. Both report their progress to the
//! UI through the connection-state and entry events.

use crate::core::crypto::UserKey;
use crate::core::http::dto::EntryRow;
use crate::core::http::ServerClient;
use crate::core::storage::devices::DeviceRecord;
use crate::core::storage::{accounts, devices, entries_cache};
use crate::core::sync::state::should_persist_contact;
use crate::core::sync::uploader::{UploadTransport, Uploader, UploaderEvents};
use crate::core::sync::{decryptor, sse, BackoffPlan, ConnectionState};
use crate::errors::AppError;
use crate::events::{
    ConnectionStateEvent, DecryptionError, EntryAdded, EntryDeleted, EntryView, PendingCount,
    ContactEvent, CONNECTION_STATE, DECRYPTION_ERROR, ENTRY_ADDED, ENTRY_DELETED,
    HISTORY_CHANGED, PENDING_COUNT, CONTACT_EVENT,
};
use crate::state::AppState;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::Emitter;
use tokio::sync::{mpsc, Notify};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

/// How long an entry from an unmirrored device is allowed to provoke a
/// `GET /me`. Without it, a relay that never labels a device would turn every
/// entry from that device into a round trip; one refresh a minute is still
/// prompt enough to pick up a device paired mid-session.
const MIRROR_REFRESH_DEBOUNCE: Duration = Duration::from_secs(60);

/// Uploads a queued entry over the account's authenticated connection.
///
/// `uploader.rs` only knows the `UploadTransport` shape, which keeps the HTTP
/// client out of its tests; this is the one production implementation.
struct ServerUpload(ServerClient);

#[async_trait::async_trait]
impl UploadTransport for ServerUpload {
    async fn upload(&self, b64: &str) -> Result<i64, AppError> {
        self.0.post_entry(b64).await.map(|r| r.id)
    }
}

/// The handles both session tasks share: the Tauri app to emit through, the
/// global state, the account the session belongs to, and the Device mirror's
/// refresh clock.
#[derive(Clone)]
struct SessionCtx {
    app: tauri::AppHandle,
    state: Arc<AppState>,
    user_id: String,
    mirror_refreshed_at: Arc<Mutex<Option<Instant>>>,
}

impl SessionCtx {
    /// Record a connection-state transition and tell the UI about it.
    ///
    /// Contact is flushed here, on the edge out of `Online` and nowhere else —
    /// see [`should_persist_contact`]. Every transition in the session passes
    /// through this one method, so a future caller cannot forget it.
    fn set_conn_state(&self, new_state: ConnectionState, last_error: Option<String>) {
        let prev = self
            .state
            .conn_states
            .lock()
            .insert(self.user_id.clone(), new_state);
        if should_persist_contact(prev, new_state) {
            self.persist_contact();
        }
        let _ = self.app.emit(
            CONNECTION_STATE,
            ConnectionStateEvent {
                user_id: self.user_id.clone(),
                state: new_state,
                last_error,
            },
        );
    }

    /// Write the live Contact reading to the database and tell the UI,
    /// off-thread because the database sits behind an async lock.
    fn persist_contact(&self) {
        let Some(cell) = self.state.last_contact.lock().get(&self.user_id).cloned() else {
            return; // no session ever held a cell for this user
        };
        let ctx = self.clone();
        tauri::async_runtime::spawn(async move {
            let at = {
                let conn = ctx.state.conn.lock().await;
                match flush_contact(&conn, &ctx.user_id, &cell) {
                    Ok(Some(at)) => at,
                    Ok(None) => return,
                    Err(e) => {
                        tracing::warn!(err = %e, "persisting contact failed");
                        return;
                    }
                }
            };
            let _ = ctx.app.emit(
                CONTACT_EVENT,
                ContactEvent { user_id: ctx.user_id.clone(), last_contact_at: Some(at) },
            );
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
/// session that cannot label its Origins is still a working session.
async fn mirror_me(ctx: &SessionCtx, server: &ServerClient) {
    let me = match server.me().await {
        Ok(me) => me,
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
async fn refresh_mirror_if_unknown(ctx: &SessionCtx, server: &ServerClient, device_id: &str) {
    let known = {
        let conn = ctx.state.conn.lock().await;
        devices::is_mirrored(&conn, &ctx.user_id, device_id).unwrap_or(true)
    };
    if !known && ctx.claim_mirror_refresh() {
        mirror_me(ctx, server).await;
    }
}

/// Start the sync session for `user_id`, cancelling whichever one it already had.
pub(crate) async fn run_session(app: tauri::AppHandle, state: Arc<AppState>, user_id: String) {
    let cancel = CancellationToken::new();
    {
        let mut tasks = state.sync_tasks.lock();
        if let Some(prev) = tasks.insert(user_id.clone(), cancel.clone()) {
            prev.cancel();
        }
    }
    let ctx = SessionCtx {
        app,
        state,
        user_id,
        mirror_refreshed_at: Arc::new(Mutex::new(None)),
    };
    let m = match ctx
        .state
        .registry
        .load_active_membership(&ctx.user_id)
        .await
    {
        Ok(m) => m,
        Err(e) => {
            ctx.set_conn_state(ConnectionState::AuthFailed, Some(e.to_string()));
            return;
        }
    };
    // UserKey is Zeroizing<[u8;32]> with no Clone; clone the inner array via a fresh
    // Zeroizing wrapper so each spawned task owns its own key.
    let user_key: UserKey = Zeroizing::new(*m.user_key);

    // Register the upload trigger up-front so the SSE task can notify it on each
    // successful (re)connect, even before the uploader task has started.
    let upload_trigger = Arc::new(Notify::new());
    ctx.state
        .upload_triggers
        .lock()
        .insert(ctx.user_id.clone(), upload_trigger.clone());

    tauri::async_runtime::spawn(run_sse_loop(
        ctx.clone(),
        m.server.clone(),
        user_key,
        cancel.clone(),
        upload_trigger.clone(),
    ));
    // Pending-queue uploader on its own task.
    tauri::async_runtime::spawn(run_uploader(ctx, m.server, cancel, upload_trigger));
}

/// Backfill, then stream, for as long as the session lives.
///
/// Every iteration re-reads `last_seen_id` from the database, so a stream that
/// drops resumes from the last entry actually ingested rather than replaying or
/// skipping. A failed backfill retries on the shared backoff; a successful one
/// resets it and marks the account online.
async fn run_sse_loop(
    ctx: SessionCtx,
    server: ServerClient,
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
        let last_seen = {
            let conn = ctx.state.conn.lock().await;
            accounts::find(&conn, &ctx.user_id)
                .ok()
                .flatten()
                .map(|a| a.last_seen_id)
                .unwrap_or(0)
        };
        match server.list_entries(last_seen, 500).await {
            Ok(rows) => {
                // The backfill window: bytes arrived here too, and the SSE tap
                // has not opened yet.
                contact.store(crate::now_ms(), Ordering::Relaxed);
                let conn = ctx.state.conn.lock().await;
                let mut new_last = last_seen;
                for row in rows {
                    let _ = decryptor::ingest(
                        &conn,
                        &user_key,
                        &ctx.user_id,
                        &row,
                        crate::now_ms(),
                    );
                    if row.id > new_last {
                        new_last = row.id;
                    }
                }
                if new_last != last_seen {
                    let _ = accounts::set_last_seen(&conn, &ctx.user_id, new_last);
                    let _ = ctx
                        .app
                        .emit(HISTORY_CHANGED, serde_json::json!({ "user_id": ctx.user_id }));
                }
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

        mirror_me(&ctx, &server).await;
        ctx.set_conn_state(ConnectionState::Online, None);
        backoff.reset();
        // Server reachable again — push any queued entries.
        upload_trigger.notify_one();

        let (tx, mut rx) = mpsc::channel::<sse::ServerEvent>(64);
        let server_for_sse = server.clone();
        let cancel_for_sse = cancel.clone();
        let contact_for_sse = contact.clone();
        let sse_handle = tokio::spawn(async move {
            sse::run(server_for_sse, tx, cancel_for_sse, contact_for_sse).await
        });

        'recv: loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                ev = rx.recv() => match ev {
                    None => break 'recv,
                    Some(sse::ServerEvent::Entry { id, ciphertext, created_at, device_id }) => {
                        refresh_mirror_if_unknown(&ctx, &server, &device_id).await;
                        let row = EntryRow { id, ciphertext, created_at, device_id: device_id.clone() };
                        let conn = ctx.state.conn.lock().await;
                        match decryptor::ingest(&conn, &user_key, &ctx.user_id, &row, crate::now_ms()) {
                            Ok(out) => {
                                let _ = accounts::set_last_seen(&conn, &ctx.user_id, id);
                                let device_label = devices::map_for(&conn, &ctx.user_id)
                                    .unwrap_or_default()
                                    .remove(&device_id);
                                let _ = ctx.app.emit(ENTRY_ADDED, EntryAdded {
                                    user_id: ctx.user_id.clone(),
                                    entry: EntryView {
                                        id, user_id: ctx.user_id.clone(),
                                        preview: out.plaintext_preview.unwrap_or_default(),
                                        created_at, device_id, device_label,
                                    },
                                });
                                if out.undecryptable {
                                    let _ = ctx.app.emit(DECRYPTION_ERROR, DecryptionError {
                                        user_id: ctx.user_id.clone(), entry_id: id,
                                    });
                                }
                            }
                            Err(e) => tracing::warn!(err = %e, "ingest failed"),
                        }
                    }
                    Some(sse::ServerEvent::Delete { id }) => {
                        let conn = ctx.state.conn.lock().await;
                        let _ = entries_cache::delete_one(&conn, &ctx.user_id, id);
                        let _ = ctx.app.emit(ENTRY_DELETED, EntryDeleted {
                            user_id: ctx.user_id.clone(), entry_id: id,
                        });
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
async fn run_uploader(
    ctx: SessionCtx,
    server: ServerClient,
    cancel: CancellationToken,
    upload_trigger: Arc<Notify>,
) {
    let ctx_pending = ctx.clone();
    let ctx_auth = ctx.clone();
    let events = UploaderEvents {
        on_pending_count: Box::new(move |n| {
            let _ = ctx_pending.app.emit(
                PENDING_COUNT,
                PendingCount {
                    user_id: ctx_pending.user_id.clone(),
                    count: n,
                },
            );
        }),
        on_auth_failed: Box::new(move || {
            ctx_auth.set_conn_state(ConnectionState::AuthFailed, None);
        }),
    };
    let up = Uploader {
        user_id: ctx.user_id.clone(),
        conn: ctx.state.conn.clone(),
        transport: Arc::new(ServerUpload(server)),
        trigger: upload_trigger.clone(),
        events,
    };
    // Fire trigger once to flush whatever might already be queued from a previous run.
    upload_trigger.notify_one();
    up.run(cancel).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::storage::open_in_memory;
    use rusqlite::Connection;

    fn paired(conn: &Connection) {
        accounts::upsert(
            conn,
            &accounts::Account {
                user_id: "u".into(), device_id: "d".into(), device_label: "mac".into(),
                server_url: "https://srv".into(), last_seen_id: 0, created_at: 1,
                username: None, last_contact_at: None,
            },
        )
        .unwrap();
    }

    fn stored(conn: &Connection) -> Option<i64> {
        accounts::find(conn, "u").unwrap().unwrap().last_contact_at
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
}
