//! What the shell holds: the core, plus the three things that cannot cross into
//! it because they are Tauri types.

use parking_lot::Mutex;
use sharepaste_core::errors::AppError;
use sharepaste_core::facade::Sharepaste;
use sharepaste_core::platform::Clipboard;
use std::sync::Arc;

/// The system clipboard, for the core.
///
/// A fresh `arboard::Clipboard` per call, exactly as `copy_to_clipboard` did
/// before the write moved into the facade: the handle is cheap and holding one
/// open across the app's lifetime is what the crate warns against.
pub(crate) struct SystemClipboard;

impl Clipboard for SystemClipboard {
    fn read_text(&self) -> Result<Option<String>, AppError> {
        let mut cb = arboard::Clipboard::new().map_err(|e| AppError::Storage(e.to_string()))?;
        match cb.get_text() {
            Ok(text) => Ok(Some(text)),
            // Anything that is not text is not an error here; Watched Capture
            // goes through `PasteboardSniff`, which inspects types first.
            Err(_) => Ok(None),
        }
    }

    fn write_text(&self, text: &str) -> Result<(), AppError> {
        let mut cb = arboard::Clipboard::new().map_err(|e| AppError::Storage(e.to_string()))?;
        cb.set_text(text.to_string())
            .map_err(|e| AppError::Storage(e.to_string()))
    }
}

pub(crate) struct AppState {
    pub(crate) core: Arc<Sharepaste>,

    // -- shell-only: Tauri types, which must never cross into the core -----
    pub(crate) last_tray_rect: Mutex<Option<tauri::Rect>>,
    /// The Release the last check found, held because three callers need the
    /// same answer: the tray item's presence, the Settings pane, and the
    /// install path — which needs the very `Update` whose signature verified.
    pub(crate) pending_update: Mutex<Option<tauri_plugin_updater::Update>>,
    /// The tray menu, so the install item can be added and removed after
    /// `build_tray` has finished.
    ///
    /// Every other tray item is built once and never touched; this one is the
    /// exception, and reaching it from a check that runs on the async runtime
    /// means the menu has to outlive the setup hook.
    pub(crate) tray_menu: Mutex<Option<tauri::menu::Menu<tauri::Wry>>>,
}

impl AppState {
    pub(crate) fn new(core: Arc<Sharepaste>) -> Self {
        Self {
            core,
            last_tray_rect: Mutex::new(None),
            pending_update: Mutex::new(None),
            tray_menu: Mutex::new(None),
        }
    }
}
