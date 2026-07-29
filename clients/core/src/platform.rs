//! The three things a shell must supply, and nothing else.
//!
//! Everything else the protocol needs it either owns (the database, the relay
//! client, the crypto) or is handed as data (paths). These three are the only
//! places where "what the host can do" leaks in: a secret store, the system
//! clipboard, and somewhere to put an event.
//!
//! All three are object-safe and all three are `Send + Sync`, because the
//! session loop holds them across `await` points on a background runtime.

use crate::errors::AppError;
use crate::event::CoreEvent;

/// The platform secret store.
///
/// Re-exported here rather than redefined: it predates the other two and
/// already ships with a system implementation on macOS and Windows and
/// [`InMemoryKeychain`](crate::keychain::InMemoryKeychain) everywhere else. A
/// foreign shell supplies its own — Android's `EncryptedSharedPreferences`,
/// iOS's Keychain Services — by implementing this one trait.
pub use crate::keychain::Keychain;

/// The system clipboard, as the core needs it.
///
/// [`write_text`](Clipboard::write_text) is the **raw** write, and
/// `Sharepaste::write_clipboard` is its only caller. The self-write invariant
/// that stops Watched Capture re-capturing this app's own clipboard write is
/// stated, in full and in one place, on
/// [`Sharepaste::write_clipboard`](crate::facade::Sharepaste::write_clipboard);
/// a shell that reimplements it gets a clipboard write that immediately
/// re-uploads itself.
pub trait Clipboard: Send + Sync {
    fn read_text(&self) -> Result<Option<String>, AppError>;
    fn write_text(&self, text: &str) -> Result<(), AppError>;
}

/// Where the core puts everything a shell is expected to react to.
///
/// One method, one enum, one direction. `emit` is infallible by design — the
/// session loop has nothing useful to do about a shell that cannot receive an
/// event, and the old `let _ = app.emit(..)` at every call site said the same
/// thing less honestly.
///
/// **No lock the core owns is held when `emit` runs, and the one that matters is
/// the database.** `Sharepaste`'s `Connection` sits behind a
/// `tokio::sync::Mutex`, and a sink is foreign code: a Kotlin implementation
/// that marshals onto the main dispatcher and re-enters the facade from there
/// would deadlock on that mutex, and it would look like a hung app rather than
/// like a lock-ordering bug. So every path that writes and then announces scopes
/// its guard shut first, and a helper that runs *under* a guard returns what to
/// announce instead of announcing it — see `Uploader::cache_own_entry`. An
/// implementation may therefore call straight back into the facade.
///
/// It should still not block for long. Emits arrive on the session's own tasks,
/// so a sink that sleeps is a stream that has stopped reading.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: CoreEvent);
}
