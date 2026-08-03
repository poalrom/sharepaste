//! Fakes for the three platform seams.
//!
//! Shipped behind the `testing` feature as well as `cfg(test)` so the shells
//! can drive the core the same way its own tests do. They exist because the
//! session loop is only honestly testable when nothing real is attached: no
//! relay, no window system, no system keychain.
//!
//! [`InMemoryKeychain`](crate::keychain::InMemoryKeychain) is the third fake and
//! is not repeated here — it is a shipped implementation, not a test double.

use crate::errors::AppError;
use crate::event::CoreEvent;
use crate::http::dto::{EntryRow, MeResp};
use crate::platform::{Clipboard, EventSink};
use crate::pairing::payload::{PairClaim, PairTransport};
use crate::sync::session::SessionTransport;
use crate::sync::uploader::{Uploaded, Used};
use crate::sync::{sse, ConnectionState};
use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// An [`EventSink`] that keeps everything it was handed, in order.
#[derive(Default)]
pub struct RecordingSink {
    events: Mutex<Vec<CoreEvent>>,
}

impl EventSink for RecordingSink {
    fn emit(&self, event: CoreEvent) {
        self.events.lock().push(event);
    }
}

impl RecordingSink {
    pub fn events(&self) -> Vec<CoreEvent> {
        self.events.lock().clone()
    }

    /// Every connection-state transition, in order. The session's whole
    /// lifecycle reads off this one list.
    pub fn connection_states(&self) -> Vec<ConnectionState> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                CoreEvent::ConnectionState { state, .. } => Some(state),
                _ => None,
            })
            .collect()
    }

    pub fn entry_previews(&self) -> Vec<String> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                CoreEvent::EntryAdded { entry, .. } => Some(entry.preview),
                _ => None,
            })
            .collect()
    }

    /// Every entry reported, whole. `entry_previews` is the shorthand for the
    /// common case; this is what a test about the entry's *other* fields —
    /// `undecryptable`, the Origin label — reads.
    pub fn entries(&self) -> Vec<crate::event::Entry> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                CoreEvent::EntryAdded { entry, .. } => Some(entry),
                _ => None,
            })
            .collect()
    }

    /// Every short code revealed, in order.
    ///
    /// The only place a shell can learn a code, which is what makes this the
    /// vantage point for "was it revealed before the upload finished".
    pub fn shortcodes(&self) -> Vec<String> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                CoreEvent::PairShortcode { code, .. } => Some(code),
                _ => None,
            })
            .collect()
    }

    /// The Device Label of every claim reported, in order.
    pub fn pair_claimed(&self) -> Vec<Option<String>> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                CoreEvent::PairClaimed { device_label, .. } => Some(device_label),
                _ => None,
            })
            .collect()
    }

    pub fn pending_counts(&self) -> Vec<i64> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                CoreEvent::PendingCount { count, .. } => Some(count),
                _ => None,
            })
            .collect()
    }

    pub fn saw_history_changed(&self, user_id: &str) -> bool {
        self.events()
            .iter()
            .any(|e| matches!(e, CoreEvent::HistoryChanged { user_id: u } if u == user_id))
    }

    /// How many reorders were reported, for the tests that care that a repeat
    /// ingest which moved nothing announced nothing.
    pub fn history_changes(&self, user_id: &str) -> usize {
        self.events()
            .iter()
            .filter(|e| matches!(e, CoreEvent::HistoryChanged { user_id: u } if u == user_id))
            .count()
    }
}

type WriteHook = Box<dyn Fn(&str) -> Result<(), AppError> + Send + Sync>;

type PutPayloadHook = Box<dyn Fn() + Send + Sync>;

/// The pair id and relay address [`ScriptedRelay`] answers the pairing routes
/// with.
///
/// Fixed rather than random so a failing assertion names the same thing on every
/// run. The id has to parse as a UUID, because `start_pair` rejects one that
/// does not.
pub const SCRIPTED_PAIR_ID: &str = "11111111-2222-3333-4444-555555555555";
pub const SCRIPTED_RELAY_URL: &str = "https://scripted.invalid";

/// A pasteboard holding exactly what a test says it holds.
///
/// `types()` is answered without reading the text, which is the whole point of
/// [`PasteboardSniff`](crate::capture::filter::PasteboardSniff) being a trait: a
/// concealed type has to be refused before the plaintext behind it is pulled
/// into memory.
pub struct FakePasteboard {
    types: Vec<String>,
    text: Option<String>,
}

impl FakePasteboard {
    pub fn holding(types: &[&str], text: Option<&str>) -> Self {
        Self {
            types: types.iter().map(|t| (*t).to_string()).collect(),
            text: text.map(String::from),
        }
    }
}

impl crate::capture::filter::PasteboardSniff for FakePasteboard {
    fn types(&self) -> Vec<String> {
        self.types.clone()
    }

    fn read_text(&self) -> Option<String> {
        self.text.clone()
    }
}

/// A [`Clipboard`] that records what it was handed, and can be told what to do
/// about it.
///
/// The hook runs *inside* `write_text`, which is the only vantage point from
/// which a test can check the self-write marker at the instant that matters:
/// after the facade recorded it, before the write has returned.
#[derive(Default)]
pub struct FakeClipboard {
    writes: Mutex<Vec<String>>,
    hook: Mutex<Option<WriteHook>>,
}

impl FakeClipboard {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn on_write(&self, f: impl Fn(&str) -> Result<(), AppError> + Send + Sync + 'static) {
        *self.hook.lock() = Some(Box::new(f));
    }

    /// Every text handed to `write_text`, attempted writes included.
    pub fn writes(&self) -> Vec<String> {
        self.writes.lock().clone()
    }
}

impl Clipboard for FakeClipboard {
    fn read_text(&self) -> Result<Option<String>, AppError> {
        Ok(self.writes.lock().last().cloned())
    }

    fn write_text(&self, text: &str) -> Result<(), AppError> {
        self.writes.lock().push(text.to_string());
        let hook = self.hook.lock();
        match hook.as_ref() {
            Some(f) => f(text),
            None => Ok(()),
        }
    }
}

/// One scripted SSE connection.
pub enum Wire {
    /// Deliver these frames, then hold the stream open until the session is
    /// cancelled — a healthy connection.
    Holds(Vec<sse::ServerEvent>),
    /// Deliver these frames, then drop, so the loop must back off and
    /// reconnect.
    Drops(Vec<sse::ServerEvent>),
}

/// What a [`ScriptedRelay`] does when asked to record a **Use**.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum UseAnswer {
    /// Stamp it, the way a relay with the route and the entry does.
    #[default]
    Record,
    /// 404: the entry is gone, or this relay is older than the client and has
    /// no such route. The client must not queue either.
    Gone,
    /// The relay could not be reached at all, which is what queues a use.
    Unreachable,
}

/// What a [`ScriptedRelay`] does when asked to take a queued act.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum UploadAnswer {
    /// Take it and stamp it, the way a reachable relay does.
    #[default]
    Take,
    /// The relay could not be reached at all.
    Unreachable,
    /// The relay accepted the connection and never answered. The shape a
    /// *bounded* drain exists for: a refused port fails fast and would leave the
    /// bound untested.
    Stall,
}

/// A relay that answers from a script instead of a socket.
///
/// The seam that makes the session loop testable at all. An exhausted script
/// degrades the way a quiet relay does — an empty backfill, a 404 on `/me`, a
/// stream that stays open — so nothing here ends a session. The test cancels
/// it, exactly as a shell does.
#[derive(Default)]
pub struct ScriptedRelay {
    backfills: Mutex<VecDeque<Result<Vec<EntryRow>, AppError>>>,
    asked_since: Mutex<Vec<i64>>,
    me_results: Mutex<VecDeque<Result<MeResp, AppError>>>,
    me_calls: AtomicI64,
    wires: Mutex<VecDeque<Wire>>,
    closed: AtomicI64,
    uploaded: Mutex<Vec<String>>,
    uses: Mutex<Vec<i64>>,
    deleted: Mutex<Vec<i64>>,
    use_answer: Mutex<UseAnswer>,
    upload_answer: Mutex<UploadAnswer>,
    pair_payloads: Mutex<Vec<String>>,
    polls: Mutex<VecDeque<Result<PairClaim, AppError>>>,
    put_payload_hook: Mutex<Option<PutPayloadHook>>,
    base_url: Mutex<Option<String>>,
}

impl ScriptedRelay {
    pub fn new(backfills: Vec<Result<Vec<EntryRow>, AppError>>, wires: Vec<Wire>) -> Arc<Self> {
        Arc::new(Self {
            backfills: Mutex::new(backfills.into()),
            wires: Mutex::new(wires.into()),
            ..Self::default()
        })
    }

    /// Script the `/me` answers. Unscripted calls 404, which is what a relay
    /// older than this client does.
    pub fn answering_me(self: Arc<Self>, results: Vec<Result<MeResp, AppError>>) -> Arc<Self> {
        *self.me_results.lock() = results.into();
        self
    }

    /// The `since=` of every backfill, in order.
    pub fn asked_since(&self) -> Vec<i64> {
        self.asked_since.lock().clone()
    }

    pub fn me_calls(&self) -> i64 {
        self.me_calls.load(Ordering::Relaxed)
    }

    /// How many streams have ended — the only evidence, from outside, that a
    /// cancellation reached the session.
    pub fn streams_closed(&self) -> i64 {
        self.closed.load(Ordering::Relaxed)
    }

    pub fn uploaded(&self) -> Vec<String> {
        self.uploaded.lock().clone()
    }

    /// Every entry id a **Use** was recorded against, in order.
    pub fn uses(&self) -> Vec<i64> {
        self.uses.lock().clone()
    }

    /// Every entry id the relay was asked to delete, in order. What the
    /// withdrawal race is checked against: an act withdrawn while it was
    /// uploading has to be taken back off the relay.
    pub fn deleted(&self) -> Vec<i64> {
        self.deleted.lock().clone()
    }

    /// Change what this relay does with a use. The call is still recorded.
    pub fn answering_uses(self: Arc<Self>, answer: UseAnswer) -> Arc<Self> {
        *self.use_answer.lock() = answer;
        self
    }

    /// Change what this relay does with a queued act.
    pub fn answering_uploads(self: Arc<Self>, answer: UploadAnswer) -> Arc<Self> {
        *self.upload_answer.lock() = answer;
        self
    }

    /// Script the inviter's side of pairing: what each `/pair/poll` answers, in
    /// order.
    ///
    /// An exhausted script reads as expired rather than still waiting, so the
    /// facade's poll task ends instead of looping for the rest of the run.
    pub fn pairing(self: Arc<Self>, polls: Vec<Result<PairClaim, AppError>>) -> Arc<Self> {
        *self.polls.lock() = polls.into();
        self
    }

    /// Answer from `base_url` rather than [`SCRIPTED_RELAY_URL`] — what a test
    /// about the relay's *address* rather than its answers needs.
    pub fn at_url(self: Arc<Self>, base_url: &str) -> Arc<Self> {
        *self.base_url.lock() = Some(base_url.to_string());
        self
    }

    /// Runs *inside* `put_payload`, which is the only vantage point from which a
    /// test can see whether the short code was revealed before the upload had
    /// finished — the same trick [`FakeClipboard::on_write`] plays on the
    /// self-write marker.
    pub fn on_put_payload(&self, f: impl Fn() + Send + Sync + 'static) {
        *self.put_payload_hook.lock() = Some(Box::new(f));
    }

    /// Every payload handed to `put_payload`, in order.
    pub fn pair_payloads(&self) -> Vec<String> {
        self.pair_payloads.lock().clone()
    }
}

#[async_trait]
impl SessionTransport for ScriptedRelay {
    async fn list_entries(&self, since: i64, _limit: u32) -> Result<Vec<EntryRow>, AppError> {
        self.asked_since.lock().push(since);
        self.backfills.lock().pop_front().unwrap_or_else(|| Ok(Vec::new()))
    }

    async fn me(&self) -> Result<MeResp, AppError> {
        self.me_calls.fetch_add(1, Ordering::Relaxed);
        self.me_results
            .lock()
            .pop_front()
            .unwrap_or_else(|| Err(AppError::NotFound("no /me on this relay".into())))
    }

    async fn stream(
        &self,
        sink: mpsc::Sender<sse::ServerEvent>,
        cancel: CancellationToken,
        contact: Arc<AtomicI64>,
    ) -> Result<(), AppError> {
        // The real tap sits below the SSE parser, so every byte counts —
        // including the `: connected` preamble, which is what this stands in
        // for. A dispatched frame is not required for Contact to move.
        contact.store(crate::now_ms(), Ordering::Relaxed);
        let (frames, holds) = match self.wires.lock().pop_front() {
            Some(Wire::Holds(f)) => (f, true),
            Some(Wire::Drops(f)) => (f, false),
            None => (Vec::new(), true),
        };
        for frame in frames {
            if sink.send(frame).await.is_err() {
                break;
            }
        }
        if holds {
            cancel.cancelled().await;
        }
        self.closed.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Hands back an id that climbs with each upload, so a test can tell two of
    /// this device's own acts apart in the cache the flush reconciles.
    ///
    /// `seq` tracks the id, as the relay's own seeding does, and `last_use`
    /// equals `created_at`: a capture is the entry's first use.
    async fn upload(&self, ciphertext_b64: &str) -> Result<Uploaded, AppError> {
        let answer = *self.upload_answer.lock();
        match answer {
            UploadAnswer::Take => {}
            UploadAnswer::Unreachable => {
                return Err(AppError::Network("scripted outage".into()))
            }
            // Longer than any bound a caller could reasonably set, so what ends
            // the wait is the caller's own timeout and nothing else.
            UploadAnswer::Stall => {
                tokio::time::sleep(std::time::Duration::from_secs(600)).await;
            }
        }
        let mut uploaded = self.uploaded.lock();
        uploaded.push(ciphertext_b64.to_string());
        let id = uploaded.len() as i64;
        let at = crate::now_ms();
        Ok(Uploaded { id, created_at: at, seq: id, last_use: at })
    }

    async fn use_entry(&self, entry_id: i64) -> Result<Used, AppError> {
        self.uses.lock().push(entry_id);
        match *self.use_answer.lock() {
            UseAnswer::Record => Ok(Used { seq: 10_000 + entry_id, last_use: crate::now_ms() }),
            UseAnswer::Gone => Err(AppError::NotFound("entry not found".into())),
            UseAnswer::Unreachable => Err(AppError::Network("scripted outage".into())),
        }
    }

    async fn delete_entry(&self, entry_id: i64) -> Result<(), AppError> {
        self.deleted.lock().push(entry_id);
        Ok(())
    }
}

#[async_trait]
impl PairTransport for ScriptedRelay {
    async fn start(&self, _secret_hash: &str) -> Result<String, AppError> {
        Ok(SCRIPTED_PAIR_ID.to_string())
    }

    async fn put_payload(&self, _pair_id: &str, encrypted_payload: &str) -> Result<(), AppError> {
        self.pair_payloads.lock().push(encrypted_payload.to_string());
        if let Some(f) = self.put_payload_hook.lock().as_ref() {
            f();
        }
        Ok(())
    }

    async fn poll(&self, _pair_id: &str, _timeout_ms: u32) -> Result<PairClaim, AppError> {
        self.polls
            .lock()
            .pop_front()
            .unwrap_or_else(|| Ok(PairClaim::Expired))
    }

    fn base_url(&self) -> String {
        self.base_url
            .lock()
            .clone()
            .unwrap_or_else(|| SCRIPTED_RELAY_URL.to_string())
    }
}

/// The user key every fixture below encrypts with, and the hex a fake keychain
/// hands back for it.
pub const TEST_USER_KEY_HEX: &str = "0707070707070707070707070707070707070707070707070707070707070707";

pub fn test_user_key() -> crate::crypto::UserKey {
    zeroize::Zeroizing::new([7u8; 32])
}

fn ciphertext_of(user_id: &str, plaintext: &str) -> String {
    let ct = crate::crypto::encrypt(&test_user_key(), user_id, plaintext.as_bytes()).unwrap();
    crate::pairing::payload::base64_encode(&ct)
}

/// A backfill row [`test_user_key`] can actually decrypt.
///
/// Stamped from the clock, not from a small constant: `entries_cache` prunes
/// anything older than its 30-day window on every insert, so a fixture dated
/// near the epoch is deleted by the very write that stores it and no test could
/// then read the row back.
pub fn encrypted_row(id: i64, user_id: &str, plaintext: &str, device_id: &str) -> EntryRow {
    let at = crate::now_ms() + id;
    EntryRow {
        id,
        ciphertext: ciphertext_of(user_id, plaintext),
        created_at: at,
        device_id: device_id.into(),
        seq: id,
        last_use: at,
    }
}

/// A row `decryptor::ingest` cannot even unwrap: the ciphertext is not base64,
/// so ingest fails outright rather than storing an undecryptable row. This is
/// the shape that must not move the last-seen watermark.
pub fn unstorable_row(id: i64) -> EntryRow {
    let at = crate::now_ms() + id;
    EntryRow {
        id,
        ciphertext: "!!! not base64 !!!".into(),
        created_at: at,
        device_id: "d".into(),
        seq: id,
        last_use: at,
    }
}

/// The same row, as a live SSE frame.
pub fn live_entry(id: i64, user_id: &str, plaintext: &str, device_id: &str) -> sse::ServerEvent {
    let at = crate::now_ms() + id;
    sse::ServerEvent::Entry {
        id,
        ciphertext: ciphertext_of(user_id, plaintext),
        created_at: at,
        device_id: device_id.into(),
        seq: id,
        last_use: at,
    }
}

/// A **Use** of an entry already cached, as the relay republishes it: the same
/// row, with a later `last_use` and a fresh sequence.
pub fn live_use(row: &EntryRow, seq: i64, last_use: i64) -> sse::ServerEvent {
    sse::ServerEvent::Entry {
        id: row.id,
        ciphertext: row.ciphertext.clone(),
        created_at: row.created_at,
        device_id: row.device_id.clone(),
        seq,
        last_use,
    }
}
