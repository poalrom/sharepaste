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

/// Find our app's status-bar item rect by walking the CoreGraphics
/// window list, filtering for windows owned by our PID at the status-bar
/// layer (25). CG bounds are already in top-left logical coordinates,
/// matching the shape Tauri's `TrayIconEvent::Click` provides.
///
/// We use CG instead of `NSApp.windows()` / `NSScreen.screens()` because
/// `objc2-foundation` 0.2.2 ships NSArray bindings that declare the wrong
/// type encoding for `NSUInteger` (`'q'` signed long) versus the current
/// macOS SDK (`'Q'` unsigned long), which makes objc2 0.6's strict
/// encoding check abort the process at the first `count` /
/// `countByEnumeratingWithState:` call. CoreGraphics' window-list API is
/// pure C and sidesteps that.
pub fn find_tray_rect() -> Option<tauri::Rect> {
    if let Some(r) = find_tray_rect_via_appkit() {
        return Some(r);
    }
    find_tray_rect_via_cgwindow()
}

/// Walk NSApplication.windows looking for the runtime class
/// `NSStatusBarWindow` and return its frame. We bypass the
/// objc2-foundation 0.2.2 typed NSArray bindings (their declared
/// `NSUInteger` encoding panics against the current macOS SDK) by sending
/// `count` / `objectAtIndex:` through raw `msg_send!` with `usize` return
/// types, which match the actual runtime encoding.
fn find_tray_rect_via_appkit() -> Option<tauri::Rect> {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_app_kit::NSScreen;
    use objc2_foundation::MainThreadMarker;
    use tauri::{LogicalPosition, LogicalSize};

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct CGSize {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }
    unsafe impl objc2::Encode for CGPoint {
        const ENCODING: objc2::Encoding = objc2::Encoding::Struct(
            "CGPoint",
            &[objc2::Encoding::Double, objc2::Encoding::Double],
        );
    }
    unsafe impl objc2::Encode for CGSize {
        const ENCODING: objc2::Encoding = objc2::Encoding::Struct(
            "CGSize",
            &[objc2::Encoding::Double, objc2::Encoding::Double],
        );
    }
    unsafe impl objc2::Encode for CGRect {
        const ENCODING: objc2::Encoding = objc2::Encoding::Struct(
            "CGRect",
            &[CGPoint::ENCODING, CGSize::ENCODING],
        );
    }

    let mtm = MainThreadMarker::new()?;
    let screen = NSScreen::mainScreen(mtm)?;
    let primary_h = screen.frame().size.height;

    unsafe {
        let app_class = AnyClass::get("NSApplication")?;
        let app: *mut AnyObject = msg_send![app_class, sharedApplication];
        if app.is_null() {
            return None;
        }
        let windows: *mut AnyObject = msg_send![app, windows];
        if windows.is_null() {
            return None;
        }
        let count: usize = msg_send![windows, count];
        for i in 0..count {
            let w: *mut AnyObject = msg_send![windows, objectAtIndex: i];
            if w.is_null() {
                continue;
            }
            let cls: *const AnyClass = msg_send![w, class];
            if cls.is_null() {
                continue;
            }
            let class_name = (*cls).name();
            tracing::info!(name = class_name, "appkit window class");
            if class_name != "NSStatusBarWindow" {
                continue;
            }
            // SAFETY: NSWindow.frame returns CGRect (NSRect on macOS 64-bit).
            let frame: CGRect = msg_send![w, frame];
            let logical_x = frame.origin.x;
            // Convert from AppKit's bottom-left origin to Tauri's
            // top-left origin using primary screen height.
            let logical_y = primary_h - (frame.origin.y + frame.size.height);
            let pos = LogicalPosition::<f64>::new(logical_x, logical_y);
            let size = LogicalSize::<f64>::new(frame.size.width, frame.size.height);
            tracing::info!(
                logical_x, logical_y, w = frame.size.width, h = frame.size.height,
                "find_tray_rect_via_appkit: matched NSStatusBarWindow"
            );
            return Some(tauri::Rect {
                position: pos.into(),
                size: size.into(),
            });
        }
    }
    tracing::warn!("find_tray_rect_via_appkit: no NSStatusBarWindow found");
    None
}

fn find_tray_rect_via_cgwindow() -> Option<tauri::Rect> {
    use core_foundation::array::{CFArrayGetCount, CFArrayGetValueAtIndex};
    use core_foundation::base::TCFType;
    use core_foundation::dictionary::{CFDictionaryGetValue, CFDictionaryRef};
    use core_foundation::number::{
        kCFNumberDoubleType, kCFNumberSInt64Type, CFNumberGetValue, CFNumberRef,
    };
    use core_foundation::string::CFString;
    use core_graphics::window::{
        copy_window_info, kCGNullWindowID, kCGWindowBounds, kCGWindowLayer,
        kCGWindowListOptionOnScreenOnly, kCGWindowOwnerPID,
    };
    use std::ffi::c_void;
    use tauri::{LogicalPosition, LogicalSize};

    // SAFETY helpers — wrap unsafe CF reads in small functions so the
    // hot loop stays readable. Each helper guards against null pointers.
    unsafe fn read_i64(num: CFNumberRef) -> Option<i64> {
        if num.is_null() {
            return None;
        }
        let mut out: i64 = 0;
        if CFNumberGetValue(
            num,
            kCFNumberSInt64Type,
            &mut out as *mut _ as *mut c_void,
        ) {
            Some(out)
        } else {
            None
        }
    }
    unsafe fn read_f64(num: CFNumberRef) -> Option<f64> {
        if num.is_null() {
            return None;
        }
        let mut out: f64 = 0.0;
        if CFNumberGetValue(
            num,
            kCFNumberDoubleType,
            &mut out as *mut _ as *mut c_void,
        ) {
            Some(out)
        } else {
            None
        }
    }

    let our_pid: i64 = std::process::id() as i64;
    let array = copy_window_info(kCGWindowListOptionOnScreenOnly, kCGNullWindowID)?;
    let arr_ref = array.as_concrete_TypeRef();
    let bounds_x_key = CFString::new("X");
    let bounds_y_key = CFString::new("Y");
    let bounds_w_key = CFString::new("Width");
    let bounds_h_key = CFString::new("Height");
    unsafe {
        let n = CFArrayGetCount(arr_ref);
        let mut best: Option<(f64, f64, f64, f64, i64)> = None;
        for i in 0..n {
            let dict = CFArrayGetValueAtIndex(arr_ref, i) as CFDictionaryRef;
            if dict.is_null() {
                continue;
            }
            let pid_num =
                CFDictionaryGetValue(dict, kCGWindowOwnerPID as *const c_void) as CFNumberRef;
            if read_i64(pid_num) != Some(our_pid) {
                continue;
            }
            let layer_num =
                CFDictionaryGetValue(dict, kCGWindowLayer as *const c_void) as CFNumberRef;
            let layer = read_i64(layer_num).unwrap_or(0);
            let bounds_dict =
                CFDictionaryGetValue(dict, kCGWindowBounds as *const c_void) as CFDictionaryRef;
            if bounds_dict.is_null() {
                continue;
            }
            let x = read_f64(CFDictionaryGetValue(
                bounds_dict,
                bounds_x_key.as_concrete_TypeRef() as *const c_void,
            ) as CFNumberRef)?;
            let y = read_f64(CFDictionaryGetValue(
                bounds_dict,
                bounds_y_key.as_concrete_TypeRef() as *const c_void,
            ) as CFNumberRef)?;
            let w = read_f64(CFDictionaryGetValue(
                bounds_dict,
                bounds_w_key.as_concrete_TypeRef() as *const c_void,
            ) as CFNumberRef)?;
            let h = read_f64(CFDictionaryGetValue(
                bounds_dict,
                bounds_h_key.as_concrete_TypeRef() as *const c_void,
            ) as CFNumberRef)?;
            tracing::info!(
                ?layer, x, y, w, h, "find_tray_rect: candidate window owned by us"
            );
            // The status item is the smallest of our on-screen windows; the
            // popover is 360x480 and modals are 420x520. Track minimum-area
            // candidate so we don't depend on a magic layer number.
            let area = w * h;
            if area <= 0.0 || area > 5_000.0 {
                continue;
            }
            match best {
                Some((_, _, _, _, prev_layer)) if prev_layer >= layer => {}
                _ => best = Some((x, y, w, h, layer)),
            }
        }
        if let Some((x, y, w, h, layer)) = best {
            tracing::info!(?layer, x, y, w, h, "find_tray_rect: chosen tray rect");
            let pos = LogicalPosition::<f64>::new(x, y);
            let size = LogicalSize::<f64>::new(w, h);
            return Some(tauri::Rect {
                position: pos.into(),
                size: size.into(),
            });
        }
        tracing::warn!("find_tray_rect: no candidate window owned by our pid");
    }
    None
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
