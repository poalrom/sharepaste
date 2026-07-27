//! Asking the Update Source for a newer Release, and installing one.
//!
//! The check lives in Rust rather than the webview because the tray has to know
//! whether an update is pending and the tray is not reachable from JS. Routing
//! it through `@tauri-apps/plugin-updater` as well would mean two callers
//! asking the same Update Source, and a plugin ACL entry for a surface that
//! already exists here.
//!
//! Nothing in this module downloads or restarts on its own: [`spawn_launch_check`]
//! asks and stops. [`install`] runs only from a click, per ADR 0005.

use crate::errors::AppError;
use crate::events::{UpdateAvailable, UPDATE_AVAILABLE};
use crate::state::AppState;
use std::sync::Arc;
use tauri::menu::{MenuItemBuilder, MenuItemKind};
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

/// Menu id of the tray's install item. `build_tray` routes this id to
/// [`install`], the same path the Settings button takes.
pub(crate) const TRAY_ITEM_ID: &str = "install_update";

impl From<tauri_plugin_updater::Error> for AppError {
    fn from(e: tauri_plugin_updater::Error) -> Self {
        AppError::Update(e.to_string())
    }
}

impl From<&tauri_plugin_updater::Update> for UpdateAvailable {
    fn from(u: &tauri_plugin_updater::Update) -> Self {
        UpdateAvailable { version: u.version.clone(), notes: u.body.clone() }
    }
}

/// Ask the Update Source whether it holds a Release newer than this build.
///
/// Caches the answer on [`AppState`] — the install path needs the very `Update`
/// this returned, because that is what carries the signature the download will
/// be checked against — then brings the tray item and any open window in line
/// with it.
pub(crate) async fn check(
    app: &AppHandle,
    state: &Arc<AppState>,
) -> Result<Option<UpdateAvailable>, AppError> {
    let found = app.updater()?.check().await?;
    let available = found.as_ref().map(UpdateAvailable::from);
    *state.pending_update.lock() = found;

    // A tray that cannot be repainted is not a reason to fail the check: the
    // Settings pane still has the answer, and the next check retries.
    if let Err(e) = set_tray_item_present(app, state, available.is_some()) {
        tracing::warn!(err = %e, "update tray item could not be repainted");
    }
    if let Some(update) = &available {
        let _ = app.emit(UPDATE_AVAILABLE, update.clone());
    }
    Ok(available)
}

/// Replace this device's copy with the pending Release and come back running it.
///
/// On Windows the NSIS installer takes the process over and never returns here;
/// on macOS the bundle is swapped in place and we relaunch ourselves.
pub(crate) async fn install(app: &AppHandle, state: &Arc<AppState>) -> Result<(), AppError> {
    // Cloned out of the lock: `download_and_install` awaits, and a parking_lot
    // guard held across an await point is a deadlock waiting for a second caller.
    let pending = state.pending_update.lock().clone();
    let Some(update) = pending else {
        return Err(AppError::NotFound(
            "no update is pending on this device".into(),
        ));
    };
    update.download_and_install(|_, _| {}, || {}).await?;
    app.restart()
}

/// This build's version, as the Settings pane reports it.
pub(crate) fn current_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

/// Check at launch unless the user has switched it off.
///
/// The setting is read before anything is sent, because "off" has to mean no
/// packet at all — that is the disclosure the README makes.
pub(crate) fn spawn_launch_check(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let enabled = {
            let conn = state.conn.lock().await;
            match crate::core::storage::settings::load(&conn) {
                Ok(s) => s.update_check_enabled,
                Err(e) => {
                    // Unreadable settings must not be read as consent. A missing
                    // row already loads as the default, so this is corruption.
                    tracing::warn!(err = %e, "settings unreadable; skipping the update check");
                    return;
                }
            }
        };
        if !enabled {
            tracing::debug!("automatic update check is off; not contacting the update source");
            return;
        }
        match check(&app, &state).await {
            Ok(Some(u)) => tracing::info!(version = %u.version, "update available"),
            Ok(None) => tracing::debug!("no update available"),
            Err(e) => tracing::warn!(err = %e, "update check failed"),
        }
    });
}

/// Add or drop the tray's `Install update…` item.
///
/// Absence rather than a disabled item is the requirement: ADR 0002 rules that
/// chrome which informs in the rare case does not get to cost space in the
/// common one, and nothing is pending nearly always.
///
/// Must not run on the main thread — every `Menu` call here dispatches to the
/// main thread and blocks for the reply.
fn set_tray_item_present(
    app: &AppHandle,
    state: &Arc<AppState>,
    present: bool,
) -> tauri::Result<()> {
    let menu = state.tray_menu.lock().clone();
    // No menu means `build_tray` has not run yet, which only happens if a check
    // beats setup. Nothing to repaint, and the next check will find one.
    let Some(menu) = menu else { return Ok(()) };
    match (present, menu.get(TRAY_ITEM_ID)) {
        (true, None) => {
            let item = MenuItemBuilder::with_id(TRAY_ITEM_ID, "Install update…").build(app)?;
            // Directly above the separator that fences Quit off, wherever
            // `build_tray` has left it. A hard index would silently drift to
            // the wrong side of that separator the day a tray item is added.
            let position = menu
                .items()?
                .iter()
                .position(|i| matches!(i, MenuItemKind::Predefined(_)))
                .unwrap_or(0);
            menu.insert(&item, position)
        }
        (false, Some(item)) => menu.remove(&item),
        _ => Ok(()),
    }
}
