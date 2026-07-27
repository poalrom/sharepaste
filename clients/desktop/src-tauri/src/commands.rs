use crate::core::http::ServerClient;
use crate::core::keychain::{token_account, user_key_account};
use crate::core::pairing::invite::{claim_invite, persist_claimed_pairing};
use crate::core::pairing::payload::{
    fetch_and_decrypt_pair_payload, secret_proof_hex, start_pair, upload_pair_payload,
};
use crate::core::pairing::shortcode::{decode as decode_shortcode, group_for_display};
use crate::core::storage::{accounts as accounts_repo, devices, entries_cache, pending, settings};
use crate::core::sync::ConnectionState;
use crate::errors::AppError;
use crate::events::{
    PairingAdded, EntryView, PairShortcode, ContactEvent, PAIRING_ADDED, ACTIVE_PAIRING_CHANGED,
    HISTORY_CHANGED, PAIR_SHORTCODE,
};
use crate::now_ms;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, State};
use zeroize::Zeroizing;

#[derive(Serialize)]
pub(crate) struct PairingSummary {
    pub(crate) user_id: String,
    pub(crate) device_id: String,
    pub(crate) label: String,
    /// The User's name on the relay, mirrored by `GET /me`. `None` until this
    /// device has reached a relay that serves it.
    pub(crate) username: Option<String>,
    pub(crate) server_url: String,
    pub(crate) status: ConnectionState,
    pub(crate) pending: i64,
    pub(crate) is_active: bool,
}

#[tauri::command]
pub async fn list_pairings(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<PairingSummary>, AppError> {
    let accts = state.registry.list().await?;
    let active = state.registry.active_user_id();
    let mut out = Vec::with_capacity(accts.len());
    let conn = state.conn.lock().await;
    let conn_states = state.conn_states.lock();
    for a in accts {
        let pending = pending::count(&conn, &a.user_id)?;
        let is_active = active.as_deref() == Some(&a.user_id);
        let status = conn_states
            .get(&a.user_id)
            .copied()
            .unwrap_or(ConnectionState::Disconnected);
        out.push(PairingSummary {
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

#[derive(Deserialize)]
pub(crate) struct PairWithInviteArgs {
    pub(crate) server_url: String,
    pub(crate) token: String,
    pub(crate) device_label: String,
}

#[derive(Serialize)]
pub(crate) struct PairWithInviteResp {
    pub(crate) user_id: String,
    pub(crate) device_id: String,
}

#[tauri::command]
pub async fn pair_with_invite(
    args: PairWithInviteArgs,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<PairWithInviteResp, AppError> {
    let server = ServerClient::new(args.server_url.trim())?;
    let mut claimed = claim_invite(&server, &args.token, &args.device_label).await?;
    claimed.server_url = args.server_url.clone();
    {
        let conn = state.conn.lock().await;
        persist_claimed_pairing(
            &conn,
            state.keychain.as_ref(),
            &claimed,
            &args.device_label,
            now_ms(),
        )?;
    }
    app.emit(
        PAIRING_ADDED,
        PairingAdded {
            user_id: claimed.user_id.clone(),
            device_id: claimed.device_id.clone(),
            label: args.device_label.clone(),
        },
    )
    .ok();
    activate_and_sync(&app, state.inner(), &claimed.user_id).await;
    Ok(PairWithInviteResp {
        user_id: claimed.user_id,
        device_id: claimed.device_id,
    })
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

#[tauri::command]
pub async fn pair_start(
    args: PairStartArgs,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<PairStartResp, AppError> {
    let m = state.registry.load_active_membership(&args.user_id).await?;
    let started = start_pair(&m.server).await?;

    // Pre-upload payload (encrypted to pairing_secret) before exposing the code,
    // so the claimer's fetch can never race the inviter's upload.
    let user_key: crate::core::crypto::UserKey = Zeroizing::new(*m.user_key);
    let server_url = m.server.base().to_string();
    upload_pair_payload(
        &m.server,
        started.pair_id,
        &started.pairing_secret,
        &args.user_id,
        &user_key,
        &server_url,
    )
    .await?;

    let expires_at = now_ms() + 2 * 60 * 1000;
    let formatted = group_for_display(&started.shortcode);
    app.emit(
        PAIR_SHORTCODE,
        PairShortcode {
            code: formatted.clone(),
            expires_at,
        },
    )
    .ok();

    // Spawn pair-watch task that polls /pair/poll and emits pair-claimed / pair-expired.
    let server = m.server.clone();
    let user_id = args.user_id.clone();
    let pair_id = started.pair_id;
    let app2 = app.clone();
    tokio::spawn(async move {
        loop {
            match server.pair_poll(&pair_id.to_string(), 25_000).await {
                Ok(p) if p.status == "consumed" => {
                    let _ = app2.emit(
                        crate::events::PAIR_CLAIMED,
                        crate::events::PairClaimed {
                            user_id: user_id.clone(),
                            device_label: p.device_label,
                        },
                    );
                    return;
                }
                Ok(p) if p.status == "expired" => {
                    let _ = app2.emit(crate::events::PAIR_EXPIRED, ());
                    return;
                }
                Ok(_waiting) => continue,
                Err(AppError::PairExpired(_)) => {
                    let _ = app2.emit(crate::events::PAIR_EXPIRED, ());
                    return;
                }
                Err(e) => {
                    tracing::warn!(err = %e, "pair poll errored");
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
    });

    Ok(PairStartResp {
        code: formatted,
        expires_at,
    })
}

#[derive(Deserialize)]
pub(crate) struct PairWithCodeArgs {
    pub(crate) code: String,
    pub(crate) device_label: String,
}

#[derive(Serialize)]
pub(crate) struct PairWithCodeResp {
    pub(crate) user_id: String,
    pub(crate) device_id: String,
}

#[tauri::command]
pub async fn pair_with_code(
    args: PairWithCodeArgs,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<PairWithCodeResp, AppError> {
    let payload_decoded = decode_shortcode(&args.code)?;
    let server = ServerClient::new(&payload_decoded.server_url)?;
    let proof = secret_proof_hex(&payload_decoded.pairing_secret);
    server
        .pair_claim(&payload_decoded.pair_id.to_string(), &proof)
        .await?;

    let pair_payload = fetch_and_decrypt_pair_payload(
        &server,
        payload_decoded.pair_id,
        &payload_decoded.pairing_secret,
    )
    .await?;
    let resp = server
        .devices(
            &payload_decoded.pair_id.to_string(),
            &proof,
            &args.device_label,
        )
        .await?;

    state.keychain.put(
        &user_key_account(&pair_payload.user_id),
        &pair_payload.user_key,
    )?;
    state
        .keychain
        .put(&token_account(&pair_payload.user_id), &resp.device_token)?;

    {
        let conn = state.conn.lock().await;
        accounts_repo::upsert(
            &conn,
            &accounts_repo::Account {
                user_id: pair_payload.user_id.clone(),
                device_id: resp.device_id.clone(),
                device_label: args.device_label.clone(),
                server_url: pair_payload.server_url.clone(),
                last_seen_id: 0,
                created_at: now_ms(),
                username: None,
                last_contact_at: None,
            },
        )?;
    }
    app.emit(
        PAIRING_ADDED,
        PairingAdded {
            user_id: pair_payload.user_id.clone(),
            device_id: resp.device_id.clone(),
            label: args.device_label.clone(),
        },
    )
    .ok();
    activate_and_sync(&app, state.inner(), &pair_payload.user_id).await;

    Ok(PairWithCodeResp {
        user_id: pair_payload.user_id,
        device_id: resp.device_id,
    })
}

#[derive(Deserialize)]
pub(crate) struct UserScopedArgs {
    pub(crate) user_id: String,
}

#[tauri::command]
pub async fn forget_pairing(
    args: UserScopedArgs,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), AppError> {
    let was_active = state
        .registry
        .active_user_id()
        .as_deref()
        == Some(args.user_id.as_str());

    if let Some(cancel) = state.sync_tasks.lock().remove(&args.user_id) {
        cancel.cancel();
    }
    state.conn_states.lock().remove(&args.user_id);
    state.last_contact.lock().remove(&args.user_id);

    let result = state.registry.forget(&args.user_id).await;

    let new_active = match &result {
        Ok(next) => next.clone(),
        Err(_) => None,
    };

    if was_active {
        app.emit(
            ACTIVE_PAIRING_CHANGED,
            crate::events::ActivePairingChanged {
                user_id: new_active.clone(),
            },
        )
        .ok();
    }
    app.emit(
        crate::events::PAIRING_REMOVED,
        crate::events::PairingRemoved {
            user_id: args.user_id.clone(),
        },
    )
    .ok();

    result?;

    if let Some(uid) = new_active {
        crate::core::sync::session::run_session(app.clone(), Arc::clone(state.inner()), uid).await;
    }

    Ok(())
}

#[tauri::command]
pub async fn set_active_pairing(
    args: UserScopedArgs,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), AppError> {
    state.registry.set_active_persisted(Some(args.user_id.clone())).await?;
    app.emit(
        ACTIVE_PAIRING_CHANGED,
        crate::events::ActivePairingChanged {
            user_id: Some(args.user_id.clone()),
        },
    )
    .ok();
    crate::core::sync::session::run_session(app.clone(), Arc::clone(state.inner()), args.user_id)
        .await;
    Ok(())
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
) -> Result<Vec<EntryView>, AppError> {
    let conn = state.conn.lock().await;
    let rows = entries_cache::list_recent(&conn, &args.user_id, args.before_id, args.limit)?;
    let labels = devices::map_for(&conn, &args.user_id)?;
    Ok(to_entry_views(rows, &labels))
}

/// Cached rows in wire shape, with each Origin resolved against the Device
/// mirror.
///
/// A `device_id` the mirror has never heard of keeps a `None` label rather
/// than failing: a device paired since the last `GET /me`, or a relay too old
/// to serve one, is expected — the UI falls back to a slice of the id.
fn to_entry_views(
    rows: Vec<entries_cache::CachedEntry>,
    labels: &HashMap<String, String>,
) -> Vec<EntryView> {
    rows.into_iter()
        .map(|r| {
            let device_label = labels.get(&r.device_id).cloned();
            EntryView {
                id: r.id,
                user_id: r.user_id,
                preview: r.plaintext.unwrap_or_default(),
                created_at: r.created_at,
                device_id: r.device_id,
                device_label,
            }
        })
        .collect()
}

/// Contact for one user.
///
/// Reads the live cell while a session holds one and falls back to the
/// persisted value otherwise: the popover opens long after the last
/// `contact` event fired and has to hydrate from somewhere.
#[tauri::command]
pub async fn get_contact(
    args: UserScopedArgs,
    state: State<'_, Arc<AppState>>,
) -> Result<ContactEvent, AppError> {
    let live = state
        .last_contact
        .lock()
        .get(&args.user_id)
        .map(|c| c.load(Ordering::Relaxed))
        .filter(|at| *at != 0);
    let last_contact_at = match live {
        Some(at) => Some(at),
        None => {
            let conn = state.conn.lock().await;
            accounts_repo::find(&conn, &args.user_id)?.and_then(|a| a.last_contact_at)
        }
    };
    Ok(ContactEvent { user_id: args.user_id, last_contact_at })
}

#[derive(Deserialize)]
pub(crate) struct EntryScopedArgs {
    pub(crate) user_id: String,
    pub(crate) entry_id: i64,
}

fn set_clipboard_text_with_self_write_guard<F>(
    last_self_write: &parking_lot::Mutex<Option<(Instant, String)>>,
    plaintext: String,
    set_text: F,
) -> Result<(), AppError>
where
    F: FnOnce(String) -> Result<(), AppError>,
{
    *last_self_write.lock() = Some((Instant::now(), plaintext.clone()));
    if let Err(err) = set_text(plaintext.clone()) {
        let mut marker = last_self_write.lock();
        if marker
            .as_ref()
            .map(|(_, marked_text)| marked_text == &plaintext)
            .unwrap_or(false)
        {
            *marker = None;
        }
        return Err(err);
    }
    Ok(())
}

#[tauri::command]
pub async fn copy_to_clipboard(
    args: EntryScopedArgs,
    state: State<'_, Arc<AppState>>,
) -> Result<(), AppError> {
    let plaintext = {
        let conn = state.conn.lock().await;
        entries_cache::get_full(&conn, &args.user_id, args.entry_id)?
            .ok_or_else(|| AppError::NotFound("plaintext unavailable".into()))?
    };
    let mut cb = arboard::Clipboard::new().map_err(|e| AppError::Storage(e.to_string()))?;
    set_clipboard_text_with_self_write_guard(&state.last_self_write, plaintext, |text| {
        cb.set_text(text)
            .map_err(|e| AppError::Storage(e.to_string()))
    })
}

#[tauri::command]
pub async fn delete_entry(
    args: EntryScopedArgs,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), AppError> {
    let m = state.registry.load_active_membership(&args.user_id).await?;
    m.server.delete_entry(args.entry_id).await?;
    let conn = state.conn.lock().await;
    entries_cache::delete_one(&conn, &args.user_id, args.entry_id)?;
    app.emit(
        crate::events::ENTRY_DELETED,
        crate::events::EntryDeleted {
            user_id: args.user_id,
            entry_id: args.entry_id,
        },
    )
    .ok();
    Ok(())
}

#[tauri::command]
pub async fn clear_history(
    args: UserScopedArgs,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), AppError> {
    let m = state.registry.load_active_membership(&args.user_id).await?;
    m.server.delete_all_entries().await?;
    let conn = state.conn.lock().await;
    entries_cache::delete_all(&conn, &args.user_id)?;
    app.emit(
        HISTORY_CHANGED,
        serde_json::json!({ "user_id": args.user_id }),
    )
    .ok();
    Ok(())
}

#[tauri::command]
pub async fn get_settings(
    state: State<'_, Arc<AppState>>,
) -> Result<settings::Settings, AppError> {
    let conn = state.conn.lock().await;
    settings::load(&conn)
}

#[tauri::command]
pub async fn update_settings(
    patch: serde_json::Value,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<settings::Settings, AppError> {
    let mut hotkey_changed: Option<Option<String>> = None;
    let mut autostart_changed: Option<bool> = None;
    let s = {
        let conn = state.conn.lock().await;
        let mut s = settings::load(&conn)?;
        if let Some(v) = patch.get("capture_enabled").and_then(|v| v.as_bool()) {
            s.capture_enabled = v;
        }
        if let Some(arr) = patch.get("deny_list").and_then(|v| v.as_array()) {
            s.deny_list = arr
                .iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect();
        }
        if let Some(v) = patch.get("autostart").and_then(|v| v.as_bool()) {
            if v != s.autostart {
                autostart_changed = Some(v);
            }
            s.autostart = v;
        }
        if let Some(v) = patch.get("update_check_enabled").and_then(|v| v.as_bool()) {
            s.update_check_enabled = v;
        }
        if let Some(v) = patch.get("hotkey") {
            let new_hotkey = if v.is_null() {
                None
            } else {
                v.as_str().map(String::from)
            };
            if new_hotkey != s.hotkey {
                hotkey_changed = Some(new_hotkey.clone());
            }
            s.hotkey = new_hotkey;
        }
        settings::save(&conn, &s)?;
        s
    };
    if let Some(new_hotkey) = hotkey_changed {
        if let Err(e) = crate::apply_hotkey(&app, new_hotkey.as_deref()) {
            tracing::warn!(err = %e, "re-register hotkey failed");
        }
    }
    // The choice is already persisted above; a failed login-item write is logged
    // and swallowed so the user does not silently lose the setting.
    if let Some(enabled) = autostart_changed {
        if let Err(e) = crate::set_autostart(&app, enabled) {
            tracing::warn!(err = %e, enabled, "update autostart registration failed");
        }
    }
    Ok(s)
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

async fn activate_and_sync(app: &AppHandle, state: &Arc<AppState>, user_id: &str) {
    if let Err(e) = state
        .registry
        .set_active_persisted(Some(user_id.to_string()))
        .await
    {
        tracing::warn!(err = %e, "persisting the Active Pairing failed");
    }
    let _ = app.emit(
        ACTIVE_PAIRING_CHANGED,
        crate::events::ActivePairingChanged {
            user_id: Some(user_id.to_string()),
        },
    );
    crate::core::sync::session::run_session(app.clone(), Arc::clone(state), user_id.to_string())
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;

    fn cached(id: i64, device_id: &str) -> entries_cache::CachedEntry {
        entries_cache::CachedEntry {
            user_id: "u".into(),
            id,
            ciphertext: vec![1],
            plaintext: Some(format!("entry {id}")),
            created_at: 1_000 + id,
            device_id: device_id.into(),
        }
    }

    #[test]
    fn list_history_labels_origins_from_the_device_mirror() {
        let labels = HashMap::from([("d1".to_string(), "IPHONE-15".to_string())]);
        let views = to_entry_views(vec![cached(1, "d1")], &labels);
        assert_eq!(views[0].device_label.as_deref(), Some("IPHONE-15"));
        assert_eq!(views[0].device_id, "d1");
        assert_eq!(views[0].preview, "entry 1");
    }

    #[test]
    fn list_history_tolerates_a_device_id_the_mirror_has_never_seen() {
        let labels = HashMap::from([("d1".to_string(), "IPHONE-15".to_string())]);
        let views = to_entry_views(vec![cached(1, "d1"), cached(2, "unpaired-yesterday")], &labels);
        assert_eq!(views[0].device_label.as_deref(), Some("IPHONE-15"));
        assert_eq!(views[1].device_label, None, "unmirrored origin must not fail the row");
        assert_eq!(views[1].device_id, "unpaired-yesterday");
    }

    #[test]
    fn list_history_maps_an_undecryptable_entry_to_an_empty_preview() {
        let mut row = cached(1, "d1");
        row.plaintext = None;
        let views = to_entry_views(vec![row], &HashMap::new());
        assert_eq!(views[0].preview, "");
    }

    #[test]
    fn self_write_guard_sets_marker_before_clipboard_write() {
        let last_self_write = Mutex::new(None);
        let plaintext = "secret".to_string();

        set_clipboard_text_with_self_write_guard(&last_self_write, plaintext.clone(), |text| {
            let marker = last_self_write.lock();
            let (_, marked_text) = marker.as_ref().expect("self-write marker should be set");
            assert_eq!(marked_text, &plaintext);
            assert_eq!(text, plaintext);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn self_write_guard_clears_matching_marker_when_clipboard_write_fails() {
        let last_self_write = Mutex::new(None);
        let plaintext = "secret".to_string();

        let err =
            set_clipboard_text_with_self_write_guard(&last_self_write, plaintext.clone(), |text| {
                let marker = last_self_write.lock();
                let (_, marked_text) = marker.as_ref().expect("self-write marker should be set");
                assert_eq!(marked_text, &plaintext);
                assert_eq!(text, plaintext);
                drop(marker);
                Err(AppError::Storage("clipboard failed".into()))
            })
            .unwrap_err();

        assert!(matches!(err, AppError::Storage(_)));
        assert!(last_self_write.lock().is_none());
    }
}
