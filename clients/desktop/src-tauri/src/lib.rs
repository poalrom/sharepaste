pub mod config;
pub mod errors;
pub mod logging;
pub mod state;
pub mod events;
pub mod commands;
pub mod core;

use crate::config::Paths;
use crate::core::account::AccountRegistry;
use crate::core::keychain::SystemKeychain;
use crate::core::storage::open as open_storage;
use crate::events::{
    ConnectionStateEvent, EntryAdded, EntryView, PendingCount, ACTIVE_CHANGED,
    CONNECTION_STATE, ENTRY_ADDED, ENTRY_DELETED, HISTORY_CHANGED, PENDING_COUNT,
};
use crate::state::AppState;
use std::sync::Arc;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

pub fn launch() {
    let paths = Paths::resolve();
    paths.ensure_dirs().expect("create app dirs");
    let _log_guard = logging::init(&paths.log_dir);
    let conn = open_storage(&paths.db_path).expect("open sqlite");
    let conn = Arc::new(tokio::sync::Mutex::new(conn));
    let keychain: Arc<dyn core::keychain::Keychain> = Arc::new(SystemKeychain::default());
    let registry = Arc::new(AccountRegistry::new(conn.clone(), keychain.clone()));
    let app_state = Arc::new(AppState::new(paths, conn, keychain, registry));

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(app_state.clone())
        .setup(move |app| {
            build_tray(app, app_state.clone())?;
            build_popover_window(app)?;
            spawn_sync_for_existing_accounts(app.handle().clone(), app_state.clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_accounts,
            commands::pair_with_invite,
            commands::pair_start,
            commands::pair_with_code,
            commands::forget_account,
            commands::revoke_device,
            commands::set_active_account,
            commands::list_history,
            commands::search_history,
            commands::get_entry_full,
            commands::copy_to_clipboard,
            commands::delete_entry,
            commands::clear_history,
            commands::get_settings,
            commands::update_settings,
            commands::get_status,
        ])
        .run(tauri::generate_context!())
        .expect("run tauri");
}

fn build_tray(app: &mut tauri::App, _state: Arc<AppState>) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id("show", "Show history").build(app)?)
        .item(&MenuItemBuilder::with_id("pair", "Pair device…").build(app)?)
        .item(&MenuItemBuilder::with_id("settings", "Settings…").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("quit", "Quit").build(app)?)
        .build()?;

    let _tray = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                let _ = toggle_popover(app);
            }
            "pair" => {
                let _ = open_modal(app, "pairing");
            }
            "settings" => {
                let _ = open_modal(app, "settings");
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if matches!(
                ev,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                let _ = toggle_popover(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn build_popover_window(app: &mut tauri::App) -> tauri::Result<()> {
    let _ = WebviewWindowBuilder::new(app, "popover", WebviewUrl::App("popover.html".into()))
        .title("sharepaste")
        .inner_size(360.0, 480.0)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .visible(false)
        .skip_taskbar(true)
        .build()?;
    Ok(())
}

fn toggle_popover(app: &tauri::AppHandle) -> tauri::Result<()> {
    if let Some(win) = app.get_webview_window("popover") {
        if win.is_visible().unwrap_or(false) {
            win.hide()?;
        } else {
            win.show()?;
            win.set_focus()?;
        }
    }
    Ok(())
}

fn open_modal(app: &tauri::AppHandle, kind: &str) -> tauri::Result<()> {
    let label = format!("modal-{kind}");
    if let Some(existing) = app.get_webview_window(&label) {
        existing.set_focus()?;
        return Ok(());
    }
    let url = format!("modal.html?kind={kind}");
    let win = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(url.into()))
        .title("sharepaste")
        .inner_size(420.0, 520.0)
        .resizable(false)
        .build()?;
    let _ = win;
    Ok(())
}

fn spawn_sync_for_existing_accounts(app: tauri::AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let accounts = state.registry.list().await.unwrap_or_default();
        if let Some(first) = accounts.first() {
            state.registry.set_active(Some(first.user_id.clone()));
            let _ = app.emit(
                ACTIVE_CHANGED,
                crate::events::ActiveChanged {
                    user_id: Some(first.user_id.clone()),
                },
            );
            spawn_sync(app.clone(), state.clone(), first.user_id.clone()).await;
        }
    });
}

pub async fn spawn_sync(app: tauri::AppHandle, state: Arc<AppState>, user_id: String) {
    let cancel = CancellationToken::new();
    {
        let mut tasks = state.sync_tasks.lock();
        if let Some(prev) = tasks.insert(
            user_id.clone(),
            crate::state::SyncSlot {
                user_id: user_id.clone(),
                cancel: cancel.clone(),
            },
        ) {
            prev.cancel.cancel();
        }
    }
    let m = match state.registry.load_active_membership(&user_id).await {
        Ok(m) => m,
        Err(e) => {
            let _ = app.emit(
                CONNECTION_STATE,
                ConnectionStateEvent {
                    user_id: user_id.clone(),
                    state: crate::core::sync::ConnectionState::AuthFailed,
                    last_error: Some(e.to_string()),
                },
            );
            return;
        }
    };
    let server_for_sse_task = m.server.clone();
    let server_for_upload = m.server.clone();
    // UserKey is Zeroizing<[u8;32]> with no Clone; clone the inner array via a fresh
    // Zeroizing wrapper so each spawned task owns its own key.
    let user_key_for_sse: crate::core::crypto::UserKey = Zeroizing::new(*m.user_key);
    let app2 = app.clone();
    let state2 = state.clone();
    let cancel2 = cancel.clone();
    let user_id_for_sse = user_id.clone();

    tauri::async_runtime::spawn(async move {
        let server = server_for_sse_task;
        // Connecting → backfill → Online → SSE; on drop reconnect with backoff.
        let _ = app2.emit(
            CONNECTION_STATE,
            ConnectionStateEvent {
                user_id: user_id_for_sse.clone(),
                state: crate::core::sync::ConnectionState::Connecting,
                last_error: None,
            },
        );
        let last_seen = {
            let conn = state2.conn.lock().await;
            crate::core::storage::accounts::find(&conn, &user_id_for_sse)
                .ok()
                .flatten()
                .map(|a| a.last_seen_id)
                .unwrap_or(0)
        };
        match server.list_entries(last_seen, 500).await {
            Ok(rows) => {
                let conn = state2.conn.lock().await;
                let mut new_last = last_seen;
                for row in rows {
                    let _ = crate::core::sync::decryptor::ingest(
                        &conn,
                        &user_key_for_sse,
                        &user_id_for_sse,
                        &row,
                        now_ms(),
                    );
                    if row.id > new_last {
                        new_last = row.id;
                    }
                }
                if new_last != last_seen {
                    let _ = crate::core::storage::accounts::set_last_seen(
                        &conn,
                        &user_id_for_sse,
                        new_last,
                    );
                    let _ = app2.emit(
                        HISTORY_CHANGED,
                        serde_json::json!({ "user_id": user_id_for_sse }),
                    );
                }
            }
            Err(e) => tracing::warn!(err = %e, "backfill failed"),
        }

        let _ = app2.emit(
            CONNECTION_STATE,
            ConnectionStateEvent {
                user_id: user_id_for_sse.clone(),
                state: crate::core::sync::ConnectionState::Online,
                last_error: None,
            },
        );

        let (tx, mut rx) = mpsc::channel::<crate::core::sync::sse::ServerEvent>(64);
        let server_for_sse = server.clone();
        let cancel_for_sse = cancel2.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::core::sync::sse::run(server_for_sse, tx, cancel_for_sse).await {
                tracing::warn!(err = %e, "sse loop exited");
            }
        });

        loop {
            tokio::select! {
                _ = cancel2.cancelled() => return,
                ev = rx.recv() => match ev {
                    None => return,
                    Some(crate::core::sync::sse::ServerEvent::Entry { id, ciphertext, created_at, device_id }) => {
                        let row = crate::core::http::dto::EntryRow { id, ciphertext, created_at, device_id: device_id.clone() };
                        let conn = state2.conn.lock().await;
                        match crate::core::sync::decryptor::ingest(&conn, &user_key_for_sse, &user_id_for_sse, &row, now_ms()) {
                            Ok(out) => {
                                let _ = crate::core::storage::accounts::set_last_seen(&conn, &user_id_for_sse, id);
                                let _ = app2.emit(ENTRY_ADDED, EntryAdded {
                                    user_id: user_id_for_sse.clone(),
                                    entry: EntryView {
                                        id, user_id: user_id_for_sse.clone(),
                                        preview: out.plaintext_preview.unwrap_or_default(),
                                        created_at, device_id, device_label: None,
                                    },
                                });
                                if out.undecryptable {
                                    let _ = app2.emit(crate::events::DECRYPTION_ERROR, crate::events::DecryptionError {
                                        user_id: user_id_for_sse.clone(), entry_id: id,
                                    });
                                }
                            }
                            Err(e) => tracing::warn!(err = %e, "ingest failed"),
                        }
                    }
                    Some(crate::core::sync::sse::ServerEvent::Delete { id }) => {
                        let conn = state2.conn.lock().await;
                        let _ = crate::core::storage::entries_cache::delete_one(&conn, &user_id_for_sse, id);
                        let _ = app2.emit(ENTRY_DELETED, crate::events::EntryDeleted {
                            user_id: user_id_for_sse.clone(), entry_id: id,
                        });
                    }
                }
            }
        }
    });

    // Pending-queue uploader on its own task.
    let conn_for_upload = state.conn.clone();
    let app_for_upload = app.clone();
    let cancel3 = cancel.clone();
    let user_id2 = user_id.clone();
    tauri::async_runtime::spawn(async move {
        use crate::core::sync::uploader::{UploadTransport, Uploader, UploaderEvents};
        struct ServerUpload(crate::core::http::ServerClient);
        #[async_trait::async_trait]
        impl UploadTransport for ServerUpload {
            async fn upload(&self, b64: &str) -> Result<i64, crate::errors::AppError> {
                self.0.post_entry(b64).await.map(|r| r.id)
            }
        }
        let app_pc = app_for_upload.clone();
        let app_af = app_for_upload.clone();
        let user_pc = user_id2.clone();
        let user_af = user_id2.clone();
        let events = UploaderEvents {
            on_pending_count: Box::new(move |n| {
                let _ = app_pc.emit(
                    PENDING_COUNT,
                    PendingCount {
                        user_id: user_pc.clone(),
                        count: n,
                    },
                );
            }),
            on_auth_failed: Box::new(move || {
                let _ = app_af.emit(
                    CONNECTION_STATE,
                    ConnectionStateEvent {
                        user_id: user_af.clone(),
                        state: crate::core::sync::ConnectionState::AuthFailed,
                        last_error: None,
                    },
                );
            }),
        };
        let trigger = std::sync::Arc::new(tokio::sync::Notify::new());
        let up = Uploader {
            user_id: user_id2.clone(),
            conn: conn_for_upload,
            transport: std::sync::Arc::new(ServerUpload(server_for_upload)),
            trigger: trigger.clone(),
            events,
        };
        // Fire trigger once to flush whatever might already be queued from a previous run.
        trigger.notify_one();
        up.run(cancel3).await;
    });
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
