#![cfg(target_os = "windows")]

use crate::core::capture::filter::PasteboardSniff;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId,
};

/// Real implementation of [`PasteboardSniff`] backed by the Windows clipboard.
/// This adapter only exposes plain text for the first Windows capture pass.
pub struct WindowsClipboardSniffer;

impl WindowsClipboardSniffer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WindowsClipboardSniffer {
    fn default() -> Self {
        Self::new()
    }
}

impl PasteboardSniff for WindowsClipboardSniffer {
    fn types(&self) -> Vec<String> {
        if self.read_text().is_some() {
            vec!["text/plain".to_string()]
        } else {
            Vec::new()
        }
    }

    fn read_text(&self) -> Option<String> {
        let mut clipboard = arboard::Clipboard::new().ok()?;
        clipboard.get_text().ok()
    }
}

/// Returns the executable file name for the foreground window's owning
/// process, for example `1Password.exe`. Returns `None` when there is no
/// foreground window, process access is denied, or the image path cannot be
/// queried.
pub fn frontmost_process_name() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }

        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }

        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }

        let mut buf = vec![0u16; 32_768];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut len);
        let _ = CloseHandle(process);
        if ok == 0 || len == 0 {
            return None;
        }

        let path = PathBuf::from(OsString::from_wide(&buf[..len as usize]));
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
    }
}
