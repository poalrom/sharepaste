//! The platform half of Watched Capture.
//!
//! The filter itself lives in `sharepaste_core::capture::filter`; what is left
//! here is the part that cannot: two pasteboard sniffers written against
//! `objc2` and `windows-sys`, and the clipboard-change watcher. They reach the
//! core by implementing `sharepaste_core::capture::filter::PasteboardSniff`.

#[cfg(target_os = "macos")]
pub(crate) mod macos;

#[cfg(target_os = "windows")]
pub(crate) mod windows;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) mod watcher;
