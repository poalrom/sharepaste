pub(crate) mod capture;
pub(crate) mod commands;
pub(crate) mod config;
pub(crate) mod events;
pub(crate) mod logging;
mod popover;
pub(crate) mod state;
pub(crate) mod update;

use crate::config::{desktop_data_dir, Paths};
use crate::events::TauriEventSink;
use crate::popover::{build_popover_window, toggle_popover};
use crate::state::{AppState, SystemClipboard};
use sharepaste_core::facade::{Sharepaste, SharepasteConfig};
use sharepaste_core::http::TransportPolicy;
use sharepaste_core::keychain::SystemKeychain;
use sharepaste_core::relay::RelayDial;
use std::sync::Arc;
use tauri::menu::{ContextMenu, MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub fn launch() {
    let paths = Paths::for_data_dir(desktop_data_dir());
    paths.ensure_dirs().expect("create app dirs");
    let _log_guard = logging::init(&paths.log_dir);

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            // The core is built here rather than before the builder because its
            // event sink needs an `AppHandle`, and that only exists once Tauri
            // has started. Nothing runs before `setup`, so no event is missed.
            let core = Sharepaste::open(SharepasteConfig {
                db_path: paths.db_path.clone(),
                keychain: Arc::new(SystemKeychain::default()),
                clipboard: Arc::new(SystemClipboard),
                events: Arc::new(TauriEventSink::new(app.handle().clone())),
                // `AllowCleartext` is not an oversight and not a thing to tidy
                // to `RequireHttps`: a desktop already paired to a cleartext
                // relay has to keep working, so the scheme rule belongs to
                // whichever shell can afford it. The mobile shells require
                // HTTPS; flipping this one would strand an existing
                // installation with no way to recover its pairing.
                relay: RelayDial::over_http(TransportPolicy::AllowCleartext),
            })?;
            let app_state = Arc::new(AppState::new(core));
            app.manage(app_state.clone());

            build_tray(app, app_state.clone())?;
            build_popover_window(app)?;
            spawn_sync_for_existing_pairings(app_state.clone());
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            spawn_clipboard_capture(app_state.clone());
            register_initial_hotkey(app.handle().clone(), app_state.clone());
            reconcile_autostart(app.handle().clone(), app_state.clone());
            update::spawn_launch_check(app.handle().clone(), app_state.clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_pairings,
            commands::pair_with_invite,
            commands::pair_start,
            commands::pair_with_code,
            commands::forget_pairing,
            commands::set_active_pairing,
            commands::get_contact,
            commands::list_history,
            commands::copy_to_clipboard,
            commands::delete_entry,
            commands::resend_entry,
            commands::clear_history,
            commands::get_settings,
            commands::update_settings,
            commands::open_main_window,
            commands::hide_popover,
            commands::get_update_status,
            commands::check_for_update,
            commands::install_update,
        ])
        .run(tauri::generate_context!())
        .expect("run tauri");
}

fn build_tray(app: &mut tauri::App, state: Arc<AppState>) -> tauri::Result<()> {
    // The tray icon itself is created from tauri.conf.json (`app.trayIcon`).
    // Building another one here would render a second, icon-less status item.
    // Look up the existing one and attach our menu + handlers to it.
    let tray = app
        .tray_by_id("main")
        .ok_or_else(|| tauri::Error::Io(std::io::Error::other("tray 'main' not found")))?;

    let menu = MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id("open_history", "History…").build(app)?)
        .item(&MenuItemBuilder::with_id("open_pairings", "Pairings…").build(app)?)
        .item(&MenuItemBuilder::with_id("open_pairing", "Pair device…").build(app)?)
        .item(&MenuItemBuilder::with_id("open_settings", "Settings…").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("quit", "Quit").build(app)?)
        .build()?;

    // `Install update…` is deliberately not here: it is inserted by
    // `update::check` only while a Release is pending, and removed again
    // otherwise. Hence the menu has to outlive this function.
    *state.tray_menu.lock() = Some(menu.clone());

    // tray-icon-0.23.1's TrayTarget intercepts mouseDown before NSStatusItem's
    // setMenu can fire, and the performClick(None) fallback doesn't actually
    // display the menu — neither click pops it. We display the menu manually
    // on right click via Menu::popup, so left click stays free for the popover
    // toggle and the tray-icon auto-popup machinery is left disabled.
    tray.set_show_menu_on_left_click(false)?;

    let menu_for_event = menu.clone();
    tray.on_menu_event(|app, event| match event.id.as_ref() {
        "open_history" => {
            let _ = open_main_window_impl(app, "history", None);
        }
        "open_pairings" => {
            let _ = open_main_window_impl(app, "pairings", None);
        }
        "open_pairing" => {
            let _ = open_main_window_impl(app, "pairing", None);
        }
        "open_settings" => {
            let _ = open_main_window_impl(app, "settings", None);
        }
        id if id == update::TRAY_ITEM_ID => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let Some(state) = app.try_state::<Arc<AppState>>() else { return };
                let state = state.inner().clone();
                if let Err(e) = update::install(&app, &state).await {
                    tracing::warn!(err = %e, "install update from tray failed");
                }
            });
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
///
/// `pairing` is not a pane of its own: it selects the Pairings pane with the
/// add-flow already open, which is what the tray's "Pair device…" wants and
/// what ADR 0004 kept the singular around for.
fn is_valid_section(section: &str) -> bool {
    matches!(section, "history" | "pairings" | "settings" | "pairing")
}

/// Open (or focus) the main window on `section`.
///
/// `entry_id` is the popover's handoff: the entry it had selected, carried so
/// the reader opens on the row the user was already squinting at. A stale id
/// simply selects nothing.
fn open_main_window_impl(
    app: &tauri::AppHandle,
    section: &str,
    entry_id: Option<i64>,
) -> tauri::Result<()> {
    if !is_valid_section(section) {
        return Err(tauri::Error::Io(std::io::Error::other(format!(
            "unknown section: {section}"
        ))));
    }
    if let Some(win) = app.get_webview_window(WINDOW_LABEL_MAIN) {
        win.set_focus()?;
        let _ = app.emit_to(
            WINDOW_LABEL_MAIN,
            crate::events::MAIN_NAVIGATE,
            crate::events::MainNavigate { section: section.to_string(), entry_id },
        );
        return Ok(());
    }
    let url = match entry_id {
        Some(id) => format!("main.html?section={section}&entry={id}"),
        None => format!("main.html?section={section}"),
    };
    // Decorations off: the panel's notch is cut into the window's own corners,
    // and an OS titlebar sitting above it reads as a rendering fault. Resizable
    // and generously sized anyway — unlike the popover this window holds a
    // scrolling list beside a pane for reading arbitrary text.
    let win = WebviewWindowBuilder::new(app, WINDOW_LABEL_MAIN, WebviewUrl::App(url.into()))
        .title("sharepaste")
        .inner_size(980.0, 680.0)
        .min_inner_size(860.0, 560.0)
        .resizable(true)
        .decorations(false)
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
    fn accepts_exactly_the_four_routable_sections() {
        for section in ["history", "pairings", "settings", "pairing"] {
            assert!(is_valid_section(section), "{section} must be routable");
        }
    }

    #[test]
    fn rejects_blank_unknown_and_miscased_sections() {
        for section in ["", "  ", "accounts", "History", "PAIRINGS"] {
            assert!(
                !is_valid_section(section),
                "{section:?} must not open the main window"
            );
        }
    }
}

/// Pick the Active Pairing back up at launch and open its session.
///
/// Two calls, because the core deliberately does not decide for itself that now
/// is a good time to open a socket: `resume_active_pairing` settles which pairing
/// and announces it, and this shell — being in the foreground by definition —
/// asks for the session.
fn spawn_sync_for_existing_pairings(state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let resumed = match state.core.resume_active_pairing().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(err = %e, "resuming the Active Pairing failed");
                return;
            }
        };
        let Some(user_id) = resumed else { return };
        if let Err(e) = state.core.start_session(&user_id).await {
            tracing::warn!(err = %e, %user_id, "starting the session on launch failed");
        }
    });
}

/// Watched Capture's platform half.
///
/// The shell keeps the three things only it can do — the clipboard-change
/// watcher, the platform pasteboard sniffer, and the frontmost-application
/// lookup — and hands them to the core. The filter, the encryption, the queue and
/// the `pending-count` event are all `capture_watched`'s, so there is exactly one
/// implementation of what may be captured and it is the one a phone's Offered
/// Capture shares.
///
/// The sniffer crosses as a `&dyn PasteboardSniff` rather than a snapshot on
/// purpose: it reads the pasteboard's *types* first and its text only if none of
/// them is transient or concealed, so a concealed password's plaintext is never
/// pulled into memory for the filter to then reject.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn spawn_clipboard_capture(state: Arc<AppState>) {
    use sharepaste_core::capture::filter::PasteboardSniff;
    use sharepaste_core::facade::OfferOutcome;
    #[cfg(target_os = "macos")]
    use crate::capture::macos::{frontmost_bundle_id, NSPasteboardSniffer};
    use crate::capture::watcher;
    #[cfg(target_os = "windows")]
    use crate::capture::windows::{frontmost_process_name, WindowsClipboardSniffer};
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    let (tx, mut rx) = mpsc::channel::<crate::capture::watcher::ClipboardEvent>(32);
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
            let Some(user_id) = state.core.active_pairing() else {
                continue;
            };
            #[cfg(target_os = "macos")]
            let frontmost = frontmost_bundle_id();
            #[cfg(target_os = "windows")]
            let frontmost = frontmost_process_name();

            match state
                .core
                .capture_watched(
                    &user_id,
                    &sniffer as &dyn PasteboardSniff,
                    frontmost.as_deref(),
                )
                .await
            {
                Ok(OfferOutcome::Queued { .. }) => {}
                // Text this device already holds: a Use of the entry that is
                // there, recorded by the core. A Watched Capture is silent
                // either way — nothing on screen distinguishes the two, and
                // ADR 0012 keeps it that way deliberately.
                Ok(OfferOutcome::Recognised { .. }) => {}
                Ok(OfferOutcome::Rejected(reason)) => {
                    tracing::debug!(?reason, "clipboard skip");
                }
                Err(e) => tracing::warn!(err = %e, "capturing the clipboard failed"),
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
pub(crate) fn set_autostart(
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
        let desired = match state.core.get_settings().await {
            Ok(s) => s.autostart,
            Err(e) => {
                tracing::warn!(err = %e, "load settings for autostart reconcile");
                return;
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
        let hotkey = state
            .core
            .get_settings()
            .await
            .map(|s| s.hotkey)
            .unwrap_or(None);
        if let Some(h) = hotkey {
            if let Err(e) = apply_hotkey(&app, Some(&h)) {
                tracing::warn!(err = %e, hotkey = %h, "register global shortcut failed");
            }
        }
    });
}

pub(crate) fn apply_hotkey(app: &tauri::AppHandle, hotkey: Option<&str>) -> tauri::Result<()> {
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
