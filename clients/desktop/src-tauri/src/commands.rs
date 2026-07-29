//! The IPC surface: argument shapes in, wire shapes out, one facade call each.
//!
//! Nothing here reaches for the network, a key or the database. Every command
//! below either delegates to [`Sharepaste`](sharepaste_core::facade::Sharepaste)
//! or drives something only a Tauri shell owns — a window, the tray, the
//! updater. The events a command's work produces are emitted by the core through
//! `TauriEventSink`, not from here, so an operation reports itself identically
//! however it was started.

use crate::events::ContactEvent;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use sharepaste_core::errors::AppError;
use sharepaste_core::event::Entry;
use sharepaste_core::facade::SettingsPatch;
use sharepaste_core::storage::settings::Settings;
use sharepaste_core::sync::ConnectionState;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

/// One Pairing as the frontend lists it.
///
/// The shell's own wire shape rather than the core's `PairingSummary`, which
/// carries no `Serialize`: the JSON field names are this app's IPC contract and
/// belong on this side of the boundary.
#[derive(Serialize)]
pub(crate) struct PairingSummary {
    pub(crate) user_id: String,
    pub(crate) device_id: String,
    pub(crate) label: String,
    /// The User's name on the relay, mirrored by `GET /me`. `None` until this
    /// device has reached a relay that serves it.
    pub(crate) username: Option<String>,
    pub(crate) server_url: String,
    /// The relay as a person reads it. The core resolves it because Kotlin
    /// needs the same string and the two parsers had already diverged.
    pub(crate) relay_host: String,
    pub(crate) status: ConnectionState,
    pub(crate) pending: i64,
    pub(crate) is_active: bool,
}

impl From<sharepaste_core::facade::PairingSummary> for PairingSummary {
    fn from(p: sharepaste_core::facade::PairingSummary) -> Self {
        Self {
            user_id: p.user_id,
            device_id: p.device_id,
            label: p.label,
            username: p.username,
            server_url: p.server_url,
            relay_host: p.relay_host,
            status: p.status,
            pending: p.pending,
            is_active: p.is_active,
        }
    }
}

#[tauri::command]
pub async fn list_pairings(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<PairingSummary>, AppError> {
    let pairings = state.core.list_pairings().await?;
    Ok(pairings.into_iter().map(PairingSummary::from).collect())
}

#[derive(Deserialize)]
pub(crate) struct PairWithInviteArgs {
    pub(crate) server_url: String,
    pub(crate) token: String,
    pub(crate) device_label: String,
}

/// The Pairing a completed handshake produced, for both pairing paths.
#[derive(Serialize)]
pub(crate) struct PairedDevice {
    pub(crate) user_id: String,
    pub(crate) device_id: String,
}

impl From<sharepaste_core::facade::PairedDevice> for PairedDevice {
    fn from(d: sharepaste_core::facade::PairedDevice) -> Self {
        Self { user_id: d.user_id, device_id: d.device_id }
    }
}

#[tauri::command]
pub async fn pair_with_invite(
    args: PairWithInviteArgs,
    state: State<'_, Arc<AppState>>,
) -> Result<PairedDevice, AppError> {
    let device = state
        .core
        .pair_with_invite(&args.server_url, &args.token, &args.device_label)
        .await?;
    Ok(PairedDevice::from(device))
}

#[derive(Deserialize)]
pub(crate) struct PairStartArgs {
    pub(crate) user_id: String,
}

#[derive(Serialize)]
pub(crate) struct PairStartResp {
    pub(crate) code: String,
    pub(crate) expires_at: i64,
}

/// Reveal a short code for a second device.
///
/// One call, because the ordering inside it is load-bearing: the payload is
/// uploaded before the code exists anywhere this shell can see it. See
/// `Sharepaste::pair_start` — the claim and the expiry arrive later as events,
/// from the poll task the facade's runtime owns.
#[tauri::command]
pub async fn pair_start(
    args: PairStartArgs,
    state: State<'_, Arc<AppState>>,
) -> Result<PairStartResp, AppError> {
    let revealed = state.core.pair_start(&args.user_id).await?;
    Ok(PairStartResp { code: revealed.code, expires_at: revealed.expires_at })
}

#[derive(Deserialize)]
pub(crate) struct PairWithCodeArgs {
    pub(crate) code: String,
    pub(crate) device_label: String,
}

#[tauri::command]
pub async fn pair_with_code(
    args: PairWithCodeArgs,
    state: State<'_, Arc<AppState>>,
) -> Result<PairedDevice, AppError> {
    let device = state
        .core
        .pair_with_code(&args.code, &args.device_label)
        .await?;
    Ok(PairedDevice::from(device))
}

#[derive(Deserialize)]
pub(crate) struct UserScopedArgs {
    pub(crate) user_id: String,
}

#[tauri::command]
pub async fn forget_pairing(
    args: UserScopedArgs,
    state: State<'_, Arc<AppState>>,
) -> Result<(), AppError> {
    state.core.forget_pairing(&args.user_id).await
}

#[tauri::command]
pub async fn set_active_pairing(
    args: UserScopedArgs,
    state: State<'_, Arc<AppState>>,
) -> Result<(), AppError> {
    state.core.set_active_pairing(&args.user_id).await
}

#[derive(Deserialize)]
pub(crate) struct ListHistoryArgs {
    pub(crate) user_id: String,
    pub(crate) before_id: Option<i64>,
    pub(crate) limit: i64,
}

#[tauri::command]
pub async fn list_history(
    args: ListHistoryArgs,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<Entry>, AppError> {
    state
        .core
        .list_history(&args.user_id, args.before_id, args.limit)
        .await
}

/// Contact for one user.
///
/// The popover opens long after the last `contact` event fired and has to
/// hydrate from somewhere; the facade decides whether that is the live reading
/// or the persisted one.
#[tauri::command]
pub async fn get_contact(
    args: UserScopedArgs,
    state: State<'_, Arc<AppState>>,
) -> Result<ContactEvent, AppError> {
    let contact = state.core.get_contact(&args.user_id).await?;
    Ok(ContactEvent {
        user_id: contact.user_id,
        last_contact_at: contact.last_contact_at,
    })
}

#[derive(Deserialize)]
pub(crate) struct EntryScopedArgs {
    pub(crate) user_id: String,
    pub(crate) entry_id: i64,
}

#[tauri::command]
pub async fn copy_to_clipboard(
    args: EntryScopedArgs,
    state: State<'_, Arc<AppState>>,
) -> Result<(), AppError> {
    state.core.recall(&args.user_id, args.entry_id).await
}

#[tauri::command]
pub async fn delete_entry(
    args: EntryScopedArgs,
    state: State<'_, Arc<AppState>>,
) -> Result<(), AppError> {
    state.core.delete_entry(&args.user_id, args.entry_id).await
}

#[tauri::command]
pub async fn clear_history(
    args: UserScopedArgs,
    state: State<'_, Arc<AppState>>,
) -> Result<(), AppError> {
    state.core.clear_history(&args.user_id).await
}

#[tauri::command]
pub async fn get_settings(
    state: State<'_, Arc<AppState>>,
) -> Result<Settings, AppError> {
    state.core.get_settings().await
}

#[tauri::command]
pub async fn update_settings(
    patch: serde_json::Value,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<Settings, AppError> {
    // The frontend sends JSON; the core takes a typed patch. Converting here is
    // what keeps the IPC shape the UI already sends *and* keeps arbitrary JSON
    // out of the core, which could not carry a `serde_json::Value` over FFI in
    // any case.
    let patch = SettingsPatch {
        capture_enabled: patch.get("capture_enabled").and_then(|v| v.as_bool()),
        deny_list: patch
            .get("deny_list")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect()),
        autostart: patch.get("autostart").and_then(|v| v.as_bool()),
        update_check_enabled: patch.get("update_check_enabled").and_then(|v| v.as_bool()),
        // Present-and-null clears the binding, which is why the field is doubly
        // optional: "absent from the patch" has to stay distinguishable from
        // "cleared by the user".
        hotkey: patch
            .get("hotkey")
            .map(|v| if v.is_null() { None } else { v.as_str().map(String::from) }),
    };

    // Read before, compare after. The hotkey registration and the login item are
    // this shell's mechanisms and stay here; comparing is how it learns which of
    // them the patch actually moved.
    let before = state.core.get_settings().await?;
    let after = state.core.update_settings(patch).await?;

    if after.hotkey != before.hotkey {
        if let Err(e) = crate::apply_hotkey(&app, after.hotkey.as_deref()) {
            tracing::warn!(err = %e, "re-register hotkey failed");
        }
    }
    // The choice is already persisted above; a failed login-item write is logged
    // and swallowed so the user does not silently lose the setting.
    if after.autostart != before.autostart {
        if let Err(e) = crate::set_autostart(&app, after.autostart) {
            tracing::warn!(
                err = %e,
                enabled = after.autostart,
                "update autostart registration failed"
            );
        }
    }
    Ok(after)
}

#[derive(Deserialize)]
pub(crate) struct OpenMainWindowArgs {
    pub(crate) section: String,
    #[serde(default)]
    pub(crate) entry_id: Option<i64>,
}

#[tauri::command]
pub async fn open_main_window(
    app: AppHandle,
    args: OpenMainWindowArgs,
) -> Result<(), AppError> {
    crate::open_main_window_impl(&app, &args.section, args.entry_id)
        .map_err(|e| AppError::BadInput(e.to_string()))
}

#[tauri::command]
pub async fn hide_popover(app: AppHandle) -> Result<(), AppError> {
    if let Some(win) = app.get_webview_window("popover") {
        win.hide().map_err(|e| AppError::BadInput(e.to_string()))?;
    }
    Ok(())
}

/// What the Settings pane needs to render the update row without asking the
/// Update Source anything.
///
/// Reported separately from the check so opening Settings never emits a packet:
/// "automatic check off" has to hold for every surface, not just launch.
#[derive(Serialize)]
pub(crate) struct UpdateStatus {
    pub(crate) current_version: String,
    /// The Release on offer as of the last check, or `None` if none was found.
    pub(crate) available: Option<crate::events::UpdateAvailable>,
}

#[tauri::command]
pub async fn get_update_status(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<UpdateStatus, AppError> {
    let available = state
        .pending_update
        .lock()
        .as_ref()
        .map(crate::events::UpdateAvailable::from);
    Ok(UpdateStatus { current_version: crate::update::current_version(&app), available })
}

#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<UpdateStatus, AppError> {
    let available = crate::update::check(&app, state.inner()).await?;
    Ok(UpdateStatus { current_version: crate::update::current_version(&app), available })
}

/// Download the pending Release, install it and relaunch. Never reached
/// without a click — see ADR 0005.
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), AppError> {
    crate::update::install(&app, state.inner()).await
}
