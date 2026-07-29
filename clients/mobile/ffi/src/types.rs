//! The data a foreign shell sees, and the conversions from the core's own.
//!
//! Each type here restates a core type. The duplication buys one thing: the
//! core stays free to hold `Instant`s, `PathBuf`s and doubly-optional patches,
//! while the boundary carries only what a foreign binding can spell. Every
//! conversion is `From`, one direction, so a field added to the core is a
//! compile error here rather than a field a phone silently never sees.

use sharepaste_core::capture::filter::SkipReason as CoreSkipReason;
use sharepaste_core::event::{CoreEvent as CoreCoreEvent, Entry as CoreEntry};
use sharepaste_core::facade::{
    Contact as CoreContact, OfferOutcome as CoreOfferOutcome, PairedDevice as CorePairedDevice,
    PairingSummary as CorePairingSummary, RecallSource as CoreRecallSource,
    Recalled as CoreRecalled, SettingsPatch as CoreSettingsPatch, ShortCode as CoreShortCode,
};
use sharepaste_core::storage::settings::Settings as CoreSettings;
use sharepaste_core::sync::ConnectionState as CoreConnectionState;

/// How a pairing's link to its relay currently reads.
///
/// On a phone, `Disconnected` is the **nominal** case, not a fault: the sync
/// model is foreground-only, so a backgrounded app is disconnected by design.
/// A shell that paints this red is wrong. `AuthFailed` is the one that is
/// genuinely broken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Online,
    AuthFailed,
}

impl From<CoreConnectionState> for ConnectionState {
    fn from(s: CoreConnectionState) -> Self {
        match s {
            CoreConnectionState::Disconnected => ConnectionState::Disconnected,
            CoreConnectionState::Connecting => ConnectionState::Connecting,
            CoreConnectionState::Online => ConnectionState::Online,
            CoreConnectionState::AuthFailed => ConnectionState::AuthFailed,
        }
    }
}

/// One entry as a shell renders it.
///
/// `preview` is the **Preview** on every path — the decrypted, single-line
/// rendering an entry gets in a list, built by the core and never by a shell.
/// Empty when the entry is Undecryptable.
///
/// `plaintext` is the whole decrypted text, `None` only when the entry is
/// Undecryptable. A phone has no reader pane and no search, so nothing on
/// Android reads it; it is carried because this record is the core's `Entry`,
/// whole, and a boundary that dropped the field would make an iOS reader
/// impossible to write. Same warning as [`ShortCode`]: it is whatever the
/// person copied, and UniFFI records always get a `toString` — do not log one.
///
/// `undecryptable` is the one field a shell must never re-derive. Nothing on
/// the wire flags it, and an entry whose plaintext is genuinely empty is
/// indistinguishable from one that will not decrypt to anybody guessing from an
/// empty `preview`.
///
/// `origin_label` is the Device Label, or a short slice of the Device id when
/// the mirror has none. Resolved by the core because the alternative was the
/// same three lines in Kotlin and in TypeScript.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Entry {
    pub id: i64,
    pub user_id: String,
    pub preview: String,
    pub plaintext: Option<String>,
    pub created_at: i64,
    pub device_id: String,
    pub device_label: Option<String>,
    pub origin_label: String,
    pub undecryptable: bool,
}

impl From<CoreEntry> for Entry {
    fn from(e: CoreEntry) -> Self {
        Entry {
            id: e.id,
            user_id: e.user_id,
            preview: e.preview,
            plaintext: e.plaintext,
            created_at: e.created_at,
            device_id: e.device_id,
            device_label: e.device_label,
            origin_label: e.origin_label,
            undecryptable: e.undecryptable,
        }
    }
}

/// One pairing as a list shows it. `status` and `pending` are live readings,
/// not stored columns.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PairingSummary {
    pub user_id: String,
    pub device_id: String,
    pub label: String,
    /// The User's name on the relay. `None` until this device has reached a
    /// relay that serves `GET /me`.
    pub username: Option<String>,
    pub server_url: String,
    /// The same relay as a person reads it: host and port, no scheme, no path
    /// and no credentials. Resolved by the core, which is the only place that
    /// parses a relay address.
    pub relay_host: String,
    pub status: ConnectionState,
    pub pending: i64,
    pub is_active: bool,
}

impl From<CorePairingSummary> for PairingSummary {
    fn from(p: CorePairingSummary) -> Self {
        PairingSummary {
            user_id: p.user_id,
            device_id: p.device_id,
            label: p.label,
            username: p.username,
            server_url: p.server_url,
            relay_host: p.relay_host,
            status: p.status.into(),
            pending: p.pending,
            is_active: p.is_active,
        }
    }
}

/// The pairing a completed handshake produced.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PairedDevice {
    pub user_id: String,
    pub device_id: String,
}

impl From<CorePairedDevice> for PairedDevice {
    fn from(p: CorePairedDevice) -> Self {
        PairedDevice { user_id: p.user_id, device_id: p.device_id }
    }
}

/// A short code, grouped for someone to read aloud, and when it stops working.
///
/// The core's equivalent deliberately has no `Debug`, because the code *is* the
/// pairing secret for the next two minutes. UniFFI records always get a
/// `toString`, so the same warning has to be carried by a comment instead: do
/// not log this, and do not put it in a crash report.
#[derive(Clone, uniffi::Record)]
pub struct ShortCode {
    pub code: String,
    pub expires_at: i64,
}

impl From<CoreShortCode> for ShortCode {
    fn from(c: CoreShortCode) -> Self {
        ShortCode { code: c.code, expires_at: c.expires_at }
    }
}

/// The last time anything at all arrived from the relay for one pairing.
///
/// `None` means this device has never heard from it, which is not contact at
/// the epoch.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Contact {
    pub user_id: String,
    pub last_contact_at: Option<i64>,
}

impl From<CoreContact> for Contact {
    fn from(c: CoreContact) -> Self {
        Contact { user_id: c.user_id, last_contact_at: c.last_contact_at }
    }
}

/// Whether a recall's answer was confirmed by the relay or read off a cache
/// that could not be brought current.
///
/// A shell must surface `Cache` visibly. It is the difference between "this is
/// the newest thing you copied" and "this is the newest thing this device knows
/// about", and a silent fallback hands over yesterday's link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RecallSource {
    Relay,
    Cache,
}

impl From<CoreRecallSource> for RecallSource {
    fn from(s: CoreRecallSource) -> Self {
        match s {
            CoreRecallSource::Relay => RecallSource::Relay,
            CoreRecallSource::Cache => RecallSource::Cache,
        }
    }
}

/// What `recall_latest` put on the clipboard, and where it came from.
///
/// Same warning as [`ShortCode`]: `text` is whatever the person copied. It has
/// no business in a log.
#[derive(Clone, uniffi::Record)]
pub struct Recalled {
    pub text: String,
    pub entry_id: i64,
    pub created_at: i64,
    pub source: RecallSource,
}

impl From<CoreRecalled> for Recalled {
    fn from(r: CoreRecalled) -> Self {
        Recalled {
            text: r.text,
            entry_id: r.entry_id,
            created_at: r.created_at,
            source: r.source.into(),
        }
    }
}

/// Why the capture filter refused text.
///
/// Only three of these are reachable through [`crate::Sharepaste::offer`],
/// which is the only way a phone hands text in: `NonText`, `TooLarge` and
/// `Duplicate`. The other four describe Watched Capture, which a phone never
/// does — `Disabled` because an Offer is honoured regardless of the setting,
/// `DenyList` and `SelfWrite` and `Transient` because there is no frontmost
/// application, no watcher and no pasteboard sniff on a phone. They are carried
/// anyway so the enum is the core's, whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SkipReason {
    Disabled,
    Transient,
    NonText,
    TooLarge,
    DenyList,
    SelfWrite,
    Duplicate,
}

impl From<CoreSkipReason> for SkipReason {
    fn from(s: CoreSkipReason) -> Self {
        match s {
            CoreSkipReason::Disabled => SkipReason::Disabled,
            CoreSkipReason::Transient => SkipReason::Transient,
            CoreSkipReason::NonText => SkipReason::NonText,
            CoreSkipReason::TooLarge => SkipReason::TooLarge,
            CoreSkipReason::DenyList => SkipReason::DenyList,
            CoreSkipReason::SelfWrite => SkipReason::SelfWrite,
            CoreSkipReason::Duplicate => SkipReason::Duplicate,
        }
    }
}

/// What became of text handed to [`crate::Sharepaste::offer`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum OfferOutcome {
    /// Queued for upload. `pending` is the depth of the queue afterwards.
    Queued { pending: i64 },
    Rejected { reason: SkipReason },
}

impl From<CoreOfferOutcome> for OfferOutcome {
    fn from(o: CoreOfferOutcome) -> Self {
        match o {
            CoreOfferOutcome::Queued { pending } => OfferOutcome::Queued { pending },
            CoreOfferOutcome::Rejected(reason) => {
                OfferOutcome::Rejected { reason: reason.into() }
            }
        }
    }
}

/// The stored settings row.
///
/// `autostart`, `hotkey` and `update_check_enabled` are desktop concerns and a
/// phone has nothing to do with them. They are carried rather than filtered
/// because this is one row shared by both shells and hiding three fields would
/// make a phone's `update_settings` silently clear them.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Settings {
    pub capture_enabled: bool,
    pub deny_list: Vec<String>,
    pub autostart: bool,
    pub hotkey: Option<String>,
    pub last_active_user_id: Option<String>,
    pub update_check_enabled: bool,
}

impl From<CoreSettings> for Settings {
    fn from(s: CoreSettings) -> Self {
        Settings {
            capture_enabled: s.capture_enabled,
            deny_list: s.deny_list,
            autostart: s.autostart,
            hotkey: s.hotkey,
            last_active_user_id: s.last_active_user_id,
            update_check_enabled: s.update_check_enabled,
        }
    }
}

/// What to do about the hotkey in a [`SettingsPatch`].
///
/// The core spells this `Option<Option<String>>`: the outer `None` means "not
/// in this patch", the inner means "clear it". Kotlin has no `String??`, so the
/// inner optional becomes this enum rather than collapsing into a nullable that
/// cannot say which of the two was meant.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum HotkeyPatch {
    Set { hotkey: String },
    Clear,
}

/// A change to the settings, one optional per field: `None` leaves what is
/// stored alone.
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct SettingsPatch {
    #[uniffi(default = None)]
    pub capture_enabled: Option<bool>,
    #[uniffi(default = None)]
    pub deny_list: Option<Vec<String>>,
    #[uniffi(default = None)]
    pub autostart: Option<bool>,
    #[uniffi(default = None)]
    pub hotkey: Option<HotkeyPatch>,
    #[uniffi(default = None)]
    pub update_check_enabled: Option<bool>,
}

impl From<SettingsPatch> for CoreSettingsPatch {
    fn from(p: SettingsPatch) -> Self {
        CoreSettingsPatch {
            capture_enabled: p.capture_enabled,
            deny_list: p.deny_list,
            autostart: p.autostart,
            hotkey: p.hotkey.map(|h| match h {
                HotkeyPatch::Set { hotkey } => Some(hotkey),
                HotkeyPatch::Clear => None,
            }),
            update_check_enabled: p.update_check_enabled,
        }
    }
}

/// Everything the core tells a shell about.
///
/// Delivered on the session loop's own tokio tasks, which is to say: not on the
/// caller's thread and not on the platform's main thread. A Kotlin
/// implementation must marshal onto the main dispatcher before touching UI
/// state.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum CoreEvent {
    PairingAdded { user_id: String, device_id: String, label: String },
    PairingRemoved { user_id: String },
    ActivePairingChanged { user_id: Option<String> },
    ConnectionState { user_id: String, state: ConnectionState, last_error: Option<String> },
    EntryAdded { user_id: String, entry: Entry },
    EntryDeleted { user_id: String, entry_id: i64 },
    HistoryChanged { user_id: String },
    PendingCount { user_id: String, count: i64 },
    Contact { user_id: String, last_contact_at: Option<i64> },
    PairShortcode { code: String, expires_at: i64 },
    PairClaimed { user_id: String, device_label: Option<String> },
    PairExpired,
}

impl From<CoreCoreEvent> for CoreEvent {
    fn from(e: CoreCoreEvent) -> Self {
        match e {
            CoreCoreEvent::PairingAdded { user_id, device_id, label } => {
                CoreEvent::PairingAdded { user_id, device_id, label }
            }
            CoreCoreEvent::PairingRemoved { user_id } => CoreEvent::PairingRemoved { user_id },
            CoreCoreEvent::ActivePairingChanged { user_id } => {
                CoreEvent::ActivePairingChanged { user_id }
            }
            CoreCoreEvent::ConnectionState { user_id, state, last_error } => {
                CoreEvent::ConnectionState { user_id, state: state.into(), last_error }
            }
            CoreCoreEvent::EntryAdded { user_id, entry } => {
                CoreEvent::EntryAdded { user_id, entry: entry.into() }
            }
            CoreCoreEvent::EntryDeleted { user_id, entry_id } => {
                CoreEvent::EntryDeleted { user_id, entry_id }
            }
            CoreCoreEvent::HistoryChanged { user_id } => CoreEvent::HistoryChanged { user_id },
            CoreCoreEvent::PendingCount { user_id, count } => {
                CoreEvent::PendingCount { user_id, count }
            }
            CoreCoreEvent::Contact { user_id, last_contact_at } => {
                CoreEvent::Contact { user_id, last_contact_at }
            }
            CoreCoreEvent::PairShortcode { code, expires_at } => {
                CoreEvent::PairShortcode { code, expires_at }
            }
            CoreCoreEvent::PairClaimed { user_id, device_label } => {
                CoreEvent::PairClaimed { user_id, device_label }
            }
            CoreCoreEvent::PairExpired => CoreEvent::PairExpired,
        }
    }
}
