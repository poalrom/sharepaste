//! Everything the core tells a shell about.
//!
//! One variant per event a shell consumes. Two of the desktop's events are
//! *not* here and must not be added: `main://navigate` routes a window and
//! `update-available` describes a Release, and neither is protocol — they stay
//! in the desktop's own `events.rs`.
//!
//! A shell's [`EventSink`](crate::platform::EventSink) maps each variant onto
//! whatever its host expects. The desktop's maps them onto the event names and
//! JSON shapes its frontend already listens for, byte for byte.

use crate::render;
use crate::sync::ConnectionState;
use serde::Serialize;

/// One entry as a shell renders it in a list.
///
/// `preview` and `plaintext` are two different things and each of them means
/// exactly one thing on every path:
///
///  * `preview` is the **Preview** — the decrypted, single-line rendering of an
///    entry as shown in a list, built by [`render::preview`] and never by a
///    shell. Empty when the entry is Undecryptable.
///  * `plaintext` is the whole decrypted text, unbounded and unaltered, for the
///    reader pane ADR 0003 describes, for a search that has to match a word on
///    an entry's third line, and for the byte count that explains a monstrous
///    one. `None`, and only, when the entry is Undecryptable.
///
/// They were one field once, carrying the Preview on [`CoreEvent::EntryAdded`]
/// and the whole plaintext out of the history query. Both shells guessed which,
/// and differently.
///
/// `undecryptable` is explicit, and is the one thing a shell must not re-derive.
/// Nothing on the wire flags it; the only fact that means it is a NULL cached
/// plaintext, and an entry whose plaintext is genuinely empty is
/// indistinguishable from one that will not decrypt to anybody guessing from an
/// empty `preview`.
///
/// `origin_label` is the **Origin** as a row names it, resolved here for the
/// same reason: it was written twice, in two languages, character for
/// character.
///
/// `Serialize` for the same reason [`ConnectionState`] and
/// [`Settings`](crate::storage::settings::Settings) carry it: the desktop hands
/// this straight to its webview, and a shell-side copy of the struct would be one
/// more place for a field to go missing.
///
/// No `Debug`, for the reason [`ShortCode`](crate::facade::ShortCode) and
/// [`Recalled`](crate::facade::Recalled) have none: `plaintext` is whatever the
/// person copied, and a struct that formats itself is one `tracing::debug!` away
/// from putting it in a log file. A shell that genuinely needs a rendering should
/// write a `Debug` that redacts the payload rather than reach for the derive.
#[derive(Clone, Serialize)]
pub struct Entry {
    pub id: i64,
    pub user_id: String,
    pub preview: String,
    /// The whole decrypted text. Whatever the person copied — do not log it.
    pub plaintext: Option<String>,
    pub created_at: i64,
    /// The moment of this entry's most recent **Use** — the fact the History is
    /// ordered by, and the one a list row's age column reads. Equal to
    /// `created_at` for an entry never used since capture.
    pub last_use: i64,
    pub device_id: String,
    pub device_label: Option<String>,
    pub origin_label: String,
    pub undecryptable: bool,
}

impl Entry {
    /// The only constructor, so every field derived from another cannot
    /// disagree with what it was derived from.
    ///
    /// Two ingest paths and one cache query build every `Entry` there is, and
    /// while each of them filled the struct itself, `preview` came to mean the
    /// Preview on one path and the whole plaintext on another. No test caught
    /// it because both were self-consistent. `preview` and `undecryptable` now
    /// follow from `plaintext`, and `origin_label` from the two Origin fields,
    /// in one place.
    pub(crate) fn new(
        id: i64,
        user_id: String,
        plaintext: Option<String>,
        created_at: i64,
        last_use: i64,
        device_id: String,
        device_label: Option<String>,
    ) -> Self {
        Entry {
            preview: plaintext.as_deref().map(render::preview).unwrap_or_default(),
            origin_label: render::origin_label(device_label.as_deref(), &device_id),
            // A missing plaintext is the one fact that means Undecryptable.
            undecryptable: plaintext.is_none(),
            id,
            user_id,
            plaintext,
            created_at,
            last_use,
            device_id,
            device_label,
        }
    }
}

/// Everything a shell reacts to.
///
/// There is deliberately no decryption-failure variant. An entry that will not
/// decrypt arrives on `EntryAdded` already carrying `undecryptable`, so its own
/// row says so; a second event naming the same id told no shell anything it did
/// not have, fired once per row on a backfill, and both shells ended up
/// discarding it.
///
/// No `Debug`, for the reason [`Entry`] has none. `EntryAdded` embeds one, and
/// `PairShortcode` carries the pairing secret itself for the next two minutes —
/// which is exactly what [`ShortCode`](crate::facade::ShortCode) refuses to make
/// printable. A sink is the one place a shell is most likely to log the thing it
/// was handed.
#[derive(Clone)]
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
