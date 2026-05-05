use crate::core::http::ServerClient;
use crate::core::keychain::{token_account, user_key_account};
use crate::core::pairing::invite::{claim_invite, persist_claimed_account};
use crate::core::pairing::qr::{
    fetch_and_decrypt_pair_payload, secret_proof_hex, start_pair, upload_pair_payload,
};
use crate::core::pairing::shortcode::{decode as decode_shortcode, group_for_display};
use crate::core::storage::{accounts as accounts_repo, entries_cache, pending, settings};
use crate::core::sync::ConnectionState;
use crate::errors::AppError;
use crate::events::{
    AccountAdded, PairShortcode, ACCOUNT_ADDED, ACTIVE_CHANGED, HISTORY_CHANGED, PAIR_SHORTCODE,
};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use zeroize::Zeroizing;

#[derive(Serialize)]
pub struct AccountSummary {
    pub user_id: String,
    pub device_id: String,
    pub label: String,
    pub server_url: String,
    pub status: ConnectionState,
    pub pending: i64,
    pub is_active: bool,
}

#[derive(Serialize)]
pub struct EntryViewDto {
    pub id: i64,
    pub user_id: String,
    pub preview: String,
    pub created_at: i64,
    pub device_id: String,
    pub device_label: Option<String>,
}

#[tauri::command]
pub async fn list_accounts(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<AccountSummary>, AppError> {
    let accts = state.registry.list().await?;
    let active = state.registry.active_user_id();
    let mut out = Vec::with_capacity(accts.len());
    let conn = state.conn.lock().await;
    for a in accts {
        let pending = pending::count(&conn, &a.user_id)?;
        let is_active = active.as_deref() == Some(&a.user_id);
        let status = if is_active {
            ConnectionState::Connecting
        } else {
            ConnectionState::Disconnected
        };
        out.push(AccountSummary {
            user_id: a.user_id,
            device_id: a.device_id,
            label: a.device_label,
            server_url: a.server_url,
            status,
            pending,
            is_active,
        });
    }
    Ok(out)
}

#[derive(Deserialize)]
pub struct PairWithInviteArgs {
    pub server_url: String,
    pub token: String,
    pub device_label: String,
}

#[derive(Serialize)]
pub struct PairWithInviteResp {
    pub user_id: String,
    pub device_id: String,
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
        persist_claimed_account(
            &conn,
            state.keychain.as_ref(),
            &claimed,
            &args.device_label,
            now_ms(),
        )?;
    }
    app.emit(
        ACCOUNT_ADDED,
        AccountAdded {
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
pub struct PairStartArgs {
    pub user_id: String,
}

#[derive(Serialize)]
pub struct PairStartResp {
    pub code: String,
    pub expires_at: i64,
}

#[tauri::command]
pub async fn pair_start(
    args: PairStartArgs,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<PairStartResp, AppError> {
    let m = state.registry.load_active_membership(&args.user_id).await?;
    let started = start_pair(&m.server).await?;
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

    // Spawn pair-watch task that polls /pair/poll, uploads payload on `claimed`,
    // and emits pair-claimed / pair-expired.
    let server = m.server.clone();
    let user_id = args.user_id.clone();
    // UserKey (Zeroizing<[u8;32]>) does not impl Clone; clone the inner array
    // through a fresh Zeroizing wrapper so the spawned task owns its own key.
    let user_key: crate::core::crypto::UserKey = Zeroizing::new(*m.user_key);
    let pair_id = started.pair_id;
    let pairing_secret = *started.pairing_secret;
    let app2 = app.clone();
    tokio::spawn(async move {
        loop {
            match server.pair_poll(&pair_id.to_string(), 25_000).await {
                Ok(p) if p.status == "claimed" => {
                    let server_url = server.base().to_string();
                    if let Err(e) = upload_pair_payload(
                        &server,
                        pair_id,
                        &pairing_secret,
                        &user_id,
                        &user_key,
                        &server_url,
                    )
                    .await
                    {
                        tracing::warn!(err = %e, "pair payload upload failed");
                    } else {
                        let _ = app2.emit(
                            crate::events::PAIR_CLAIMED,
                            crate::events::PairClaimed {
                                user_id: user_id.clone(),
                            },
                        );
                    }
                    return;
                }
                Ok(p) if p.status == "consumed" || p.status == "expired" => {
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
pub struct PairWithCodeArgs {
    pub code: String,
    pub device_label: String,
}

#[derive(Serialize)]
pub struct PairWithCodeResp {
    pub user_id: String,
    pub device_id: String,
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
            },
        )?;
    }
    app.emit(
        ACCOUNT_ADDED,
        AccountAdded {
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
pub struct UserScopedArgs {
    pub user_id: String,
}

#[tauri::command]
pub async fn forget_account(
    args: UserScopedArgs,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), AppError> {
    state.registry.forget(&args.user_id).await?;
    app.emit(
        crate::events::ACCOUNT_REMOVED,
        crate::events::AccountRemoved {
            user_id: args.user_id.clone(),
        },
    )
    .ok();
    Ok(())
}

#[derive(Deserialize)]
pub struct RevokeDeviceArgs {
    pub user_id: String,
    pub device_id: String,
}

#[tauri::command]
pub async fn revoke_device(
    args: RevokeDeviceArgs,
    state: State<'_, Arc<AppState>>,
) -> Result<(), AppError> {
    let m = state.registry.load_active_membership(&args.user_id).await?;
    m.server.revoke_device(&args.device_id).await
}

#[tauri::command]
pub async fn set_active_account(
    args: UserScopedArgs,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), AppError> {
    state.registry.set_active(Some(args.user_id.clone()));
    app.emit(
        ACTIVE_CHANGED,
        crate::events::ActiveChanged {
            user_id: Some(args.user_id),
        },
    )
    .ok();
    Ok(())
}

#[derive(Deserialize)]
pub struct ListHistoryArgs {
    pub user_id: String,
    pub before_id: Option<i64>,
    pub limit: i64,
}

#[tauri::command]
pub async fn list_history(
    args: ListHistoryArgs,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<EntryViewDto>, AppError> {
    let conn = state.conn.lock().await;
    let rows = entries_cache::list_recent(&conn, &args.user_id, args.before_id, args.limit)?;
    Ok(rows
        .into_iter()
        .map(|r| EntryViewDto {
            id: r.id,
            user_id: r.user_id,
            preview: r.plaintext.unwrap_or_default(),
            created_at: r.created_at,
            device_id: r.device_id,
            device_label: None,
        })
        .collect())
}

#[derive(Deserialize)]
pub struct SearchHistoryArgs {
    pub user_id: String,
    pub query: String,
    pub limit: i64,
}

#[tauri::command]
pub async fn search_history(
    args: SearchHistoryArgs,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<EntryViewDto>, AppError> {
    let conn = state.conn.lock().await;
    let rows = entries_cache::search(&conn, &args.user_id, &args.query, args.limit)?;
    Ok(rows
        .into_iter()
        .map(|r| EntryViewDto {
            id: r.id,
            user_id: r.user_id,
            preview: r
                .plaintext
                .as_deref()
                .map(crate::core::sync::decryptor::build_preview)
                .unwrap_or_default(),
            created_at: r.created_at,
            device_id: r.device_id,
            device_label: None,
        })
        .collect())
}

#[derive(Deserialize)]
pub struct EntryScopedArgs {
    pub user_id: String,
    pub entry_id: i64,
}

#[tauri::command]
pub async fn get_entry_full(
    args: EntryScopedArgs,
    state: State<'_, Arc<AppState>>,
) -> Result<String, AppError> {
    let conn = state.conn.lock().await;
    entries_cache::get_full(&conn, &args.user_id, args.entry_id)?
        .ok_or_else(|| AppError::NotFound("plaintext unavailable".into()))
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
            s.autostart = v;
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
    Ok(s)
}

#[derive(Serialize)]
pub struct StatusResp {
    pub state: ConnectionState,
    pub pending_count: i64,
    pub last_error: Option<String>,
}

#[tauri::command]
pub async fn get_status(
    args: UserScopedArgs,
    state: State<'_, Arc<AppState>>,
) -> Result<StatusResp, AppError> {
    let conn = state.conn.lock().await;
    let count = pending::count(&conn, &args.user_id)?;
    let active = state.registry.active_user_id();
    let st = if active.as_deref() == Some(&args.user_id) {
        ConnectionState::Connecting
    } else {
        ConnectionState::Disconnected
    };
    Ok(StatusResp {
        state: st,
        pending_count: count,
        last_error: None,
    })
}

#[derive(Deserialize)]
pub struct OpenModalArgs {
    pub kind: String,
}

#[tauri::command]
pub async fn open_modal(app: AppHandle, args: OpenModalArgs) -> Result<(), AppError> {
    let kind = args.kind;
    if !matches!(kind.as_str(), "pairing" | "settings" | "accounts") {
        return Err(AppError::BadInput(format!("unknown modal kind: {kind}")));
    }
    let label = format!("modal-{kind}");
    if let Some(existing) = app.get_webview_window(&label) {
        existing
            .set_focus()
            .map_err(|e| AppError::BadInput(e.to_string()))?;
        return Ok(());
    }
    let url = format!("modal.html?kind={kind}");
    WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url.into()))
        .title("sharepaste")
        .inner_size(420.0, 520.0)
        .resizable(false)
        .build()
        .map_err(|e| AppError::BadInput(e.to_string()))?;
    Ok(())
}

#[tauri::command]
pub async fn hide_popover(app: AppHandle) -> Result<(), AppError> {
    if let Some(win) = app.get_webview_window("popover") {
        win.hide().map_err(|e| AppError::BadInput(e.to_string()))?;
    }
    Ok(())
}

async fn activate_and_sync(app: &AppHandle, state: &Arc<AppState>, user_id: &str) {
    state.registry.set_active(Some(user_id.to_string()));
    let _ = app.emit(
        ACTIVE_CHANGED,
        crate::events::ActiveChanged {
            user_id: Some(user_id.to_string()),
        },
    );
    crate::spawn_sync(app.clone(), Arc::clone(state), user_id.to_string()).await;
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;

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
