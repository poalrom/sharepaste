use serde::Serialize;

pub(crate) const PAIRING_ADDED: &str   = "pairing-added";
pub(crate) const PAIRING_REMOVED: &str = "pairing-removed";
pub(crate) const ACTIVE_PAIRING_CHANGED: &str  = "active-pairing-changed";
pub(crate) const CONNECTION_STATE: &str = "connection-state";
pub(crate) const ENTRY_ADDED: &str     = "entry-added";
pub(crate) const ENTRY_DELETED: &str   = "entry-deleted";
pub(crate) const HISTORY_CHANGED: &str = "history-changed";
pub(crate) const PENDING_COUNT: &str   = "pending-count";
pub(crate) const DECRYPTION_ERROR: &str = "decryption-error";
pub(crate) const CONTACT_EVENT: &str      = "contact";
pub(crate) const PAIR_SHORTCODE: &str  = "pair-shortcode";
pub(crate) const PAIR_CLAIMED: &str    = "pair-claimed";
pub(crate) const PAIR_EXPIRED: &str    = "pair-expired";
pub(crate) const MAIN_NAVIGATE: &str   = "main://navigate";
pub(crate) const UPDATE_AVAILABLE: &str = "update-available";

#[derive(Serialize, Clone)]
pub(crate) struct PairingAdded { pub user_id: String, pub device_id: String, pub label: String }

#[derive(Serialize, Clone)]
pub(crate) struct PairingRemoved { pub user_id: String }

#[derive(Serialize, Clone)]
pub(crate) struct ActivePairingChanged { pub user_id: Option<String> }

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

/// Contact for one user, as the popover renders it: `LAST CONTACT 3m AGO`.
///
/// `None` means this device has never heard from the relay for this user.
#[derive(Serialize, Clone)]
pub(crate) struct ContactEvent { pub user_id: String, pub last_contact_at: Option<i64> }

#[derive(Serialize, Clone)]
pub(crate) struct PairShortcode { pub code: String, pub expires_at: i64 }

#[derive(Serialize, Clone)]
pub(crate) struct PairClaimed { pub user_id: String, pub device_label: Option<String> }

/// Where an already-open main window should navigate to.
///
/// `entry_id` rides along so the popover's History icon can hand the reader the
/// row it had selected; `None` leaves the pane on its own selection.
#[derive(Serialize, Clone)]
pub(crate) struct MainNavigate { pub section: String, pub entry_id: Option<i64> }

/// A Release the Update Source is offering and this device does not have.
///
/// `notes` is the changelog section the pipeline put in `latest.json`; the
/// prompt shows it verbatim, so it is written for a user rather than lifted
/// from the commit log.
#[derive(Serialize, Clone)]
pub(crate) struct UpdateAvailable { pub version: String, pub notes: Option<String> }
