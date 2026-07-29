//! A live session and the pieces it drives.
//!
//! All of it is shell-free: `session` notifies through an
//! [`EventSink`](crate::platform::EventSink) and spawns on the facade's own
//! runtime, so the same loop serves the desktop and a phone.

pub mod decryptor;
pub mod session;
pub mod sse;
pub mod state;
pub mod uploader;

pub use state::{BackoffPlan, ConnectionState};
