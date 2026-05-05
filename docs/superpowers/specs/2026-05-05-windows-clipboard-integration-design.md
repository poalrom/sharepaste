# Sharepaste Windows Clipboard Integration Design

## Context

The desktop client currently has feature-complete clipboard auto-capture on
macOS. Windows can compile and launch with reduced scope: manual history copy
works through `arboard`, but clipboard auto-capture is not started.

The macOS implementation already separates the cross-platform capture filter
from platform-specific clipboard inspection. `clipboard-master` is used for
clipboard change events and already supports Windows through a Win32
`WM_CLIPBOARDUPDATE` listener. The missing Windows work is wiring that watcher
into the app on Windows and adding a Windows clipboard/frontmost-process
adapter.

## Goal

Support Windows clipboard auto-capture the same way macOS does at the product
level: when capture is enabled, text clipboard changes are encrypted locally,
queued in `pending_uploads`, and uploaded through the existing sync pipeline.

## Non-Goals

This pass does not add image, file, or rich clipboard support. It does not add
Windows-specific concealed or transient clipboard detection beyond what can be
done with the current shared filter. It does not change the server protocol,
history UI, pairing, encryption, or sync behavior.

## Approach

Reuse the current capture pipeline and keep platform differences behind small
adapters:

- Compile `core::capture::watcher` on macOS and Windows.
- Keep using `clipboard-master` for change events on both platforms.
- Add `core::capture::windows` with a `WindowsClipboardSniffer` and
  `frontmost_process_name()`.
- Make `spawn_clipboard_capture` available on macOS and Windows, selecting the
  platform sniffer and foreground-source lookup with `cfg`.
- Keep the filter, encryption, pending queue, and uploader unchanged.

## Components

### Watcher

`clients/desktop/src-tauri/src/core/capture/watcher.rs` should be gated for
`target_os = "macos"` or `target_os = "windows"`. Its public shape can stay the
same: `spawn(Sender<ClipboardEvent>, CancellationToken)` creates a background
thread and emits lightweight change events into the Tauri async task.

On Windows, `clipboard-master` creates a hidden message-only window and listens
for `WM_CLIPBOARDUPDATE`, so no polling loop is needed.

### Windows Sniffer

`clients/desktop/src-tauri/src/core/capture/windows.rs` should implement the
existing `PasteboardSniff` trait.

`WindowsClipboardSniffer::read_text()` reads text from the system clipboard
with `arboard`. Clipboard-open failures, non-text content, and unavailable text
return `None` so the shared filter skips the event as `NonText`.

`WindowsClipboardSniffer::types()` can return a small normalized type list. For
the first pass, returning `["text/plain"]` when text is readable and an empty
list otherwise is enough. Windows does not have a direct equivalent to the
macOS `NSPasteboard` concealed/transient type names used by the current filter.

### Foreground Source

`frontmost_process_name()` should query the foreground window process and return
the executable file name, for example `1Password.exe` or `Bitwarden.exe`.
Failures return `None`.

The settings `deny_list` remains one list. On macOS entries are bundle IDs. On
Windows entries are executable names. Matching stays case-insensitive through
the existing shared filter.

### Capture Startup

Tauri setup should start clipboard capture on both macOS and Windows:

- macOS uses `NSPasteboardSniffer` and `frontmost_bundle_id()`.
- Windows uses `WindowsClipboardSniffer` and `frontmost_process_name()`.
- Other platforms still do not start capture.

## Data Flow

Windows clipboard updates follow the same flow as macOS:

1. `clipboard-master` receives a clipboard change event.
2. The app loads the active account and capture settings.
3. The platform sniffer reads available clipboard metadata and text.
4. The shared filter applies capture-enabled, transient, non-text, size,
   deny-list, and self-write checks.
5. Accepted text is encrypted with the active user's key.
6. Ciphertext is enqueued in `pending_uploads`.
7. The existing uploader is notified and flushes pending rows when online.

No plaintext is sent over IPC or logged.

## Error Handling

If the Windows watcher fails to start, the app logs the error and continues
running without auto-capture. If foreground process lookup fails, capture
continues with no source process. If clipboard text cannot be read, the filter
skips the event as non-text.

Manual history copy keeps the existing `last_self_write` behavior, so copied
history entries are not immediately captured again by the Windows watcher.

## Expected Behavior

On Windows:

- Capturing text into the system clipboard enqueues and syncs it for the active
  account when capture is enabled.
- Disabling capture in settings stops new clipboard captures.
- Adding a foreground executable name to the deny-list skips clipboard changes
  copied while that executable is foreground.
- Copying an entry from Sharepaste history does not re-capture that same text
  within the self-write window.
- Non-text clipboard contents are ignored.

On macOS:

- Existing clipboard capture behavior remains unchanged.

## Testing

Unit testing should keep the shared filter coverage as the main correctness
surface. Add or preserve tests for case-insensitive deny-list matching because
Windows process names depend on that behavior.

Windows-specific live tests should be ignored by default, similar to the macOS
AppKit smoke tests:

- `WindowsClipboardSniffer::read_text()` does not panic.
- `frontmost_process_name()` does not panic.

Verification should include:

- `cargo test` from `clients/desktop/src-tauri`.
- `cargo check` from `clients/desktop/src-tauri`.
- On Windows, `npm.cmd run tauri dev`, pair an account, copy text in another
  app, and confirm the entry appears and uploads.
