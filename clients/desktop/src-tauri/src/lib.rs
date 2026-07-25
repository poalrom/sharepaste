pub mod config;
pub mod errors;
pub mod logging;
pub mod state;
pub mod events;
pub mod commands;
pub mod core;
mod popover;

use crate::config::Paths;
use crate::core::account::AccountRegistry;
use crate::core::keychain::SystemKeychain;
use crate::core::storage::open as open_storage;
use crate::events::{
    ConnectionStateEvent, EntryAdded, EntryView, PendingCount, ACTIVE_CHANGED,
    CONNECTION_STATE, ENTRY_ADDED, ENTRY_DELETED, HISTORY_CHANGED, PENDING_COUNT,
};
use crate::popover::{build_popover_window, toggle_popover};
use crate::state::AppState;
use std::sync::Arc;
use tauri::menu::{ContextMenu, MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
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
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            spawn_clipboard_capture(app.handle().clone(), app_state.clone());
            register_initial_hotkey(app.handle().clone(), app_state.clone());
            reconcile_autostart(app.handle().clone(), app_state.clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_accounts,
            commands::pair_with_invite,
            commands::pair_start,
            commands::pair_with_code,
            commands::forget_account,
            commands::set_active_account,
            commands::list_history,
            commands::copy_to_clipboard,
            commands::delete_entry,
            commands::clear_history,
            commands::get_settings,
            commands::update_settings,
            commands::open_main_window,
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
        .item(&MenuItemBuilder::with_id("open_accounts", "Accounts…").build(app)?)
        .item(&MenuItemBuilder::with_id("open_pairing", "Pair device…").build(app)?)
        .item(&MenuItemBuilder::with_id("open_settings", "Settings…").build(app)?)
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
        "open_accounts" => {
            let _ = open_main_window_impl(app, "accounts");
        }
        "open_pairing" => {
            let _ = open_main_window_impl(app, "pairing");
        }
        "open_settings" => {
            let _ = open_main_window_impl(app, "settings");
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
            let _ = toggle_popover(app, Some(rect), true);
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

/// Every webview window label this app builds.
///
/// `capabilities/default.json` must cover all of these; a label with no matching
/// capability is denied every IPC command and event listener at runtime. The
/// `capability_guard` tests below enforce that in both directions.
///
/// The named scalars below index into it, so the array is load-bearing in the
/// production build and cannot drift from the labels actually passed to Tauri.
const WINDOW_LABELS: [&str; 2] = ["main", "popover"];
const WINDOW_LABEL_MAIN: &str = WINDOW_LABELS[0];
const WINDOW_LABEL_POPOVER: &str = WINDOW_LABELS[1];

/// The set of sections `main.html` knows how to render. Kept next to the
/// window opener so the guard and the router stay in sync.
fn is_valid_section(section: &str) -> bool {
    matches!(section, "accounts" | "settings" | "pairing")
}

fn open_main_window_impl(app: &tauri::AppHandle, section: &str) -> tauri::Result<()> {
    if !is_valid_section(section) {
        return Err(tauri::Error::Io(std::io::Error::other(format!(
            "unknown section: {section}"
        ))));
    }
    if let Some(win) = app.get_webview_window(WINDOW_LABEL_MAIN) {
        win.set_focus()?;
        let _ = app.emit_to(WINDOW_LABEL_MAIN, crate::events::MAIN_NAVIGATE, section);
        return Ok(());
    }
    let url = format!("main.html?section={section}");
    let win = WebviewWindowBuilder::new(app, WINDOW_LABEL_MAIN, WebviewUrl::App(url.into()))
        .title("sharepaste")
        .inner_size(720.0, 560.0)
        .resizable(true)
        .build()?;

    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    }

    let app_handle = app.clone();
    win.on_window_event(move |ev| {
        if let WindowEvent::Destroyed = ev {
            #[cfg(target_os = "macos")]
            {
                let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }
            // Touch app_handle on non-macOS to silence unused warning.
            let _ = &app_handle;
        }
    });
    Ok(())
}

#[cfg(test)]
mod section_tests {
    use super::is_valid_section;

    #[test]
    fn accepts_exactly_the_three_routable_sections() {
        for section in ["accounts", "settings", "pairing"] {
            assert!(is_valid_section(section), "{section} must be routable");
        }
    }

    #[test]
    fn rejects_blank_unknown_and_miscased_sections() {
        for section in ["", "  ", "history", "Accounts", "ACCOUNTS"] {
            assert!(
                !is_valid_section(section),
                "{section:?} must not open the main window"
            );
        }
    }
}

fn spawn_sync_for_existing_accounts(app: tauri::AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let persisted = match state.registry.load_persisted_active().await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(err = %e, "load persisted active failed");
                None
            }
        };
        let chosen = if let Some(uid) = persisted {
            Some(uid)
        } else {
            state
                .registry
                .list()
                .await
                .unwrap_or_default()
                .into_iter()
                .next()
                .map(|a| a.user_id)
        };
        let Some(user_id) = chosen else { return };
        if let Err(e) = state
            .registry
            .set_active_persisted(Some(user_id.clone()))
            .await
        {
            tracing::warn!(err = %e, "persist active on startup failed");
        }
        let _ = app.emit(
            ACTIVE_CHANGED,
            crate::events::ActiveChanged {
                user_id: Some(user_id.clone()),
            },
        );
        spawn_sync(app.clone(), state.clone(), user_id).await;
    });
}

pub fn set_conn_state(
    app: &tauri::AppHandle,
    state: &AppState,
    user_id: &str,
    new_state: crate::core::sync::ConnectionState,
    last_error: Option<String>,
) {
    state
        .conn_states
        .lock()
        .insert(user_id.to_string(), new_state);
    let _ = app.emit(
        CONNECTION_STATE,
        ConnectionStateEvent {
            user_id: user_id.to_string(),
            state: new_state,
            last_error,
        },
    );
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
            set_conn_state(
                &app,
                &state,
                &user_id,
                crate::core::sync::ConnectionState::AuthFailed,
                Some(e.to_string()),
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
            set_conn_state(
                &app2,
                &state2,
                &user_id_for_sse,
                crate::core::sync::ConnectionState::Connecting,
                None,
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
                    set_conn_state(
                        &app2,
                        &state2,
                        &user_id_for_sse,
                        crate::core::sync::ConnectionState::AuthFailed,
                        Some(s),
                    );
                    return;
                }
                Err(e) => {
                    tracing::warn!(err = %e, "backfill failed; will retry");
                    set_conn_state(
                        &app2,
                        &state2,
                        &user_id_for_sse,
                        crate::core::sync::ConnectionState::Connecting,
                        Some(e.to_string()),
                    );
                    let delay = backoff.next_delay_secs();
                    tokio::select! {
                        _ = cancel2.cancelled() => return,
                        _ = tokio::time::sleep(std::time::Duration::from_secs(delay)) => {}
                    }
                    continue;
                }
            }

            set_conn_state(
                &app2,
                &state2,
                &user_id_for_sse,
                crate::core::sync::ConnectionState::Online,
                None,
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
            set_conn_state(
                &app2,
                &state2,
                &user_id_for_sse,
                crate::core::sync::ConnectionState::Connecting,
                last_error,
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
    let state_for_upload = state.clone();
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
        let state_af = state_for_upload.clone();
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
                set_conn_state(
                    &app_af,
                    &state_af,
                    &user_af,
                    crate::core::sync::ConnectionState::AuthFailed,
                    None,
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

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn spawn_clipboard_capture(app: tauri::AppHandle, state: Arc<AppState>) {
    use crate::core::capture::filter::{evaluate, CaptureContext, FilterDecision, PasteboardSniff};
    #[cfg(target_os = "macos")]
    use crate::core::capture::macos::{frontmost_bundle_id, NSPasteboardSniffer};
    use crate::core::capture::watcher;
    #[cfg(target_os = "windows")]
    use crate::core::capture::windows::{frontmost_process_name, WindowsClipboardSniffer};

    let (tx, mut rx) = mpsc::channel::<crate::core::capture::watcher::ClipboardEvent>(32);
    let watcher_cancel = CancellationToken::new();
    if let Err(e) = watcher::spawn(tx, watcher_cancel.clone()) {
        tracing::error!(err = %e, "clipboard watcher failed to start");
        return;
    }
    tauri::async_runtime::spawn(async move {
        #[cfg(target_os = "macos")]
        let sniffer = NSPasteboardSniffer::new();
        #[cfg(target_os = "windows")]
        let sniffer = WindowsClipboardSniffer::new();

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
            #[cfg(target_os = "macos")]
            let frontmost = frontmost_bundle_id();
            #[cfg(target_os = "windows")]
            let frontmost = frontmost_process_name();

            let last_self = state.last_self_write.lock().clone();
            let last_self_ref = last_self.as_ref().map(|(t, s)| (*t, s.as_str()));
            let last_capture = state.last_capture.lock().clone();
            let ctx = CaptureContext {
                capture_enabled: settings.capture_enabled,
                deny_list: &settings.deny_list,
                frontmost_bundle_id: frontmost.as_deref(),
                last_self_write: last_self_ref,
                last_capture: last_capture.as_deref(),
            };
            let decision =
                evaluate(&ctx, &sniffer as &dyn PasteboardSniff, std::time::Instant::now());
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
            let ciphertext =
                match crate::core::crypto::encrypt(&m.user_key, &user_id, text.as_bytes()) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!(err = %e, "encrypt failed");
                        continue;
                    }
                };
            {
                let conn = state.conn.lock().await;
                if let Err(e) =
                    crate::core::storage::pending::enqueue(&conn, &user_id, &ciphertext, now_ms())
                {
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
            // Only remembered once the entry is durably queued, so a failed
            // enqueue does not suppress the user's next copy of the same text.
            *state.last_capture.lock() = Some(text);
            if let Some(trigger) = state.upload_triggers.lock().get(&user_id).cloned() {
                trigger.notify_one();
            } else {
                tracing::warn!(%user_id, "no uploader trigger registered");
            }
        }
        let _ = watcher_cancel;
    });
}

/// Register or unregister the app with the OS login-items mechanism.
///
/// Callers persist the user's choice independently of this call and only log a
/// failure: a LaunchAgent or registry write that fails must not discard the
/// setting the user just made.
pub fn set_autostart(
    app: &tauri::AppHandle,
    enabled: bool,
) -> Result<(), tauri_plugin_autostart::Error> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    if enabled {
        manager.enable()
    } else {
        manager.disable()
    }
}

/// Bring the OS login-items entry back in line with the stored setting.
///
/// The two drift apart whenever the app is reinstalled, moved, or the entry is
/// removed by hand — without this the checkbox keeps claiming a state the OS no
/// longer has.
fn reconcile_autostart(app: tauri::AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let desired = {
            let conn = state.conn.lock().await;
            match crate::core::storage::settings::load(&conn) {
                Ok(s) => s.autostart,
                Err(e) => {
                    tracing::warn!(err = %e, "load settings for autostart reconcile");
                    return;
                }
            }
        };
        let actual = {
            use tauri_plugin_autostart::ManagerExt;
            match app.autolaunch().is_enabled() {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(err = %e, "read autostart registration");
                    return;
                }
            }
        };
        if actual == desired {
            return;
        }
        match set_autostart(&app, desired) {
            Ok(()) => tracing::info!(desired, "autostart registration reconciled"),
            Err(e) => tracing::warn!(err = %e, desired, "reconcile autostart failed"),
        }
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
                #[cfg(target_os = "windows")]
                let result = toggle_popover(&app_clone, None, false);
                #[cfg(not(target_os = "windows"))]
                let result = toggle_popover(&app_clone, None, true);
                let _ = result;
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

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Guards the invariant behind S1: the capability file must name every window the
/// app builds. This reads the shipped JSON rather than mocking the ACL, so it fails
/// if a window is added, renamed, or dropped from the capability file.
#[cfg(test)]
mod capability_guard {
    use super::WINDOW_LABELS;

    const CAPABILITY_JSON: &str = include_str!("../capabilities/default.json");

    /// Tauri matches capability `windows` entries against window labels with glob
    /// syntax. Only `*` matters for the patterns this app can legitimately use.
    fn pattern_matches(pattern: &str, label: &str) -> bool {
        match pattern.split_once('*') {
            None => pattern == label,
            Some((prefix, suffix)) => {
                label.len() >= prefix.len() + suffix.len()
                    && label.starts_with(prefix)
                    && label.ends_with(suffix)
            }
        }
    }

    fn declared_patterns() -> Vec<String> {
        let parsed: serde_json::Value = serde_json::from_str(CAPABILITY_JSON)
            .expect("capabilities/default.json must be valid JSON");
        parsed["windows"]
            .as_array()
            .expect("capabilities/default.json must declare a `windows` array")
            .iter()
            .map(|w| {
                w.as_str()
                    .expect("every `windows` entry must be a string")
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn every_window_label_is_covered_by_a_capability() {
        let patterns = declared_patterns();
        for label in WINDOW_LABELS {
            assert!(
                patterns.iter().any(|p| pattern_matches(p, label)),
                "window {label:?} matches no pattern in capabilities/default.json {patterns:?}. \
                 Tauri denies by default, so that window would silently lose every event \
                 listener at runtime while still rendering."
            );
        }
    }

    #[test]
    fn no_capability_pattern_matches_a_window_that_does_not_exist() {
        let patterns = declared_patterns();
        for pattern in &patterns {
            assert!(
                WINDOW_LABELS.iter().any(|l| pattern_matches(pattern, l)),
                "capabilities/default.json declares {pattern:?}, which matches none of the \
                 windows this app builds {WINDOW_LABELS:?}. It is a leftover from an older \
                 window topology - delete it."
            );
        }
    }

    #[test]
    fn pattern_matcher_handles_globs() {
        assert!(pattern_matches("main", "main"));
        assert!(!pattern_matches("main", "main-2"));
        assert!(pattern_matches("modal-*", "modal-settings"));
        assert!(!pattern_matches("modal-*", "main"));
        assert!(pattern_matches("*", "anything"));
    }
}
