//! One sync session per active account.
//!
//! A session owns two long-lived tasks that share a cancellation token: the SSE
//! loop, which backfills missed entries and then streams live ones, and the
//! uploader, which drains the pending queue. Both report their progress to the
//! UI through the connection-state and entry events.

use crate::core::crypto::UserKey;
use crate::core::http::dto::EntryRow;
use crate::core::http::ServerClient;
use crate::core::storage::{accounts, entries_cache};
use crate::core::sync::uploader::{UploadTransport, Uploader, UploaderEvents};
use crate::core::sync::{decryptor, sse, BackoffPlan, ConnectionState};
use crate::errors::AppError;
use crate::events::{
    ConnectionStateEvent, DecryptionError, EntryAdded, EntryDeleted, EntryView, PendingCount,
    CONNECTION_STATE, DECRYPTION_ERROR, ENTRY_ADDED, ENTRY_DELETED, HISTORY_CHANGED, PENDING_COUNT,
};
use crate::state::{AppState, SyncSlot};
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::{mpsc, Notify};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

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
/// global state, and the account the session belongs to.
#[derive(Clone)]
struct SessionCtx {
    app: tauri::AppHandle,
    state: Arc<AppState>,
    user_id: String,
}

impl SessionCtx {
    /// Record a connection-state transition and tell the UI about it.
    fn set_conn_state(&self, new_state: ConnectionState, last_error: Option<String>) {
        self.state
            .conn_states
            .lock()
            .insert(self.user_id.clone(), new_state);
        let _ = self.app.emit(
            CONNECTION_STATE,
            ConnectionStateEvent {
                user_id: self.user_id.clone(),
                state: new_state,
                last_error,
            },
        );
    }
}

/// Start the sync session for `user_id`, cancelling whichever one it already had.
pub(crate) async fn run_session(app: tauri::AppHandle, state: Arc<AppState>, user_id: String) {
    let cancel = CancellationToken::new();
    {
        let mut tasks = state.sync_tasks.lock();
        if let Some(prev) = tasks.insert(
            user_id.clone(),
            SyncSlot {
                user_id: user_id.clone(),
                cancel: cancel.clone(),
            },
        ) {
            prev.cancel.cancel();
        }
    }
    let ctx = SessionCtx {
        app,
        state,
        user_id,
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

        ctx.set_conn_state(ConnectionState::Online, None);
        backoff.reset();
        // Server reachable again — push any queued entries.
        upload_trigger.notify_one();

        let (tx, mut rx) = mpsc::channel::<sse::ServerEvent>(64);
        let server_for_sse = server.clone();
        let cancel_for_sse = cancel.clone();
        let sse_handle = tokio::spawn(async move { sse::run(server_for_sse, tx, cancel_for_sse).await });

        'recv: loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                ev = rx.recv() => match ev {
                    None => break 'recv,
                    Some(sse::ServerEvent::Entry { id, ciphertext, created_at, device_id }) => {
                        let row = EntryRow { id, ciphertext, created_at, device_id: device_id.clone() };
                        let conn = ctx.state.conn.lock().await;
                        match decryptor::ingest(&conn, &user_key, &ctx.user_id, &row, crate::now_ms()) {
                            Ok(out) => {
                                let _ = accounts::set_last_seen(&conn, &ctx.user_id, id);
                                let _ = ctx.app.emit(ENTRY_ADDED, EntryAdded {
                                    user_id: ctx.user_id.clone(),
                                    entry: EntryView {
                                        id, user_id: ctx.user_id.clone(),
                                        preview: out.plaintext_preview.unwrap_or_default(),
                                        created_at, device_id, device_label: None,
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
