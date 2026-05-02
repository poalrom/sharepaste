use serde::Serialize;

pub const ACCOUNT_ADDED: &str   = "account-added";
pub const ACCOUNT_REMOVED: &str = "account-removed";
pub const ACTIVE_CHANGED: &str  = "active-changed";
pub const CONNECTION_STATE: &str = "connection-state";
pub const ENTRY_ADDED: &str     = "entry-added";
pub const ENTRY_DELETED: &str   = "entry-deleted";
pub const HISTORY_CHANGED: &str = "history-changed";
pub const PENDING_COUNT: &str   = "pending-count";
pub const CAPTURE_SKIPPED: &str = "capture-skipped";
pub const DECRYPTION_ERROR: &str = "decryption-error";
pub const PAIR_SHORTCODE: &str  = "pair-shortcode";
pub const PAIR_CLAIMED: &str    = "pair-claimed";
pub const PAIR_EXPIRED: &str    = "pair-expired";

#[derive(Serialize, Clone)]
pub struct AccountAdded { pub user_id: String, pub device_id: String, pub label: String }

#[derive(Serialize, Clone)]
pub struct AccountRemoved { pub user_id: String }

#[derive(Serialize, Clone)]
pub struct ActiveChanged { pub user_id: Option<String> }

#[derive(Serialize, Clone)]
pub struct ConnectionStateEvent {
    pub user_id: String,
    pub state: crate::core::sync::ConnectionState,
    pub last_error: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct EntryAdded { pub user_id: String, pub entry: EntryView }

#[derive(Serialize, Clone)]
pub struct EntryView {
    pub id: i64,
    pub user_id: String,
    pub preview: String,
    pub created_at: i64,
    pub device_id: String,
    pub device_label: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct EntryDeleted { pub user_id: String, pub entry_id: i64 }

#[derive(Serialize, Clone)]
pub struct PendingCount { pub user_id: String, pub count: i64 }

#[derive(Serialize, Clone)]
pub struct CaptureSkipped { pub reason: String, pub source_app: Option<String> }

#[derive(Serialize, Clone)]
pub struct DecryptionError { pub user_id: String, pub entry_id: i64 }

#[derive(Serialize, Clone)]
pub struct PairShortcode { pub code: String, pub expires_at: i64 }

#[derive(Serialize, Clone)]
pub struct PairClaimed { pub user_id: String }
