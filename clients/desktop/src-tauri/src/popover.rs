//! The tray popover window: construction, show/hide toggling, and all of the
//! monitor geometry needed to park it next to the tray icon.

use crate::state::AppState;
use crate::WINDOW_LABEL_POPOVER;
use std::sync::Arc;
use tauri::{Manager, Monitor, PhysicalPosition, WebviewUrl, WebviewWindowBuilder, WindowEvent};

const POPOVER_W: f64 = 360.0;
const POPOVER_H: f64 = 480.0;
const POPOVER_GAP: f64 = 4.0;

pub(crate) fn build_popover_window(app: &mut tauri::App) -> tauri::Result<()> {
    let win = WebviewWindowBuilder::new(
        app,
        WINDOW_LABEL_POPOVER,
        WebviewUrl::App("popover.html".into()),
    )
        .title("sharepaste")
        .inner_size(POPOVER_W, POPOVER_H)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .visible(false)
        .skip_taskbar(true)
        .build()?;
    let win_clone = win.clone();
    win.on_window_event(move |ev| {
        if let WindowEvent::Focused(false) = ev {
            let _ = win_clone.hide();
        }
    });
    Ok(())
}

/// Show the popover, or hide it if it is already visible.
///
/// `tray_rect` is the rect Tauri handed us with a tray click, if this toggle
/// came from one. `use_cached` decides whether we may fall back to the last
/// tray rect we saw (and, on macOS, to querying the status item directly): the
/// Windows hotkey path passes `false` because the cached rect there is stale
/// relative to a keyboard-driven toggle, so it wants the work-area fallback.
/// Without any rect the popover lands in a corner of the work area.
pub(crate) fn toggle_popover(
    app: &tauri::AppHandle,
    tray_rect: Option<tauri::Rect>,
    use_cached: bool,
) -> tauri::Result<()> {
    let Some(win) = app.get_webview_window("popover") else {
        return Ok(());
    };
    if win.is_visible().unwrap_or(false) {
        win.hide()?;
        return Ok(());
    }
    let rect = tray_rect.or_else(|| {
        if !use_cached {
            return None;
        }
        app.try_state::<Arc<AppState>>()
            .and_then(|s| *s.last_tray_rect.lock())
            .or_else(query_tray_rect)
    });
    if let Ok(scale) = win.scale_factor() {
        if let Some(rect) = rect {
            let pos = rect.position.to_physical::<f64>(scale);
            let size = rect.size.to_physical::<f64>(scale);
            let popover_w_phys = POPOVER_W * scale;
            let popover_h_phys = POPOVER_H * scale;
            let (work_x, work_y, work_w, work_h) = monitor_work_area_for_tray(
                &win,
                pos.x + (size.width / 2.0),
                pos.y + (size.height / 2.0),
            )
            .unwrap_or((0.0, 0.0, f64::MAX / 4.0, f64::MAX / 4.0));
            let popover_pos = calculate_popover_position(
                pos.x,
                pos.y,
                size.width,
                size.height,
                popover_w_phys,
                popover_h_phys,
                work_x,
                work_y,
                work_w,
                work_h,
            );
            set_popover_position(&win, popover_pos, scale);
        } else if let Some(popover_pos) = fallback_popover_position(&win, scale) {
            set_popover_position(&win, popover_pos, scale);
        }
    }
    win.show()?;
    win.set_focus()?;
    Ok(())
}

fn set_popover_position(
    win: &tauri::WebviewWindow,
    popover_pos: PhysicalPosition<f64>,
    _scale: f64,
) {
    // Move the underlying NSWindow synchronously via `setFrameTopLeftPoint:`
    // so the move lands before the next paint. Tauri's set_position dispatches
    // through wry's event loop and races with show(), causing a one-frame flash
    // at the previous default location (center of screen).
    #[cfg(target_os = "macos")]
    {
        let x_logical = popover_pos.x / _scale;
        let y_logical = popover_pos.y / _scale;
        set_ns_window_top_left(win, x_logical, y_logical);
    }
    // Also call Tauri's set_position so wry's cached position matches the
    // actual frame on subsequent operations.
    let _ = win.set_position(popover_pos);
}

fn calculate_popover_position(
    tray_x: f64,
    tray_y: f64,
    tray_w: f64,
    tray_h: f64,
    popover_w: f64,
    popover_h: f64,
    work_x: f64,
    work_y: f64,
    work_w: f64,
    work_h: f64,
) -> PhysicalPosition<f64> {
    let work_right = work_x + work_w;
    let work_bottom = work_y + work_h;
    let x = clamp_to_range(
        tray_x + (tray_w / 2.0) - (popover_w / 2.0),
        work_x,
        work_right - popover_w,
    );
    let below_y = tray_y + tray_h + POPOVER_GAP;
    let above_y = tray_y - popover_h - POPOVER_GAP;
    let y = if below_y + popover_h <= work_bottom {
        below_y
    } else if above_y >= work_y {
        above_y
    } else {
        clamp_to_range(below_y, work_y, work_bottom - popover_h)
    };

    PhysicalPosition::new(x, y)
}

fn clamp_to_range(value: f64, min: f64, max: f64) -> f64 {
    if max < min {
        min
    } else {
        value.clamp(min, max)
    }
}

fn monitor_work_area_for_tray(
    win: &tauri::WebviewWindow,
    tray_center_x: f64,
    tray_center_y: f64,
) -> Option<(f64, f64, f64, f64)> {
    let monitors = win.available_monitors().ok()?;
    monitors
        .iter()
        .find(|m| monitor_contains(m, tray_center_x, tray_center_y))
        .or_else(|| monitors.first())
        .map(monitor_work_area)
}

fn monitor_contains(monitor: &Monitor, x: f64, y: f64) -> bool {
    let position = monitor.position();
    let size = monitor.size();
    let left = position.x as f64;
    let top = position.y as f64;
    let right = left + size.width as f64;
    let bottom = top + size.height as f64;
    x >= left && x <= right && y >= top && y <= bottom
}

fn monitor_work_area(monitor: &Monitor) -> (f64, f64, f64, f64) {
    let area = monitor.work_area();
    (
        area.position.x as f64,
        area.position.y as f64,
        area.size.width as f64,
        area.size.height as f64,
    )
}

fn fallback_popover_position(
    win: &tauri::WebviewWindow,
    scale: f64,
) -> Option<PhysicalPosition<f64>> {
    let monitor = win.current_monitor().ok().flatten().or_else(|| {
        win.primary_monitor()
            .ok()
            .flatten()
            .or_else(|| win.available_monitors().ok()?.into_iter().next())
    })?;
    let monitor_pos = monitor.position();
    let monitor_size = monitor.size();
    let area = monitor.work_area();
    Some(calculate_fallback_popover_position(
        monitor_pos.x as f64,
        monitor_pos.y as f64,
        monitor_size.width as f64,
        monitor_size.height as f64,
        area.position.x as f64,
        area.position.y as f64,
        area.size.width as f64,
        area.size.height as f64,
        POPOVER_W * scale,
        POPOVER_H * scale,
    ))
}

fn calculate_fallback_popover_position(
    monitor_x: f64,
    monitor_y: f64,
    monitor_w: f64,
    monitor_h: f64,
    work_x: f64,
    work_y: f64,
    work_w: f64,
    work_h: f64,
    popover_w: f64,
    popover_h: f64,
) -> PhysicalPosition<f64> {
    let monitor_right = monitor_x + monitor_w;
    let monitor_bottom = monitor_y + monitor_h;
    let work_right = work_x + work_w;
    let work_bottom = work_y + work_h;
    let bottom_inset = monitor_bottom - work_bottom;
    let top_inset = work_y - monitor_y;
    let right_inset = monitor_right - work_right;
    let left_inset = work_x - monitor_x;

    if bottom_inset >= top_inset && bottom_inset >= right_inset && bottom_inset >= left_inset {
        PhysicalPosition::new(
            clamp_to_range(work_right - popover_w - POPOVER_GAP, work_x, work_right - popover_w),
            clamp_to_range(work_bottom - popover_h - POPOVER_GAP, work_y, work_bottom - popover_h),
        )
    } else if top_inset >= right_inset && top_inset >= left_inset {
        PhysicalPosition::new(
            clamp_to_range(work_right - popover_w - POPOVER_GAP, work_x, work_right - popover_w),
            clamp_to_range(work_y + POPOVER_GAP, work_y, work_bottom - popover_h),
        )
    } else if right_inset >= left_inset {
        PhysicalPosition::new(
            clamp_to_range(work_right - popover_w - POPOVER_GAP, work_x, work_right - popover_w),
            clamp_to_range(work_bottom - popover_h - POPOVER_GAP, work_y, work_bottom - popover_h),
        )
    } else {
        PhysicalPosition::new(
            clamp_to_range(work_x + POPOVER_GAP, work_x, work_right - popover_w),
            clamp_to_range(work_bottom - popover_h - POPOVER_GAP, work_y, work_bottom - popover_h),
        )
    }
}

#[cfg(target_os = "macos")]
fn query_tray_rect() -> Option<tauri::Rect> {
    crate::core::capture::macos::find_tray_rect()
}

#[cfg(target_os = "macos")]
fn set_ns_window_top_left(win: &tauri::WebviewWindow, x_logical: f64, y_logical: f64) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2::Encode;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSPoint {
        x: f64,
        y: f64,
    }
    unsafe impl Encode for NSPoint {
        const ENCODING: objc2::Encoding = objc2::Encoding::Struct(
            "CGPoint",
            &[objc2::Encoding::Double, objc2::Encoding::Double],
        );
    }

    let Ok(raw) = win.ns_window() else { return };
    if raw.is_null() {
        return;
    }
    // setFrameTopLeftPoint takes a screen point in logical coordinates with
    // y measured from the bottom of the screen. We were given a top-left
    // (Tauri-style) y; convert by flipping against the primary screen.
    use objc2_app_kit::NSScreen;
    use objc2_foundation::MainThreadMarker;
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some(screen) = NSScreen::mainScreen(mtm) else { return };
    let screen_h = screen.frame().size.height;
    let cocoa_y = screen_h - y_logical;
    let p = NSPoint { x: x_logical, y: cocoa_y };
    let ns_window = raw as *mut AnyObject;
    unsafe {
        let _: () = msg_send![ns_window, setFrameTopLeftPoint: p];
    }
}

#[cfg(not(target_os = "macos"))]
fn query_tray_rect() -> Option<tauri::Rect> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_popover_above_bottom_tray_and_inside_work_area() {
        let pos = calculate_popover_position(
            1260.0, 728.0, 32.0, 32.0, 360.0, 480.0, 0.0, 0.0, 1366.0, 728.0,
        );

        assert_eq!(pos.x, 1006.0);
        assert_eq!(pos.y, 244.0);
    }

    #[test]
    fn falls_back_to_bottom_right_when_taskbar_reduces_bottom_work_area() {
        let pos = calculate_fallback_popover_position(
            0.0, 0.0, 1366.0, 768.0, 0.0, 0.0, 1366.0, 728.0, 360.0, 480.0,
        );

        assert_eq!(pos.x, 1002.0);
        assert_eq!(pos.y, 244.0);
    }
}
