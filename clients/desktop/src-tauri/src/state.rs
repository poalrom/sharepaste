use crate::core::account::AccountRegistry;
use crate::core::keychain::Keychain;
use crate::core::sync::ConnectionState;
use parking_lot::Mutex;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::atomic::AtomicI64;
use std::sync::Arc;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

pub(crate) struct AppState {
    pub(crate) conn: Arc<tokio::sync::Mutex<Connection>>,
    pub(crate) keychain: Arc<dyn Keychain>,
    pub(crate) registry: Arc<AccountRegistry>,
    pub(crate) sync_tasks: Mutex<HashMap<String, CancellationToken>>,
    pub(crate) upload_triggers: Mutex<HashMap<String, Arc<Notify>>>,
    pub(crate) conn_states: Mutex<HashMap<String, ConnectionState>>,
    /// Live Contact, one cell per user with a running session.
    ///
    /// The SSE byte tap stores into these on every chunk the relay sends, so
    /// the reading stays out of SQLite entirely while the session is up.
    /// `get_contact` reads a cell directly; `set_conn_state` flushes it on
    /// the way offline. A cell outlives its session so the last reading is
    /// still answerable after the stream drops.
    pub(crate) last_contact: Mutex<HashMap<String, Arc<AtomicI64>>>,
    pub(crate) last_self_write: Mutex<Option<(std::time::Instant, String)>>,
    /// Plaintext of the last clipboard capture that was enqueued, used to drop
    /// consecutive duplicates before they cost an encrypt, upload or server row.
    pub(crate) last_capture: Mutex<Option<String>>,
    pub(crate) last_tray_rect: Mutex<Option<tauri::Rect>>,
}

impl AppState {
    pub(crate) fn new(
        conn: Arc<tokio::sync::Mutex<Connection>>,
        keychain: Arc<dyn Keychain>,
        registry: Arc<AccountRegistry>,
    ) -> Self {
        Self {
            conn,
            keychain,
            registry,
            sync_tasks: Mutex::new(HashMap::new()),
            upload_triggers: Mutex::new(HashMap::new()),
            conn_states: Mutex::new(HashMap::new()),
            last_contact: Mutex::new(HashMap::new()),
            last_self_write: Mutex::new(None),
            last_capture: Mutex::new(None),
            last_tray_rect: Mutex::new(None),
        }
    }
}
