//! macOS focus restoration for an explicitly dismissed Popover.
//!
//! Call these functions on the main thread. Retain the application, not its PID,
//! so an application that exits cannot be replaced by an unrelated process.

use objc2::rc::Retained;
use objc2::runtime::NSObjectProtocol;
use objc2::sel;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSRunningApplication, NSWorkspace,
};
use objc2_foundation::MainThreadMarker;
use std::cell::RefCell;

thread_local! {
    static PREVIOUS_APP: RefCell<Option<Retained<NSRunningApplication>>> = const { RefCell::new(None) };
}

pub(crate) fn remember() {
    // SAFETY: AppKit permits these application queries from the main thread.
    let previous = unsafe {
        NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .filter(|app| **app != *NSRunningApplication::currentApplication())
    };
    PREVIOUS_APP.with(|slot| *slot.borrow_mut() = previous);
}

pub(crate) fn clear() {
    PREVIOUS_APP.with(|slot| slot.borrow_mut().take());
}

pub(crate) fn restore() {
    let Some(previous) = PREVIOUS_APP.with(|slot| slot.borrow_mut().take()) else {
        return;
    };
    // SAFETY: The caller runs on the main thread. The retained application
    // remains valid even if it has exited. Check that it is still running.
    unsafe {
        let current = NSRunningApplication::currentApplication();
        if previous.isTerminated() || !current.isActive() {
            return;
        }
        let mtm = MainThreadMarker::new()
            .expect("popover focus restoration must run on the main thread");
        let app = NSApplication::sharedApplication(mtm);
        // Yield before activation on macOS 14 and later. Check availability
        // to keep the macOS 12 support floor.
        let activated = if app.respondsToSelector(sel!(yieldActivationToApplication:)) {
            app.yieldActivationToApplication(&previous);
            previous.activateWithOptions(NSApplicationActivationOptions::empty())
        } else {
            #[allow(deprecated)]
            previous.activateWithOptions(
                NSApplicationActivationOptions::NSApplicationActivateIgnoringOtherApps,
            )
        };
        if !activated {
            tracing::warn!("previous application declined popover focus restoration");
        }
    }
}
