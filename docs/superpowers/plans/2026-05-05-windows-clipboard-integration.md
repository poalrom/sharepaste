# Windows Clipboard Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Windows clipboard auto-capture with the same encrypted pending-upload pipeline already used on macOS.

**Architecture:** Reuse the existing `clipboard-master` watcher and shared `capture::filter` logic. Add a Windows platform adapter for clipboard text reads and foreground executable lookup, then start the existing capture task on Windows as well as macOS.

**Tech Stack:** Rust 2021, Tauri 2, `clipboard-master`, `arboard`, `windows-sys`, Tokio, rusqlite.

---

## File Structure

- Modify: `clients/desktop/src-tauri/Cargo.toml`
  - Add Windows-only dependencies for `clipboard-master` and `windows-sys`.
- Modify: `clients/desktop/src-tauri/src/core/capture/mod.rs`
  - Expose `watcher` on macOS and Windows.
  - Expose the new Windows platform adapter behind `cfg(target_os = "windows")`.
- Modify: `clients/desktop/src-tauri/src/core/capture/watcher.rs`
  - Change the file-level `cfg` from macOS-only to macOS-or-Windows.
- Create: `clients/desktop/src-tauri/src/core/capture/windows.rs`
  - Implement `WindowsClipboardSniffer`.
  - Implement `frontmost_process_name()`.
  - Add ignored live smoke tests.
- Modify: `clients/desktop/src-tauri/src/lib.rs`
  - Start clipboard capture on macOS and Windows.
  - Select the platform sniffer and foreground source with `cfg`.
- Modify: `clients/desktop/README.md`
  - Update Windows feature status and smoke checklist.

Before implementation, check `git status --short`. At the time this plan was written, `clients/desktop/src-tauri/src/lib.rs` already had unrelated uncommitted Windows popover-positioning changes. Preserve them. When committing Task 3, stage only the clipboard-capture hunks for `lib.rs`; if clean non-interactive staging is not possible, leave Task 3 uncommitted and report that the file already had unrelated changes.

---

### Task 1: Enable the Shared Watcher on Windows

**Files:**
- Modify: `clients/desktop/src-tauri/Cargo.toml`
- Modify: `clients/desktop/src-tauri/src/core/capture/mod.rs`
- Modify: `clients/desktop/src-tauri/src/core/capture/watcher.rs`

- [ ] **Step 1: Confirm the current Windows watcher is not compiled**

Run:

```powershell
rg -n "target_os = `"macos`"|pub mod watcher|clipboard-master" clients\desktop\src-tauri\Cargo.toml clients\desktop\src-tauri\src\core\capture\mod.rs clients\desktop\src-tauri\src\core\capture\watcher.rs
```

Expected: `watcher.rs` and `mod.rs` are macOS-only, and `clipboard-master` is only in the macOS target dependencies.

- [ ] **Step 2: Add Windows dependencies**

Edit `clients/desktop/src-tauri/Cargo.toml` so the Windows target section becomes:

```toml
[target.'cfg(target_os = "windows")'.dependencies]
keyring = { version = "3", default-features = false, features = ["windows-native"] }
clipboard-master = "4"
windows-sys = { version = "0.61", features = ["Win32_Foundation", "Win32_System_Threading", "Win32_UI_WindowsAndMessaging"] }
```

Leave the existing macOS `clipboard-master = "4"` dependency in place.

- [ ] **Step 3: Expose watcher on macOS and Windows**

Edit `clients/desktop/src-tauri/src/core/capture/mod.rs` to:

```rust
pub mod filter;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub mod watcher;
```

- [ ] **Step 4: Change watcher file-level cfg**

Edit the top of `clients/desktop/src-tauri/src/core/capture/watcher.rs` from:

```rust
// Implemented in Task 16.
#![cfg(target_os = "macos")]
```

to:

```rust
#![cfg(any(target_os = "macos", target_os = "windows"))]
```

Leave the rest of the watcher implementation unchanged.

- [ ] **Step 5: Run check to verify the watcher compiles on Windows**

Run:

```powershell
cd clients\desktop\src-tauri
cargo check
```

Expected: compile gets past `core::capture::watcher`. If it fails because `core::capture::windows` does not exist yet after Step 3, continue to Task 2 and rerun `cargo check` there.

- [ ] **Step 6: Commit Task 1**

Run:

```powershell
git add clients\desktop\src-tauri\Cargo.toml clients\desktop\src-tauri\src\core\capture\mod.rs clients\desktop\src-tauri\src\core\capture\watcher.rs
git commit -m "feat(desktop): compile clipboard watcher on windows"
```

If Task 1 cannot compile until Task 2 creates `windows.rs`, defer this commit until Task 2 passes and use the Task 2 commit message.

---

### Task 2: Add the Windows Clipboard Adapter

**Files:**
- Create: `clients/desktop/src-tauri/src/core/capture/windows.rs`
- Test: `clients/desktop/src-tauri/src/core/capture/windows.rs`

- [ ] **Step 1: Write the failing Windows adapter tests**

Create `clients/desktop/src-tauri/src/core/capture/windows.rs` with only this test scaffold:

```rust
#![cfg(target_os = "windows")]

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::capture::filter::PasteboardSniff;

    #[test]
    #[ignore = "live Windows clipboard call; run manually on a developer desktop"]
    fn read_text_call_does_not_panic() {
        let sniff = WindowsClipboardSniffer::new();
        let _ = sniff.read_text();
    }

    #[test]
    #[ignore = "live Win32 foreground-window call; run manually on a developer desktop"]
    fn frontmost_process_name_call_does_not_panic() {
        let _ = frontmost_process_name();
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```powershell
cd clients\desktop\src-tauri
cargo test core::capture::windows -- --list
```

Expected: FAIL with unresolved names for `WindowsClipboardSniffer` and `frontmost_process_name`.

- [ ] **Step 3: Implement the Windows adapter**

Replace `clients/desktop/src-tauri/src/core/capture/windows.rs` with:

```rust
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::capture::filter::PasteboardSniff;

    #[test]
    #[ignore = "live Windows clipboard call; run manually on a developer desktop"]
    fn read_text_call_does_not_panic() {
        let sniff = WindowsClipboardSniffer::new();
        let _ = sniff.read_text();
    }

    #[test]
    #[ignore = "live Win32 foreground-window call; run manually on a developer desktop"]
    fn frontmost_process_name_call_does_not_panic() {
        let _ = frontmost_process_name();
    }
}
```

- [ ] **Step 4: Run tests to verify the adapter compiles**

Run:

```powershell
cd clients\desktop\src-tauri
cargo test core::capture::windows -- --list
```

Expected: PASS and lists the two ignored Windows smoke tests.

- [ ] **Step 5: Run full Rust tests**

Run:

```powershell
cd clients\desktop\src-tauri
cargo test
```

Expected: PASS. The new Windows live tests are ignored by default.

- [ ] **Step 6: Commit Task 2**

Run:

```powershell
git add clients\desktop\src-tauri\Cargo.toml clients\desktop\src-tauri\src\core\capture\mod.rs clients\desktop\src-tauri\src\core\capture\watcher.rs clients\desktop\src-tauri\src\core\capture\windows.rs
git commit -m "feat(desktop): add windows clipboard sniffer"
```

If Task 1 was already committed, stage and commit only `clients/desktop/src-tauri/src/core/capture/windows.rs` with the same commit message.

---

### Task 3: Start Clipboard Capture on Windows

**Files:**
- Modify: `clients/desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: Make startup call Windows capture before the function supports it**

In `clients/desktop/src-tauri/src/lib.rs`, change the setup gate from:

```rust
#[cfg(target_os = "macos")]
spawn_clipboard_capture(app.handle().clone(), app_state.clone());
```

to:

```rust
#[cfg(any(target_os = "macos", target_os = "windows"))]
spawn_clipboard_capture(app.handle().clone(), app_state.clone());
```

- [ ] **Step 2: Run check to verify it fails on Windows**

Run:

```powershell
cd clients\desktop\src-tauri
cargo check
```

Expected on Windows: FAIL because `spawn_clipboard_capture` is still compiled only for macOS.

- [ ] **Step 3: Make `spawn_clipboard_capture` select platform adapters**

In `clients/desktop/src-tauri/src/lib.rs`, replace the current macOS-only `spawn_clipboard_capture` function, from its `#[cfg(target_os = "macos")]` attribute through the closing brace of the function, with:

```rust
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn spawn_clipboard_capture(app: tauri::AppHandle, state: Arc<AppState>) {
    use crate::core::capture::filter::{evaluate, CaptureContext, FilterDecision, PasteboardSniff};
    #[cfg(target_os = "macos")]
    use crate::core::capture::macos::{frontmost_bundle_id, NSPasteboardSniffer};
    use crate::core::capture::watcher;
    #[cfg(target_os = "windows")]
    use crate::core::capture::windows::{frontmost_process_name, WindowsClipboardSniffer};

    let (tx, mut rx) = mpsc::channel::<crate::core::capture::watcher::ClipboardEvent>(32);
    let watcher_cancel = CancellationToken::new();
    if let Err(e) = watcher::spawn(tx, watcher_cancel.clone()) {
        tracing::error!(err = %e, "clipboard watcher failed to start");
        return;
    }
    tauri::async_runtime::spawn(async move {
        #[cfg(target_os = "macos")]
        let sniffer = NSPasteboardSniffer::new();
        #[cfg(target_os = "windows")]
        let sniffer = WindowsClipboardSniffer::new();

        while let Some(_ev) = rx.recv().await {
            let Some(user_id) = state.registry.active_user_id() else {
                continue;
            };
            let settings = {
                let conn = state.conn.lock().await;
                match crate::core::storage::settings::load(&conn) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(err = %e, "load settings");
                        continue;
                    }
                }
            };
            #[cfg(target_os = "macos")]
            let frontmost = frontmost_bundle_id();
            #[cfg(target_os = "windows")]
            let frontmost = frontmost_process_name();

            let last_self = state.last_self_write.lock().clone();
            let last_self_ref = last_self.as_ref().map(|(t, s)| (*t, s.as_str()));
            let ctx = CaptureContext {
                capture_enabled: settings.capture_enabled,
                deny_list: &settings.deny_list,
                frontmost_bundle_id: frontmost.as_deref(),
                last_self_write: last_self_ref,
            };
            let decision =
                evaluate(&ctx, &sniffer as &dyn PasteboardSniff, std::time::Instant::now());
            let text = match decision {
                FilterDecision::Capture(t) => t,
                FilterDecision::Skip(reason) => {
                    tracing::debug!(?reason, "clipboard skip");
                    continue;
                }
            };
            let m = match state.registry.load_active_membership(&user_id).await {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!(err = %e, "no active membership for capture");
                    continue;
                }
            };
            let ciphertext =
                match crate::core::crypto::encrypt(&m.user_key, &user_id, text.as_bytes()) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!(err = %e, "encrypt failed");
                        continue;
                    }
                };
            {
                let conn = state.conn.lock().await;
                if let Err(e) =
                    crate::core::storage::pending::enqueue(&conn, &user_id, &ciphertext, now_ms())
                {
                    tracing::warn!(err = %e, "enqueue failed");
                    continue;
                }
                let count = crate::core::storage::pending::count(&conn, &user_id).unwrap_or(0);
                let _ = app.emit(
                    PENDING_COUNT,
                    PendingCount {
                        user_id: user_id.clone(),
                        count,
                    },
                );
            }
            if let Some(trigger) = state.upload_triggers.lock().get(&user_id).cloned() {
                trigger.notify_one();
            } else {
                tracing::warn!(%user_id, "no uploader trigger registered");
            }
        }
        let _ = watcher_cancel;
    });
}
```

- [ ] **Step 4: Run check to verify capture startup compiles**

Run:

```powershell
cd clients\desktop\src-tauri
cargo check
```

Expected: PASS on Windows.

- [ ] **Step 5: Run full Rust tests**

Run:

```powershell
cd clients\desktop\src-tauri
cargo test
```

Expected: PASS.

- [ ] **Step 6: Commit Task 3**

First inspect the diff:

```powershell
git diff -- clients\desktop\src-tauri\src\lib.rs
```

If the diff contains only clipboard-capture changes, run:

```powershell
git add clients\desktop\src-tauri\src\lib.rs
git commit -m "feat(desktop): start clipboard capture on windows"
```

If the diff also contains pre-existing popover-positioning changes, do not use `git add clients\desktop\src-tauri\src\lib.rs`. Stage only the clipboard-capture hunk with `git add -p clients\desktop\src-tauri\src\lib.rs`, commit with the same message, and leave the unrelated hunk unstaged.

---

### Task 4: Update Windows Desktop Documentation

**Files:**
- Modify: `clients/desktop/README.md`

- [ ] **Step 1: Confirm the README still describes Windows capture as disabled**

Run:

```powershell
rg -n "reduced clipboard scope|auto-capture|does not start clipboard|manual history copy" clients\desktop\README.md
```

Expected: output includes text saying Windows manual history copy works but clipboard auto-capture is still macOS-only or not started.

- [ ] **Step 2: Update the Windows status text**

In `clients/desktop/README.md`, replace the opening status paragraph:

```markdown
Tauri 2 client for the sharepaste self-hosted clipboard sync server. macOS is
the feature-complete target. Windows currently supports compile and launch with
reduced clipboard scope: manual history copy works, but clipboard auto-capture
is still macOS-only.
```

with:

```markdown
Tauri 2 client for the sharepaste self-hosted clipboard sync server. macOS and
Windows support text clipboard auto-capture, encrypted sync, searchable history,
and manual history copy.
```

- [ ] **Step 3: Update the Windows smoke checklist**

In `clients/desktop/README.md`, replace the current Windows smoke checklist with:

```markdown
## Windows smoke checklist

1. `npm.cmd --prefix ui run build` - Vite production build succeeds.
2. `cd src-tauri && cargo check` - Rust/Tauri check succeeds.
3. `npm.cmd run tauri dev` - desktop shell opens.
4. Pair against a running server.
5. Copy text in another app and verify the entry appears in the popover.
6. Add the foreground app executable name to the deny-list, copy new text from
   that app, and verify the entry is skipped.
7. Copy an existing history item from the UI and verify it is not immediately
   re-captured as a new entry.
```

- [ ] **Step 4: Confirm old reduced-scope wording is gone**

Run:

```powershell
rg -n "reduced clipboard scope|auto-capture is still macOS-only|does not start clipboard auto-capture" clients\desktop\README.md
```

Expected: no matches.

- [ ] **Step 5: Commit Task 4**

Run:

```powershell
git add clients\desktop\README.md
git commit -m "docs(desktop): document windows clipboard capture"
```

---

### Task 5: Final Verification

**Files:**
- Verify only.

- [ ] **Step 1: Run Rust tests**

Run:

```powershell
cd clients\desktop\src-tauri
cargo test
```

Expected: PASS.

- [ ] **Step 2: Run Rust check**

Run:

```powershell
cd clients\desktop\src-tauri
cargo check
```

Expected: PASS.

- [ ] **Step 3: Run UI build**

Run:

```powershell
npm.cmd --prefix clients\desktop\ui run build
```

Expected: PASS and Vite reports a production build.

- [ ] **Step 4: Run optional live Windows smoke test**

Run:

```powershell
cd clients\desktop
npm.cmd run tauri dev
```

Expected: the desktop shell opens. Pair against a running server, copy text in another app, and verify the entry appears in Sharepaste history. This step is manual because it opens a desktop app and depends on a running server.

- [ ] **Step 5: Inspect final diff**

Run:

```powershell
git status --short
git diff --check
```

Expected: no whitespace errors. Any remaining uncommitted changes should be intentional and described in the final handoff, especially pre-existing `lib.rs` popover-positioning changes.

---

## Self-Review

Spec coverage:

- Windows watcher compilation: Task 1.
- `WindowsClipboardSniffer`: Task 2.
- Foreground executable deny-list source: Task 2.
- Start capture on Windows using shared pipeline: Task 3.
- Error handling by returning `None` for clipboard/process lookup failures: Task 2 and Task 3.
- Existing self-write guard preserved: Task 3.
- README and smoke verification: Task 4 and Task 5.

Completeness scan: no incomplete tasks remain. Every code-changing task includes exact file paths, snippets, commands, and expected outcomes.

Type consistency: `WindowsClipboardSniffer`, `frontmost_process_name`, `PasteboardSniff`, `spawn_clipboard_capture`, and `watcher::spawn` names are consistent across tasks.
