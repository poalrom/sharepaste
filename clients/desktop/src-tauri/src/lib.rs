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
use tauri::menu::{ContextMenu, MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tauri::{Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder, WindowEvent};
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
            #[cfg(target_os = "macos")]
            spawn_clipboard_capture(app.handle().clone(), app_state.clone());
            register_initial_hotkey(app.handle().clone(), app_state.clone());
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
            commands::open_modal,
            commands::hide_popover,
        ])
        .run(tauri::generate_context!())
        .expect("run tauri");
}

fn build_tray(app: &mut tauri::App, _state: Arc<AppState>) -> tauri::Result<()> {
    // The tray icon itself is created from tauri.conf.json (`app.trayIcon`).
    // Building another one here would render a second, icon-less status item.
    // Look up the existing one and attach our menu + handlers to it.
    let tray = app
        .tray_by_id("main")
        .ok_or_else(|| tauri::Error::Io(std::io::Error::other("tray 'main' not found")))?;

    let menu = MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id("pair", "Pair device…").build(app)?)
        .item(&MenuItemBuilder::with_id("settings", "Settings…").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("quit", "Quit").build(app)?)
        .build()?;

    // tray-icon-0.23.1's TrayTarget intercepts mouseDown before NSStatusItem's
    // setMenu can fire, and the performClick(None) fallback doesn't actually
    // display the menu — neither click pops it. We display the menu manually
    // on right click via Menu::popup, so left click stays free for the popover
    // toggle and the tray-icon auto-popup machinery is left disabled.
    tray.set_show_menu_on_left_click(false)?;

    let menu_for_event = menu.clone();
    tray.on_menu_event(|app, event| match event.id.as_ref() {
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
    });
    tray.on_tray_icon_event(move |tray, ev| match ev {
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            rect,
            ..
        } => {
            let app = tray.app_handle();
            if let Some(state) = app.try_state::<Arc<AppState>>() {
                *state.last_tray_rect.lock() = Some(rect);
            }
            let _ = toggle_popover(app, Some(rect));
        }
        TrayIconEvent::Click {
            button: MouseButton::Right,
            button_state: MouseButtonState::Down,
            ..
        } => {
            let app = tray.app_handle();
            if let Some(win) = app.get_webview_window("popover") {
                let _ = menu_for_event.popup(win.as_ref().window());
            }
        }
        _ => {}
    });
    Ok(())
}

const POPOVER_W: f64 = 360.0;
const POPOVER_H: f64 = 480.0;

fn build_popover_window(app: &mut tauri::App) -> tauri::Result<()> {
    let win = WebviewWindowBuilder::new(app, "popover", WebviewUrl::App("popover.html".into()))
        .title("sharepaste")
        .inner_size(POPOVER_W, POPOVER_H)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .visible(false)
        .skip_taskbar(true)
        .build()?;
    let win_clone = win.clone();
    win.on_window_event(move |ev| {
        if let WindowEvent::Focused(false) = ev {
            let _ = win_clone.hide();
        }
    });
    Ok(())
}

fn toggle_popover(app: &tauri::AppHandle, tray_rect: Option<tauri::Rect>) -> tauri::Result<()> {
    let Some(win) = app.get_webview_window("popover") else {
        return Ok(());
    };
    if win.is_visible().unwrap_or(false) {
        win.hide()?;
        return Ok(());
    }
    let rect = tray_rect
        .or_else(|| {
            app.try_state::<Arc<AppState>>()
                .and_then(|s| *s.last_tray_rect.lock())
        })
        .or_else(query_tray_rect);
    if let Some(rect) = rect {
        if let Ok(scale) = win.scale_factor() {
            let pos = rect.position.to_physical::<f64>(scale);
            let size = rect.size.to_physical::<f64>(scale);
            let popover_w_phys = POPOVER_W * scale;
            let x_phys = pos.x + (size.width / 2.0) - (popover_w_phys / 2.0);
            let y_phys = pos.y + size.height + 4.0;
            // Move the underlying NSWindow synchronously via
            // `setFrameTopLeftPoint:` so the move lands before the next
            // paint. Tauri's set_position dispatches through wry's event
            // loop and races with show(), causing a one-frame flash at
            // the previous default location (center of screen).
            #[cfg(target_os = "macos")]
            {
                let x_logical = x_phys / scale;
                let y_logical = y_phys / scale;
                set_ns_window_top_left(&win, x_logical, y_logical);
            }
            // Also call Tauri's set_position so wry's cached position
            // matches the actual frame on subsequent operations.
            let _ = win.set_position(PhysicalPosition::new(x_phys, y_phys));
        }
    }
    win.show()?;
    win.set_focus()?;
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

    // Register the upload trigger up-front so the SSE task can notify it on each
    // successful (re)connect, even before the uploader task has started.
    let upload_trigger = std::sync::Arc::new(tokio::sync::Notify::new());
    state
        .upload_triggers
        .lock()
        .insert(user_id.clone(), upload_trigger.clone());

    let upload_trigger_for_sse = upload_trigger.clone();
    tauri::async_runtime::spawn(async move {
        let server = server_for_sse_task;
        let mut backoff = crate::core::sync::BackoffPlan::new();
        loop {
            if cancel2.is_cancelled() {
                return;
            }
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
                Err(crate::errors::AppError::Auth(s)) => {
                    let _ = app2.emit(
                        CONNECTION_STATE,
                        ConnectionStateEvent {
                            user_id: user_id_for_sse.clone(),
                            state: crate::core::sync::ConnectionState::AuthFailed,
                            last_error: Some(s),
                        },
                    );
                    return;
                }
                Err(e) => {
                    tracing::warn!(err = %e, "backfill failed; will retry");
                    let _ = app2.emit(
                        CONNECTION_STATE,
                        ConnectionStateEvent {
                            user_id: user_id_for_sse.clone(),
                            state: crate::core::sync::ConnectionState::Connecting,
                            last_error: Some(e.to_string()),
                        },
                    );
                    let delay = backoff.next_delay_secs();
                    tokio::select! {
                        _ = cancel2.cancelled() => return,
                        _ = tokio::time::sleep(std::time::Duration::from_secs(delay)) => {}
                    }
                    continue;
                }
            }

            let _ = app2.emit(
                CONNECTION_STATE,
                ConnectionStateEvent {
                    user_id: user_id_for_sse.clone(),
                    state: crate::core::sync::ConnectionState::Online,
                    last_error: None,
                },
            );
            backoff.reset();
            // Server reachable again — push any queued entries.
            upload_trigger_for_sse.notify_one();

            let (tx, mut rx) = mpsc::channel::<crate::core::sync::sse::ServerEvent>(64);
            let server_for_sse = server.clone();
            let cancel_for_sse = cancel2.clone();
            let sse_handle = tokio::spawn(async move {
                crate::core::sync::sse::run(server_for_sse, tx, cancel_for_sse).await
            });

            'recv: loop {
                tokio::select! {
                    _ = cancel2.cancelled() => return,
                    ev = rx.recv() => match ev {
                        None => break 'recv,
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

            // SSE dropped. Surface the error if any, then back off and reconnect.
            let last_error = match sse_handle.await {
                Ok(Err(e)) => Some(e.to_string()),
                Ok(Ok(())) => None,
                Err(e) => Some(e.to_string()),
            };
            if cancel2.is_cancelled() {
                return;
            }
            let _ = app2.emit(
                CONNECTION_STATE,
                ConnectionStateEvent {
                    user_id: user_id_for_sse.clone(),
                    state: crate::core::sync::ConnectionState::Connecting,
                    last_error,
                },
            );
            let delay = backoff.next_delay_secs();
            tokio::select! {
                _ = cancel2.cancelled() => return,
                _ = tokio::time::sleep(std::time::Duration::from_secs(delay)) => {}
            }
        }
    });

    // Pending-queue uploader on its own task.
    let conn_for_upload = state.conn.clone();
    let app_for_upload = app.clone();
    let cancel3 = cancel.clone();
    let user_id2 = user_id.clone();
    let upload_trigger_for_task = upload_trigger.clone();
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
        let up = Uploader {
            user_id: user_id2.clone(),
            conn: conn_for_upload,
            transport: std::sync::Arc::new(ServerUpload(server_for_upload)),
            trigger: upload_trigger_for_task.clone(),
            events,
        };
        // Fire trigger once to flush whatever might already be queued from a previous run.
        upload_trigger_for_task.notify_one();
        up.run(cancel3).await;
    });
}

#[cfg(target_os = "macos")]
fn spawn_clipboard_capture(app: tauri::AppHandle, state: Arc<AppState>) {
    use crate::core::capture::filter::{evaluate, CaptureContext, FilterDecision, PasteboardSniff};
    use crate::core::capture::macos::{frontmost_bundle_id, NSPasteboardSniffer};
    use crate::core::capture::watcher;

    let (tx, mut rx) = mpsc::channel::<crate::core::capture::watcher::ClipboardEvent>(32);
    let watcher_cancel = CancellationToken::new();
    if let Err(e) = watcher::spawn(tx, watcher_cancel.clone()) {
        tracing::error!(err = %e, "clipboard watcher failed to start");
        return;
    }
    tauri::async_runtime::spawn(async move {
        let sniffer = NSPasteboardSniffer::new();
        while let Some(_ev) = rx.recv().await {
            let Some(user_id) = state.registry.active_user_id() else {
                continue;
            };
            let settings = {
                let conn = state.conn.lock().await;
                match crate::core::storage::settings::load(&conn) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(err = %e, "load settings");
                        continue;
                    }
                }
            };
            let frontmost = frontmost_bundle_id();
            let last_self = state.last_self_write.lock().clone();
            let last_self_ref = last_self
                .as_ref()
                .map(|(t, s)| (*t, s.as_str()));
            let ctx = CaptureContext {
                capture_enabled: settings.capture_enabled,
                deny_list: &settings.deny_list,
                frontmost_bundle_id: frontmost.as_deref(),
                last_self_write: last_self_ref,
            };
            let decision = evaluate(&ctx, &sniffer as &dyn PasteboardSniff, std::time::Instant::now());
            let text = match decision {
                FilterDecision::Capture(t) => t,
                FilterDecision::Skip(reason) => {
                    tracing::debug!(?reason, "clipboard skip");
                    continue;
                }
            };
            let m = match state.registry.load_active_membership(&user_id).await {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!(err = %e, "no active membership for capture");
                    continue;
                }
            };
            let ciphertext = match crate::core::crypto::encrypt(&m.user_key, &user_id, text.as_bytes()) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(err = %e, "encrypt failed");
                    continue;
                }
            };
            {
                let conn = state.conn.lock().await;
                if let Err(e) = crate::core::storage::pending::enqueue(
                    &conn,
                    &user_id,
                    &ciphertext,
                    now_ms(),
                ) {
                    tracing::warn!(err = %e, "enqueue failed");
                    continue;
                }
                let count = crate::core::storage::pending::count(&conn, &user_id).unwrap_or(0);
                let _ = app.emit(
                    PENDING_COUNT,
                    PendingCount {
                        user_id: user_id.clone(),
                        count,
                    },
                );
            }
            if let Some(trigger) = state.upload_triggers.lock().get(&user_id).cloned() {
                trigger.notify_one();
            } else {
                tracing::warn!(%user_id, "no uploader trigger registered");
            }
        }
        let _ = watcher_cancel;
    });
}

fn register_initial_hotkey(app: tauri::AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let hotkey = {
            let conn = state.conn.lock().await;
            crate::core::storage::settings::load(&conn)
                .map(|s| s.hotkey)
                .unwrap_or(None)
        };
        if let Some(h) = hotkey {
            if let Err(e) = apply_hotkey(&app, Some(&h)) {
                tracing::warn!(err = %e, hotkey = %h, "register global shortcut failed");
            }
        }
    });
}

pub fn apply_hotkey(app: &tauri::AppHandle, hotkey: Option<&str>) -> tauri::Result<()> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutEvent, ShortcutState};
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let Some(h) = hotkey else { return Ok(()) };
    if h.trim().is_empty() {
        return Ok(());
    }
    gs.on_shortcut(h, |app: &tauri::AppHandle, _shortcut, event: ShortcutEvent| {
        if event.state() == ShortcutState::Pressed {
            let app_clone = app.clone();
            let _ = app.run_on_main_thread(move || {
                let _ = toggle_popover(&app_clone, None);
            });
        }
    })
    .map_err(|e| {
        tauri::Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("register shortcut: {e}"),
        ))
    })?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn query_tray_rect() -> Option<tauri::Rect> {
    crate::core::capture::macos::find_tray_rect()
}

#[cfg(target_os = "macos")]
fn set_ns_window_top_left(win: &tauri::WebviewWindow, x_logical: f64, y_logical: f64) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2::Encode;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSPoint {
        x: f64,
        y: f64,
    }
    unsafe impl Encode for NSPoint {
        const ENCODING: objc2::Encoding = objc2::Encoding::Struct(
            "CGPoint",
            &[objc2::Encoding::Double, objc2::Encoding::Double],
        );
    }

    let Ok(raw) = win.ns_window() else { return };
    if raw.is_null() {
        return;
    }
    // setFrameTopLeftPoint takes a screen point in logical coordinates with
    // y measured from the bottom of the screen. We were given a top-left
    // (Tauri-style) y; convert by flipping against the primary screen.
    use objc2_app_kit::NSScreen;
    use objc2_foundation::MainThreadMarker;
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some(screen) = NSScreen::mainScreen(mtm) else { return };
    let screen_h = screen.frame().size.height;
    let cocoa_y = screen_h - y_logical;
    let p = NSPoint { x: x_logical, y: cocoa_y };
    let ns_window = raw as *mut AnyObject;
    unsafe {
        let _: () = msg_send![ns_window, setFrameTopLeftPoint: p];
    }
}

#[cfg(not(target_os = "macos"))]
fn query_tray_rect() -> Option<tauri::Rect> {
    None
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
