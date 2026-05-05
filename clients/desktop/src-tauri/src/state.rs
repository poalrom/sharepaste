use crate::config::Paths;
use crate::core::account::AccountRegistry;
use crate::core::keychain::Keychain;
use crate::core::sync::ConnectionState;
use parking_lot::Mutex;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

pub struct SyncSlot {
    pub user_id: String,
    pub cancel: CancellationToken,
}

pub struct AppState {
    pub paths: Paths,
    pub conn: Arc<tokio::sync::Mutex<Connection>>,
    pub keychain: Arc<dyn Keychain>,
    pub registry: Arc<AccountRegistry>,
    pub sync_tasks: Mutex<HashMap<String, SyncSlot>>,
    pub upload_triggers: Mutex<HashMap<String, Arc<Notify>>>,
    pub conn_states: Mutex<HashMap<String, ConnectionState>>,
    pub last_self_write: Mutex<Option<(std::time::Instant, String)>>,
    pub last_tray_rect: Mutex<Option<tauri::Rect>>,
}

impl AppState {
    pub fn new(
        paths: Paths,
        conn: Arc<tokio::sync::Mutex<Connection>>,
        keychain: Arc<dyn Keychain>,
        registry: Arc<AccountRegistry>,
    ) -> Self {
        Self {
            paths,
            conn,
            keychain,
            registry,
            sync_tasks: Mutex::new(HashMap::new()),
            upload_triggers: Mutex::new(HashMap::new()),
            conn_states: Mutex::new(HashMap::new()),
            last_self_write: Mutex::new(None),
            last_tray_rect: Mutex::new(None),
        }
    }
}
