# Sharepaste Windows Compile And Launch Design

## Context

The desktop client currently targets macOS. The docs, top-level Makefile,
Tauri bundle settings, and several Rust code paths assume macOS behavior.
On Windows, the UI build succeeds when invoked through `npm.cmd`, but the
Tauri/Rust build fails before application code finishes checking because
`src-tauri/icons/icon.ico` is missing. Additional Windows launch risks are
present in macOS-only dependency features and clipboard APIs.

## Goal

Make the desktop client compile and launch on Windows with reduced feature
scope. The first Windows pass should open the app, show the existing UI, and
keep account, pairing, history, sync, settings, and manual copy flows usable
where they do not depend on platform clipboard watching.

## Non-Goals

This pass does not add Windows clipboard monitoring, Windows-specific tray
positioning, installer polish, or full feature parity with the macOS client.
Clipboard auto-capture can remain disabled on Windows until a dedicated
Windows parity pass.

## Approach

Use a minimal platform-enablement pass:

- Add a Windows `.ico` icon so Tauri can generate Windows resources.
- Make Rust dependencies platform-correct, especially `keyring` backend
  features for macOS and Windows.
- Keep macOS clipboard capture and macOS private positioning behind existing
  `cfg(target_os = "macos")` guards.
- Make manual `copy_to_clipboard` work on Windows by using `arboard` outside
  the macOS-only dependency section.
- Add Windows-safe developer command guidance where needed, using `npm.cmd`
  for PowerShell environments that block `npm.ps1`.

## Expected Behavior

On Windows:

- `npm.cmd --prefix clients/desktop/ui run build` succeeds.
- `cargo check` from `clients/desktop/src-tauri` succeeds.
- `npm.cmd --prefix clients/desktop run tauri dev` launches the desktop shell
  when Tauri/WebView2 prerequisites are installed.
- The app does not attempt to start the macOS clipboard watcher.
- Manual copy from history uses the system clipboard.

On macOS:

- Existing macOS behavior remains intact.
- Clipboard capture continues to use the current macOS watcher.
- The existing `.app` and `.dmg` build path remains unchanged.

## Error Handling

Windows-specific unavailable features should fail closed. Clipboard
auto-capture is not started on Windows, so it should not emit runtime errors
or partially enqueue clipboard data. Manual copy errors should continue to be
reported through the existing `AppError` path.

## Testing

Verification should include:

- UI build with `npm.cmd --prefix clients/desktop/ui run build`.
- Rust check with `cargo check` in `clients/desktop/src-tauri`.
- If local prerequisites allow it, a Tauri launch smoke test with
  `npm.cmd --prefix clients/desktop run tauri dev`.

The launch smoke test may need to be manual because it opens a desktop app.
