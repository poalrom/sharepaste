//! The sharepaste protocol, with no shell attached.
//!
//! Everything here is portable: crypto, the relay client, the local database,
//! the sync machinery, pairing, the capture filter and the session loop. No
//! Tauri, no window system, no path derivation — a caller hands in the paths
//! and the three platform implementations it owns
//! ([`Keychain`](platform::Keychain), [`Clipboard`](platform::Clipboard),
//! [`EventSink`](platform::EventSink)) and drives the whole protocol through
//! [`Sharepaste`](facade::Sharepaste).

pub mod capture;
pub mod crypto;
pub mod errors;
pub mod event;
pub mod facade;
pub mod http;
pub mod keychain;
pub mod pairing;
pub mod platform;
pub mod render;
pub mod storage;
pub mod sync;

/// Fakes for the three platform seams, so a dependent's tests can drive the
/// core with nothing real attached. Behind the same feature as
/// [`storage::open_in_memory`].
#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// Wall-clock milliseconds since the Unix epoch.
///
/// The protocol stamps entries, device rows and Contact readings with this, so it
/// belongs to the core rather than to whichever shell happens to be asking. An
/// unreadable clock reads as 0 rather than panicking: a wrong timestamp is a
/// cosmetic defect, a crash in the sync loop is not.
pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
