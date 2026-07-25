use serde::Serialize;

pub(crate) const ACCOUNT_ADDED: &str   = "account-added";
pub(crate) const ACCOUNT_REMOVED: &str = "account-removed";
pub(crate) const ACTIVE_CHANGED: &str  = "active-changed";
pub(crate) const CONNECTION_STATE: &str = "connection-state";
pub(crate) const ENTRY_ADDED: &str     = "entry-added";
pub(crate) const ENTRY_DELETED: &str   = "entry-deleted";
pub(crate) const HISTORY_CHANGED: &str = "history-changed";
pub(crate) const PENDING_COUNT: &str   = "pending-count";
pub(crate) const DECRYPTION_ERROR: &str = "decryption-error";
pub(crate) const PAIR_SHORTCODE: &str  = "pair-shortcode";
pub(crate) const PAIR_CLAIMED: &str    = "pair-claimed";
pub(crate) const PAIR_EXPIRED: &str    = "pair-expired";
pub(crate) const MAIN_NAVIGATE: &str   = "main://navigate";

#[derive(Serialize, Clone)]
pub(crate) struct AccountAdded { pub user_id: String, pub device_id: String, pub label: String }

#[derive(Serialize, Clone)]
pub(crate) struct AccountRemoved { pub user_id: String }

#[derive(Serialize, Clone)]
pub(crate) struct ActiveChanged { pub user_id: Option<String> }

#[derive(Serialize, Clone)]
pub(crate) struct ConnectionStateEvent {
    pub(crate) user_id: String,
    pub(crate) state: crate::core::sync::ConnectionState,
    pub(crate) last_error: Option<String>,
}

#[derive(Serialize, Clone)]
pub(crate) struct EntryAdded { pub user_id: String, pub entry: EntryView }

#[derive(Serialize, Clone)]
pub(crate) struct EntryView {
    pub(crate) id: i64,
    pub(crate) user_id: String,
    pub(crate) preview: String,
    pub(crate) created_at: i64,
    pub(crate) device_id: String,
    pub(crate) device_label: Option<String>,
}

#[derive(Serialize, Clone)]
pub(crate) struct EntryDeleted { pub user_id: String, pub entry_id: i64 }

#[derive(Serialize, Clone)]
pub(crate) struct PendingCount { pub user_id: String, pub count: i64 }

#[derive(Serialize, Clone)]
pub(crate) struct DecryptionError { pub user_id: String, pub entry_id: i64 }

#[derive(Serialize, Clone)]
pub(crate) struct PairShortcode { pub code: String, pub expires_at: i64 }

#[derive(Serialize, Clone)]
pub(crate) struct PairClaimed { pub user_id: String, pub device_label: Option<String> }
