//! The desktop's wire format, and the one adapter that produces it.
//!
//! Every name and payload here is what `ui/` already listens for. The sink
//! below is a pure mapping from [`CoreEvent`] onto them — no logic, no second
//! event path, nothing the core does not already know.

use sharepaste_core::event::CoreEvent;
use sharepaste_core::platform::EventSink;
use serde::Serialize;
use tauri::Emitter;

pub(crate) const PAIRING_ADDED: &str   = "pairing-added";
pub(crate) const PAIRING_REMOVED: &str = "pairing-removed";
pub(crate) const ACTIVE_PAIRING_CHANGED: &str  = "active-pairing-changed";
pub(crate) const CONNECTION_STATE: &str = "connection-state";
pub(crate) const ENTRY_ADDED: &str     = "entry-added";
pub(crate) const ENTRY_DELETED: &str   = "entry-deleted";
pub(crate) const ENTRY_SETTLED: &str   = "entry-settled";
pub(crate) const ENTRY_REFUSED: &str   = "entry-refused";
pub(crate) const HISTORY_CHANGED: &str = "history-changed";
pub(crate) const PENDING_COUNT: &str   = "pending-count";
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
    pub(crate) state: sharepaste_core::sync::ConnectionState,
    pub(crate) last_error: Option<String>,
}

/// The entry travels as the core built it.
///
/// `Entry` carries `Serialize` for exactly this: a shell-side copy of the struct
/// would be one more place for a field to go missing, which is how the
/// `undecryptable` flag came to be inferred from an empty preview in the first
/// place.
#[derive(Serialize, Clone)]
pub(crate) struct EntryAdded { pub user_id: String, pub entry: sharepaste_core::event::Entry }

#[derive(Serialize, Clone)]
pub(crate) struct EntryDeleted { pub user_id: String, pub entry_id: i64 }

/// One act reached the relay, so its row may have stopped waiting.
///
/// The row is addressed by its own id, which a flush does not change, so a shell
/// updates it in place. Deliberately not `history-changed`: nothing reorders at a
/// flush, and a refetch per acked act is the cost this distinction exists to
/// avoid.
#[derive(Serialize, Clone)]
pub(crate) struct EntrySettled { pub user_id: String, pub entry_id: i64 }

/// The relay turned an act down for what it is, and said this.
#[derive(Serialize, Clone)]
pub(crate) struct EntryRefused { pub user_id: String, pub entry_id: i64, pub reason: String }

#[derive(Serialize, Clone)]
pub(crate) struct PendingCount { pub user_id: String, pub count: i64 }

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

/// Emits the core's events into the Tauri event bus.
pub(crate) struct TauriEventSink {
    app: tauri::AppHandle,
}

impl TauriEventSink {
    pub(crate) fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl EventSink for TauriEventSink {
    fn emit(&self, event: CoreEvent) {
        // A closed webview is the normal way an emit fails, and there is nothing
        // the protocol could do about it — the same reason every call site here
        // used to end in `.ok()`.
        match event {
            CoreEvent::PairingAdded { user_id, device_id, label } => {
                let _ = self.app.emit(PAIRING_ADDED, PairingAdded { user_id, device_id, label });
            }
            CoreEvent::PairingRemoved { user_id } => {
                let _ = self.app.emit(PAIRING_REMOVED, PairingRemoved { user_id });
            }
            CoreEvent::ActivePairingChanged { user_id } => {
                let _ = self.app.emit(ACTIVE_PAIRING_CHANGED, ActivePairingChanged { user_id });
            }
            CoreEvent::ConnectionState { user_id, state, last_error } => {
                let _ = self
                    .app
                    .emit(CONNECTION_STATE, ConnectionStateEvent { user_id, state, last_error });
            }
            CoreEvent::EntryAdded { user_id, entry } => {
                let _ = self.app.emit(ENTRY_ADDED, EntryAdded { user_id, entry });
            }
            CoreEvent::EntryDeleted { user_id, entry_id } => {
                let _ = self.app.emit(ENTRY_DELETED, EntryDeleted { user_id, entry_id });
            }
            CoreEvent::HistoryChanged { user_id } => {
                let _ = self.app.emit(HISTORY_CHANGED, serde_json::json!({ "user_id": user_id }));
            }
            CoreEvent::PendingCount { user_id, count } => {
                let _ = self.app.emit(PENDING_COUNT, PendingCount { user_id, count });
            }
            CoreEvent::Contact { user_id, last_contact_at } => {
                let _ = self.app.emit(CONTACT_EVENT, ContactEvent { user_id, last_contact_at });
            }
            CoreEvent::PairShortcode { code, expires_at } => {
                let _ = self.app.emit(PAIR_SHORTCODE, PairShortcode { code, expires_at });
            }
            CoreEvent::EntrySettled { user_id, entry_id } => {
                let _ = self.app.emit(ENTRY_SETTLED, EntrySettled { user_id, entry_id });
            }
            CoreEvent::EntryRefused { user_id, entry_id, reason } => {
                let _ = self.app.emit(ENTRY_REFUSED, EntryRefused { user_id, entry_id, reason });
            }
            CoreEvent::PairClaimed { user_id, device_label } => {
                let _ = self.app.emit(PAIR_CLAIMED, PairClaimed { user_id, device_label });
            }
            CoreEvent::PairExpired => {
                let _ = self.app.emit(PAIR_EXPIRED, ());
            }
        }
    }
}
