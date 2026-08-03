//! The data a foreign shell sees, and the conversions from the core's own.
//!
//! Each type here restates a core type. The duplication buys one thing: the
//! core stays free to hold `Instant`s, `PathBuf`s and doubly-optional patches,
//! while the boundary carries only what a foreign binding can spell. Every
//! conversion is `From`, one direction, and every one of them takes its source
//! apart by name — records destructure, enums `match` — so a field added to a
//! core type is a compile error here rather than a field a phone silently never
//! sees. Projecting fields off the source (`id: e.id`) would compile through
//! exactly the omission this module exists to catch: never do that here.

use sharepaste_core::capture::filter::SkipReason as CoreSkipReason;
use sharepaste_core::event::{CoreEvent as CoreCoreEvent, Entry as CoreEntry};
use sharepaste_core::facade::{
    Contact as CoreContact, HistoryCursor as CoreHistoryCursor,
    OfferOutcome as CoreOfferOutcome, PairedDevice as CorePairedDevice,
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
/// Undecryptable. A phone still has no reader pane, but the Filter matches the
/// whole `plaintext` rather than the Preview, so Android does read it; the
/// field crosses the boundary for that, and not only because this record is
/// the core's `Entry`, whole, and dropping it would make an iOS reader
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
///
/// No `Debug`, for the reason `sharepaste_core::event::Entry` has none and
/// `ShortCode` and `Recalled` below have none: `plaintext` is whatever the
/// person copied, and a struct that formats itself is one `tracing::debug!`
/// away from putting it in a log file.
///
/// That closes the Rust half only. UniFFI generates a Kotlin `data class`,
/// whose `toString` prints every field regardless of what this side derives, so
/// the phone's half of the rule is that nothing logs an event object.
/// `FlowEventSink` holds that line by logging `event::class.java.simpleName`
/// rather than the event — do not "improve" it to `$event`.
#[derive(Clone, uniffi::Record)]
pub struct Entry {
    pub id: i64,
    pub user_id: String,
    pub preview: String,
    pub plaintext: Option<String>,
    pub created_at: i64,
    /// The moment of this entry's most recent **Use** — what the History is
    /// ordered by, and what a row's age should read. Equal to `created_at` for
    /// an entry never used since capture.
    pub last_use: i64,
    pub device_id: String,
    pub device_label: Option<String>,
    pub origin_label: String,
    pub undecryptable: bool,
    /// An act against this entry has not reached the relay.
    ///
    /// True of a refusal too: the act is still owed. What a row draws is that the
    /// relay has not heard the latest word about it, and `refused_reason` is what
    /// says the waiting has stopped.
    pub pending: bool,
    /// Why the relay turned the act down, when it did.
    pub refused_reason: Option<String>,
}

impl From<CoreEntry> for Entry {
    fn from(e: CoreEntry) -> Self {
        let CoreEntry {
            id,
            user_id,
            preview,
            plaintext,
            created_at,
            last_use,
            device_id,
            device_label,
            origin_label,
            undecryptable,
            pending,
            refused_reason,
        } = e;
        Entry {
            id,
            user_id,
            preview,
            plaintext,
            created_at,
            last_use,
            device_id,
            device_label,
            origin_label,
            undecryptable,
            pending,
            refused_reason,
        }
    }
}

/// Where a page of the History resumes from: the `(rank, ord, id)` of the last
/// row of the page before it.
///
/// Three parts and not an id, because the History is two regions in one order
/// (ADR 0014) and the whole of that order is `(rank ASC, ord DESC, id DESC)`.
/// Keyset paging over the same tuple is what makes crossing the seam between the
/// pending region and the settled one no different from any other page boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct HistoryCursor {
    /// Which region: 0 refused, 1 pending, 2 settled.
    pub rank: i64,
    /// The place inside that region.
    pub ord: i64,
    /// This device's own id for the row, which keeps the order total.
    pub id: i64,
}

impl From<HistoryCursor> for CoreHistoryCursor {
    fn from(c: HistoryCursor) -> Self {
        let HistoryCursor { rank, ord, id } = c;
        CoreHistoryCursor { rank, ord, id }
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
        let CorePairingSummary {
            user_id,
            device_id,
            label,
            username,
            server_url,
            relay_host,
            status,
            pending,
            is_active,
        } = p;
        PairingSummary {
            user_id,
            device_id,
            label,
            username,
            server_url,
            relay_host,
            status: status.into(),
            pending,
            is_active,
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
        let CorePairedDevice { user_id, device_id } = p;
        PairedDevice { user_id, device_id }
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
        let CoreShortCode { code, expires_at } = c;
        ShortCode { code, expires_at }
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
        let CoreContact { user_id, last_contact_at } = c;
        Contact { user_id, last_contact_at }
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
        let CoreRecalled { text, entry_id, created_at, source } = r;
        Recalled { text, entry_id, created_at, source: source.into() }
    }
}

/// Why the capture filter refused text.
///
/// Only two of these are reachable through [`crate::Sharepaste::offer`], which
/// is the only way a phone hands text in: `NonText` and `TooLarge`. The other
/// four describe Watched Capture, which a phone never does — `Disabled`
/// because an Offer is honoured regardless of the setting, `DenyList` and
/// `SelfWrite` and `Transient` because there is no frontmost application, no
/// watcher and no pasteboard sniff on a phone. They are carried anyway so the
/// enum is the core's, whole.
///
/// There is no `Duplicate`. A repeat copy is not a refusal: it is a **Use** of
/// the entry this device already holds, and it arrives as
/// [`OfferOutcome::Recognised`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SkipReason {
    Disabled,
    Transient,
    NonText,
    TooLarge,
    DenyList,
    SelfWrite,
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
        }
    }
}

/// What became of text handed to [`crate::Sharepaste::offer`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum OfferOutcome {
    /// Queued for upload. `pending` is the depth of the queue afterwards.
    Queued { pending: i64 },
    /// This phone already held the same text, so nothing was captured: the
    /// entry it matched is now the head of the History. `pending` is the depth
    /// of the queue afterwards, exactly as on `Queued` — recognition queues no
    /// capture, but the **Use** it records queues when the relay is out of
    /// reach.
    ///
    /// A separate case, and a separate **Receipt**, because reporting it as
    /// `Queued` would claim content was saved when nothing was — on a list the
    /// person can immediately check.
    Recognised { pending: i64 },
    Rejected { reason: SkipReason },
}

impl From<CoreOfferOutcome> for OfferOutcome {
    fn from(o: CoreOfferOutcome) -> Self {
        match o {
            CoreOfferOutcome::Queued { pending } => OfferOutcome::Queued { pending },
            CoreOfferOutcome::Recognised { pending } => OfferOutcome::Recognised { pending },
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
        let CoreSettings {
            capture_enabled,
            deny_list,
            autostart,
            hotkey,
            last_active_user_id,
            update_check_enabled,
        } = s;
        Settings {
            capture_enabled,
            deny_list,
            autostart,
            hotkey,
            last_active_user_id,
            update_check_enabled,
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
        // This one runs the other way, so the destructure guards the near side:
        // a field added to the boundary's `SettingsPatch` and not forwarded is a
        // compile error rather than a setting a phone can never change.
        let SettingsPatch {
            capture_enabled,
            deny_list,
            autostart,
            hotkey,
            update_check_enabled,
        } = p;
        CoreSettingsPatch {
            capture_enabled,
            deny_list,
            autostart,
            hotkey: hotkey.map(|h| match h {
                HotkeyPatch::Set { hotkey } => Some(hotkey),
                HotkeyPatch::Clear => None,
            }),
            update_check_enabled,
        }
    }
}

/// Everything the core tells a shell about.
///
/// Delivered on the session loop's own tokio tasks, which is to say: not on the
/// caller's thread and not on the platform's main thread. A Kotlin
/// implementation must marshal onto the main dispatcher before touching UI
/// state.
///
/// No `Debug`: `EntryAdded` embeds an [`Entry`] and `PairShortcode` carries the
/// same short code the core refuses to make printable — it *is* the pairing
/// secret for the next two minutes.
#[derive(Clone, uniffi::Enum)]
pub enum CoreEvent {
    PairingAdded { user_id: String, device_id: String, label: String },
    PairingRemoved { user_id: String },
    ActivePairingChanged { user_id: Option<String> },
    ConnectionState { user_id: String, state: ConnectionState, last_error: Option<String> },
    EntryAdded { user_id: String, entry: Entry },
    EntryDeleted { user_id: String, entry_id: i64 },
    /// One act reached the relay, so its row may have stopped waiting.
    ///
    /// The row is addressed by its own id, which a flush does not change, so a
    /// shell updates it in place. Deliberately not `HistoryChanged`: nothing
    /// reorders at a flush, and a refetch per acked act is the cost this
    /// distinction exists to avoid — which is also why the relay's numbers ride
    /// along, rather than the row keeping its "never stamped" reading until
    /// something else refetches.
    ///
    /// `null` means the Relay said nothing about that number: a **Use** does not
    /// restamp a creation, and a queued use of an Entry the Relay has since
    /// dropped stamps neither.
    EntrySettled {
        user_id: String,
        entry_id: i64,
        created_at: Option<i64>,
        last_use: Option<i64>,
    },
    /// The relay turned an act down for what it is, and said this.
    EntryRefused { user_id: String, entry_id: i64, reason: String },
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
            CoreCoreEvent::EntrySettled { user_id, entry_id, created_at, last_use } => {
                CoreEvent::EntrySettled { user_id, entry_id, created_at, last_use }
            }
            CoreCoreEvent::EntryRefused { user_id, entry_id, reason } => {
                CoreEvent::EntryRefused { user_id, entry_id, reason }
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
