//! The three things a shell supplies, crossing the other way.
//!
//! These are UniFFI *foreign* traits: the implementation lives in Kotlin (or
//! Swift), and the core calls it. Each one is paired with a bridge that adapts
//! the boundary's shape — owned `String`s, [`crate::AppError`] — onto the core's
//! own trait, which speaks `&str` and `sharepaste_core::errors::AppError`.
//!
//! # Threading
//!
//! [`Keychain`] and [`Clipboard`] are called on whatever thread made the FFI
//! call, which the shell has already guaranteed is not the main thread.
//!
//! [`EventSink`] is the awkward one. It is called from the session loop's own
//! tokio tasks — the SSE reader, the uploader, the pair poll — which are worker
//! threads of the facade's private runtime, attached to no foreign runtime at
//! all. Two consequences, and both are load-bearing:
//!
//! 1. The generated binding attaches the calling native thread to the JVM for
//!    the duration of each call, so an implementation is reachable from there
//!    without any registration on the Kotlin side. It is not free — JNA attaches
//!    and detaches per call — which is a reason to keep `emit` cheap, not a
//!    reason to avoid it.
//! 2. Nothing about that thread is a UI thread. An implementation that touches
//!    UI state must marshal onto the main dispatcher first.

use crate::error::AppError;
use crate::types::CoreEvent;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use sharepaste_core::errors::AppError as CoreError;
use sharepaste_core::event::CoreEvent as CoreCoreEvent;
use sharepaste_core::platform::{
    Clipboard as CoreClipboard, EventSink as CoreEventSink, Keychain as CoreKeychain,
};

/// The platform secret store: the user key and the device token live here.
///
/// On Android this is `EncryptedSharedPreferences` with its master key in the
/// Android Keystore. On the desktop the core has its own implementation and
/// this trait is never crossed.
#[uniffi::export(foreign)]
pub trait Keychain: Send + Sync {
    fn put(&self, account: String, secret: String) -> Result<(), AppError>;
    fn get(&self, account: String) -> Result<Option<String>, AppError>;
    fn delete(&self, account: String) -> Result<(), AppError>;
}

/// The system clipboard.
///
/// `write_text` is the **raw** write. The self-write invariant that stops
/// Watched Capture re-capturing this app's own clipboard write is stated on the
/// core's `Sharepaste::write_clipboard` and nowhere else; an implementation
/// that reimplements it gets a clipboard write that immediately re-uploads
/// itself.
///
/// `read_text` returning `None` is ordinary on Android: since Android 10 the
/// clipboard is readable only by the focused app or the default IME. That is
/// the platform rule ADR 0007 is built on, not a bug to work around.
#[uniffi::export(foreign)]
pub trait Clipboard: Send + Sync {
    fn read_text(&self) -> Result<Option<String>, AppError>;
    fn write_text(&self, text: String) -> Result<(), AppError>;
}

/// Where the core puts everything a shell is expected to react to.
///
/// Infallible by design: the session loop has nothing useful to do about a
/// shell that cannot receive an event. See the module docs for where this is
/// called from.
#[uniffi::export(foreign)]
pub trait EventSink: Send + Sync {
    fn emit(&self, event: CoreEvent);
}

pub(crate) struct KeychainBridge(pub Arc<dyn Keychain>);

impl CoreKeychain for KeychainBridge {
    fn put(&self, account: &str, secret: &str) -> Result<(), CoreError> {
        self.0.put(account.to_string(), secret.to_string()).map_err(Into::into)
    }

    fn get(&self, account: &str) -> Result<Option<String>, CoreError> {
        self.0.get(account.to_string()).map_err(Into::into)
    }

    fn delete(&self, account: &str) -> Result<(), CoreError> {
        self.0.delete(account.to_string()).map_err(Into::into)
    }
}

pub(crate) struct ClipboardBridge(pub Arc<dyn Clipboard>);

impl CoreClipboard for ClipboardBridge {
    fn read_text(&self) -> Result<Option<String>, CoreError> {
        self.0.read_text().map_err(Into::into)
    }

    fn write_text(&self, text: &str) -> Result<(), CoreError> {
        self.0.write_text(text.to_string()).map_err(Into::into)
    }
}

pub(crate) struct EventSinkBridge(pub Arc<dyn EventSink>);

impl CoreEventSink for EventSinkBridge {
    fn emit(&self, event: CoreCoreEvent) {
        // The core declares `emit` infallible, and the generated binding turns
        // an exception thrown by a foreign implementation into a panic. Left
        // alone that panic unwinds through the SSE reader or the uploader and
        // kills the task, so a shell with one bad event handler quietly loses
        // its session. Catching it here costs a landing pad and keeps the
        // failure where it belongs: in a log line about the shell.
        if catch_unwind(AssertUnwindSafe(|| self.0.emit(event.into()))).is_err() {
            tracing::error!("the shell's event sink panicked; the event was dropped");
        }
    }
}
