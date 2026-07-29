//! The one capture filter.
//!
//! The platform pasteboard sniffers stay in the desktop shell — they speak
//! `objc2` and `windows-sys` — and reach across by implementing
//! [`filter::PasteboardSniff`].

pub mod filter;
