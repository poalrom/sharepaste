//! The sharepaste facade, across a foreign-function boundary.
//!
//! This crate is a translation layer and nothing else: it owns no protocol
//! state, makes no decisions, and every operation it exposes forwards straight
//! to [`sharepaste_core::facade::Sharepaste`]. Anything that looks like policy
//! here is a bug — it belongs in the core, where all three shells get it.
//!
//! # Why the boundary is blocking
//!
//! The facade owns a private multi-thread tokio runtime. Every exported method
//! therefore `block_on`s *that* runtime and hands back a plain value, instead of
//! using UniFFI's async scaffolding. One runtime, no cancellation semantics to
//! get wrong across the boundary, and nothing for a foreign caller to poll.
//!
//! The consequence is a rule the shell must hold and this crate cannot enforce:
//! **no call into this crate may run on the platform's main thread.** On Android
//! that means one repository class wrapping every call in
//! `withContext(Dispatchers.IO)`.
//!
//! # Why the types are mirrored rather than re-exported
//!
//! Every record and enum below restates a core type. That is deliberate. The
//! core's shape answers to the protocol; this one answers to what a foreign
//! binding can carry and to what a phone is allowed to see — `SettingsPatch`'s
//! doubly-optional `hotkey` has no Kotlin spelling, `capture_watched` takes a
//! trait object no phone can supply. The conversions are all in one direction
//! and all in one place, so a core refactor lands here and stops.
//!
//! # Binding-language-agnostic
//!
//! Nothing here is Kotlin- or Android-specific. The Android build is the only
//! consumer today because it is the only one that can be run and therefore the
//! only one that cannot rot; adding Swift is a bindgen invocation, not a change
//! to this crate.

mod error;
mod platform;
mod sharepaste;
mod types;

#[cfg(feature = "testing")]
mod kat;

pub use error::AppError;
pub use platform::{Clipboard, EventSink, Keychain};
pub use sharepaste::Sharepaste;
pub use types::*;

#[cfg(feature = "testing")]
pub use kat::*;

uniffi::setup_scaffolding!();
