//! The one object a foreign shell holds.

use crate::error::AppError;
use crate::platform::{
    Clipboard, ClipboardBridge, EventSink, EventSinkBridge, Keychain, KeychainBridge,
};
use crate::types::{
    Contact, ConnectionState, Entry, OfferOutcome, PairedDevice, PairingSummary, Recalled,
    Settings, SettingsPatch, ShortCode,
};
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use sharepaste_core::facade::{Sharepaste as CoreSharepaste, SharepasteConfig};

/// The facade, wrapped so a foreign caller sees plain values instead of
/// futures.
///
/// Every `async` operation below is driven on the facade's **own** runtime —
/// the one it already owns for its session loops — rather than on a second one
/// stood up here. There is exactly one runtime in the process, the facade owns
/// it, and it does not hand out a handle: a caller holding one could spawn onto
/// it, and what runs there is the facade's business. So this type calls
/// `Sharepaste::block_on` and nothing more.
///
/// Blocking the calling thread is the whole design: nothing about cancellation
/// or polling has to be agreed across the boundary. The cost is a rule the
/// shell must hold — no call may arrive on the platform's main thread, and none
/// may arrive from inside the facade's own runtime, which would panic. On
/// Android both are satisfied by routing every call through one repository
/// class on `Dispatchers.IO`.
#[derive(uniffi::Object)]
pub struct Sharepaste {
    inner: Arc<CoreSharepaste>,
}

impl Sharepaste {
    fn wrap(inner: Arc<CoreSharepaste>) -> Arc<Self> {
        Arc::new(Sharepaste { inner })
    }

    fn block_on<F: Future>(&self, fut: F) -> F::Output {
        self.inner.block_on(fut)
    }
}

#[uniffi::export]
impl Sharepaste {
    /// Open the database at `db_path` and build the facade over it.
    ///
    /// `db_path` is data, deliberately: the core never asks the OS where an
    /// application's data lives. On Android the shell passes a file inside
    /// `filesDir`, which is app-private storage and therefore covered by the
    /// platform's file-based encryption.
    ///
    /// `require_https` is data for the same reason, applied to a policy instead
    /// of a path. Android's network security configuration does not reach Rust
    /// `reqwest`, so the shell's insistence on TLS has to travel as a value and
    /// be enforced inside the core, at every `ServerClient` it builds. **The
    /// shipped app passes `true`**; an instrumented test reaching a cleartext
    /// test relay is the only thing that may pass `false`.
    #[uniffi::constructor]
    pub fn open(
        db_path: String,
        keychain: Arc<dyn Keychain>,
        clipboard: Arc<dyn Clipboard>,
        events: Arc<dyn EventSink>,
        require_https: bool,
    ) -> Result<Arc<Self>, AppError> {
        let cfg = SharepasteConfig {
            db_path: PathBuf::from(db_path),
            keychain: Arc::new(KeychainBridge(keychain)),
            clipboard: Arc::new(ClipboardBridge(clipboard)),
            events: Arc::new(EventSinkBridge(events)),
            require_https,
        };
        Ok(Self::wrap(CoreSharepaste::open(cfg)?))
    }

    // -- pairings and sessions --------------------------------------------

    pub fn list_pairings(&self) -> Result<Vec<PairingSummary>, AppError> {
        let pairings = self.block_on(self.inner.list_pairings())?;
        Ok(pairings.into_iter().map(Into::into).collect())
    }

    /// Redeem an invite token against a relay and become one of its devices.
    ///
    /// A `server_url` with the `http` scheme is refused before a byte leaves the
    /// device when the shell opened this facade with `require_https`, and the
    /// refusal is an
    /// [`AppError::InsecureRelay`](crate::AppError::InsecureRelay) naming the
    /// reason rather than an opaque network error. Without that flag the core has
    /// no opinion, which is what keeps a desktop paired to a cleartext relay
    /// working.
    pub fn pair_with_invite(
        &self,
        server_url: String,
        token: String,
        device_label: String,
    ) -> Result<PairedDevice, AppError> {
        Ok(self
            .block_on(self.inner.pair_with_invite(&server_url, &token, &device_label))?
            .into())
    }

    /// Reveal a short code another device can join with.
    ///
    /// One operation, not three: it uploads the encrypted payload, *then*
    /// reveals the code, *then* starts polling. No shell may observe the code
    /// before the upload has completed, so there is nothing here for a shell to
    /// sequence. The claim arrives later as
    /// [`CoreEvent::PairClaimed`](crate::CoreEvent::PairClaimed) or
    /// [`CoreEvent::PairExpired`](crate::CoreEvent::PairExpired).
    pub fn pair_start(&self, user_id: String) -> Result<ShortCode, AppError> {
        Ok(self.block_on(self.inner.pair_start(&user_id))?.into())
    }

    pub fn pair_with_code(
        &self,
        code: String,
        device_label: String,
    ) -> Result<PairedDevice, AppError> {
        Ok(self.block_on(self.inner.pair_with_code(&code, &device_label))?.into())
    }

    /// Forget a pairing: its session, its key, its token, its cache and its row.
    pub fn forget_pairing(&self, user_id: String) -> Result<(), AppError> {
        Ok(self.block_on(self.inner.forget_pairing(&user_id))?)
    }

    pub fn set_active_pairing(&self, user_id: String) -> Result<(), AppError> {
        Ok(self.block_on(self.inner.set_active_pairing(&user_id))?)
    }

    pub fn active_pairing(&self) -> Option<String> {
        self.inner.active_pairing()
    }

    /// Pick up the active pairing again after a cold start or a resume.
    ///
    /// Half of the foreground-only sync model: `onStart` calls this and then
    /// [`Sharepaste::start_session`], `onStop` calls
    /// [`Sharepaste::stop_all_sessions`].
    pub fn resume_active_pairing(&self) -> Result<Option<String>, AppError> {
        Ok(self.block_on(self.inner.resume_active_pairing())?)
    }

    /// Start (or restart) the sync session for `user_id`.
    ///
    /// Returns once the two background tasks are on the runtime; it does not
    /// wait for the relay. Everything after that arrives through the event
    /// sink.
    pub fn start_session(&self, user_id: String) -> Result<(), AppError> {
        Ok(self.block_on(self.inner.start_session(&user_id))?)
    }

    pub fn stop_session(&self, user_id: String) {
        self.inner.stop_session(&user_id);
    }

    /// Stop every live session. This is what an Android `onStop` calls.
    pub fn stop_all_sessions(&self) {
        self.inner.stop_all_sessions();
    }

    /// The last connection state a session reported for `user_id`.
    ///
    /// `Disconnected` for a user with no session, which on a foreground-only
    /// phone is the nominal reading and not a fault.
    pub fn connection_state(&self, user_id: String) -> ConnectionState {
        self.inner.connection_state(&user_id).into()
    }

    // -- history and clipboard --------------------------------------------

    /// One page of cached entries, newest first. `before_id` pages backwards.
    pub fn list_history(
        &self,
        user_id: String,
        before_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<Entry>, AppError> {
        let entries = self.block_on(self.inner.list_history(&user_id, before_id, limit))?;
        Ok(entries.into_iter().map(Into::into).collect())
    }

    /// The full plaintext of one entry, with no clipboard involvement.
    ///
    /// `None` covers both "no such entry" and "this device cannot decrypt it";
    /// the entry's `undecryptable` flag is what tells them apart.
    pub fn read_entry(&self, user_id: String, entry_id: i64) -> Result<Option<String>, AppError> {
        Ok(self.block_on(self.inner.read_entry(&user_id, entry_id))?)
    }

    /// Put one entry back on the clipboard.
    pub fn recall(&self, user_id: String, entry_id: i64) -> Result<(), AppError> {
        Ok(self.block_on(self.inner.recall(&user_id, entry_id))?)
    }

    /// Fetch, then hand back the newest entry there is.
    ///
    /// Always performs the round trip. `source == Cache` means the fetch failed
    /// and the answer is only as new as this device already was — a shell must
    /// say so on screen rather than passing it off as current.
    pub fn recall_latest(&self, user_id: String) -> Result<Recalled, AppError> {
        Ok(self.block_on(self.inner.recall_latest(&user_id))?.into())
    }

    /// Hand text to the protocol: the phone's only way in.
    ///
    /// Honoured regardless of the capture setting — that setting governs the
    /// desktop's Watched Capture, and refusing content a person just handed
    /// over is indefensible. It can still be rejected as non-text, too large,
    /// or a duplicate of the last capture.
    pub fn offer(&self, user_id: String, text: String) -> Result<OfferOutcome, AppError> {
        Ok(self.block_on(self.inner.offer(&user_id, &text))?.into())
    }

    pub fn delete_entry(&self, user_id: String, entry_id: i64) -> Result<(), AppError> {
        Ok(self.block_on(self.inner.delete_entry(&user_id, entry_id))?)
    }

    pub fn clear_history(&self, user_id: String) -> Result<(), AppError> {
        Ok(self.block_on(self.inner.clear_history(&user_id))?)
    }

    pub fn get_contact(&self, user_id: String) -> Result<Contact, AppError> {
        Ok(self.block_on(self.inner.get_contact(&user_id))?.into())
    }

    pub fn get_settings(&self) -> Result<Settings, AppError> {
        Ok(self.block_on(self.inner.get_settings())?.into())
    }

    pub fn update_settings(&self, patch: SettingsPatch) -> Result<Settings, AppError> {
        Ok(self.block_on(self.inner.update_settings(patch.into()))?.into())
    }

    /// Write text to the clipboard through the facade.
    ///
    /// Not the same as calling [`Clipboard::write_text`] directly: the
    /// self-write marker is recorded before the write and cleared if the write
    /// fails, and that ordering lives here so no shell can get it wrong.
    pub fn write_clipboard(&self, text: String) -> Result<(), AppError> {
        Ok(self.inner.write_clipboard(&text)?)
    }
}

// `capture_watched` is deliberately not exported. It takes a `PasteboardSniff`,
// which has to read the pasteboard's *types* before its text so a concealed
// password is refused before its plaintext is in memory. There is no such thing
// on a phone, and no phone runs a clipboard watcher — ADR 0007. Exporting it
// would mean inventing a foreign trait nothing can implement.

/// Test-only entry points, behind the `testing` feature.
///
/// A separate impl block so the `cfg` is resolved before the export macro runs
/// — a `cfg` on a method *inside* an exported block is not stripped before the
/// macro sees it, and the scaffolding would be generated for a method that
/// does not exist.
#[cfg(feature = "testing")]
#[uniffi::export]
impl Sharepaste {
    /// A facade over an in-memory database. `db_path` is not consulted.
    ///
    /// `require_https` is a parameter here too rather than defaulted, so an
    /// instrumented test that needs the cleartext test relay has to say so at
    /// the call, and a test that means to exercise the policy can.
    #[uniffi::constructor]
    pub fn open_in_memory(
        keychain: Arc<dyn Keychain>,
        clipboard: Arc<dyn Clipboard>,
        events: Arc<dyn EventSink>,
        require_https: bool,
    ) -> Result<Arc<Self>, AppError> {
        let cfg = SharepasteConfig {
            db_path: PathBuf::new(),
            keychain: Arc::new(KeychainBridge(keychain)),
            clipboard: Arc::new(ClipboardBridge(clipboard)),
            events: Arc::new(EventSinkBridge(events)),
            require_https,
        };
        Ok(Self::wrap(CoreSharepaste::open_in_memory(cfg)?))
    }
}
