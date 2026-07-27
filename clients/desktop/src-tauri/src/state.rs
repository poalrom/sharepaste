use crate::core::pairing::registry::PairingRegistry;
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
    pub(crate) registry: Arc<PairingRegistry>,
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
    /// The Release the last check found, held because three callers need the
    /// same answer: the tray item's presence, the Settings pane, and the
    /// install path — which needs the very `Update` whose signature verified.
    pub(crate) pending_update: Mutex<Option<tauri_plugin_updater::Update>>,
    /// The tray menu, so the install item can be added and removed after
    /// `build_tray` has finished.
    ///
    /// Every other tray item is built once and never touched; this one is the
    /// exception, and reaching it from a check that runs on the async runtime
    /// means the menu has to outlive the setup hook.
    pub(crate) tray_menu: Mutex<Option<tauri::menu::Menu<tauri::Wry>>>,
}

impl AppState {
    pub(crate) fn new(
        conn: Arc<tokio::sync::Mutex<Connection>>,
        keychain: Arc<dyn Keychain>,
        registry: Arc<PairingRegistry>,
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
            pending_update: Mutex::new(None),
            tray_menu: Mutex::new(None),
        }
    }
}
