#![cfg(target_os = "macos")]

//! macOS-specific pasteboard sniffer + frontmost-app helper backed by
//! `NSPasteboard` and `NSWorkspace` from objc2-app-kit 0.2.

use crate::core::capture::filter::PasteboardSniff;
use objc2::rc::Retained;
use objc2_app_kit::{NSPasteboard, NSWorkspace};
use objc2_foundation::NSString;

/// Real implementation of [`PasteboardSniff`] backed by the macOS general
/// pasteboard. Cheap to construct; holds no state — every call walks
/// AppKit fresh, so the caller is responsible for change-count gating.
pub struct NSPasteboardSniffer;

impl NSPasteboardSniffer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NSPasteboardSniffer {
    fn default() -> Self {
        Self::new()
    }
}

impl PasteboardSniff for NSPasteboardSniffer {
    fn types(&self) -> Vec<String> {
        // SAFETY: `generalPasteboard` is documented thread-safe; `types`
        // returns an autoreleased NSArray<NSString>. We immediately
        // materialize Rust strings, so no AppKit object outlives the call.
        unsafe {
            let pb: Retained<NSPasteboard> = NSPasteboard::generalPasteboard();
            let Some(types) = pb.types() else {
                return Vec::new();
            };
            types.iter().map(|t| t.to_string()).collect()
        }
    }

    fn read_text(&self) -> Option<String> {
        // SAFETY: same as above — borrow the autoreleased NSString long
        // enough to copy it into an owned `String`.
        unsafe {
            let pb: Retained<NSPasteboard> = NSPasteboard::generalPasteboard();
            let key = NSString::from_str("public.utf8-plain-text");
            let s = pb.stringForType(&key)?;
            Some(s.to_string())
        }
    }
}

/// Returns the bundle identifier of the application currently in the
/// foreground, e.g. `"com.apple.Safari"`. Returns `None` when no app is
/// frontmost (rare) or the frontmost process has no bundle id (some
/// background helpers / unsigned binaries).
pub fn frontmost_bundle_id() -> Option<String> {
    // SAFETY: `sharedWorkspace` returns a singleton; `frontmostApplication`
    // is safe to call from any thread per Apple docs. We copy the bundle
    // id out before returning, so no autoreleased object escapes.
    unsafe {
        let ws: Retained<NSWorkspace> = NSWorkspace::sharedWorkspace();
        let app = ws.frontmostApplication()?;
        let bundle = app.bundleIdentifier()?;
        Some(bundle.to_string())
    }
}

#[cfg(test)]
mod tests {
    //! These are live AppKit smoke tests. They are marked `#[ignore]` so
    //! the default `cargo test` run skips them — touching `NSPasteboard`
    //! from multiple cargo-test threads concurrently can SIGSEGV on
    //! macOS, and there is no useful assertion we can make about the
    //! current user's clipboard anyway. Run them manually with:
    //!
    //!     cargo test --lib core::capture::macos -- --ignored --test-threads=1
    //!
    //! The point is to confirm the objc2 bindings + feature flags still
    //! compile and the FFI call path doesn't panic on a developer box.

    use super::*;
    use crate::core::capture::filter::PasteboardSniff;

    #[test]
    #[ignore = "live AppKit call; run with --ignored --test-threads=1"]
    fn types_call_does_not_panic() {
        let sniff = NSPasteboardSniffer::new();
        let _types = sniff.types();
    }

    #[test]
    #[ignore = "live AppKit call; run with --ignored --test-threads=1"]
    fn read_text_call_does_not_panic() {
        let sniff = NSPasteboardSniffer::new();
        let _maybe = sniff.read_text();
    }

    #[test]
    #[ignore = "live AppKit call; run with --ignored --test-threads=1"]
    fn frontmost_bundle_id_call_does_not_panic() {
        let _bundle = frontmost_bundle_id();
    }
}
