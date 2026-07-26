# sharepaste - desktop client

Tauri 2 client for the sharepaste self-hosted clipboard sync server. macOS and
Windows support text clipboard auto-capture, encrypted sync, searchable history,
and manual history copy.

Specs:

- `docs/superpowers/specs/2026-05-01-sharepaste-macos-design.md`
- `docs/superpowers/specs/2026-05-03-sharepaste-windows-compile-launch-design.md`

## Prerequisites

- Rust stable (`rustup` will pick up `rust-toolchain.toml`)
- Node 25
- A running sharepaste server (see top-level repo README; for dev, `cd server && npx tsx src/index.ts serve`)

macOS:

- macOS 12+ (Monterey)
- Xcode command-line tools (`xcode-select --install`)

Windows:

- Windows 10/11
- Microsoft WebView2 Runtime
- Microsoft C++ Build Tools with the MSVC toolchain

## Dev workflow

```sh
# from the project root
cd clients/desktop
npm install
npm --prefix ui install
npm run dev    # boots Vite at :1420 + opens the Tauri shell
```

On Windows PowerShell, use `npm.cmd` if script execution policy blocks `npm.ps1`:

```powershell
npm.cmd install
npm.cmd --prefix ui install
npm.cmd run dev
```

`SHAREPASTE_DATA_DIR=/tmp/sp-a npm run dev` runs an isolated profile.

To run **two** instances at once (multi-account, pairing), do not run `npm run dev`
twice: Vite is pinned to port 1420 with `strictPort`, so the second aborts with
`Port 1420 is already in use`. Start one dev server, then launch the other
instances straight from the binary it built — they reuse that dev server, because
`devUrl` is baked into the config at compile time.

```sh
# terminal 1 - instance A, and the only dev server
SHAREPASTE_DATA_DIR=/tmp/sp-a npm run dev

# terminal 2 - instance B, reusing A's dev server (add .exe on Windows)
SHAREPASTE_DATA_DIR=/tmp/sp-b src-tauri/target/debug/sharepaste-desktop
```

Only the first instance wins the global hotkey; the rest log
`register global shortcut failed: HotKey already registered` and carry on. Each
instance gets its own tray icon, appended in launch order, so the last icon is the
instance you started most recently.

## Build

```sh
npm run build
```

macOS output: `src-tauri/target/release/bundle/macos/sharepaste.app`
and a `.dmg` alongside.

Windows output: a standalone NSIS installer `.exe`.
For local development, `npm.cmd run tauri dev` and `cargo check --manifest-path src-tauri/Cargo.toml` suffice for iteration.

## Install (unsigned)

```sh
xattr -d com.apple.quarantine /Applications/sharepaste.app
open /Applications/sharepaste.app
```

## Windows smoke checklist

1. `npm.cmd --prefix ui run build` - Vite production build succeeds.
2. `cargo check --manifest-path src-tauri/Cargo.toml` - Rust/Tauri check succeeds.
3. `npm.cmd run tauri dev` - desktop shell opens.
4. Pair against a running server.
5. Copy text in another app and verify the entry appears in the popover.
6. Add the foreground app executable name to the deny-list, copy new text from
   that app, and verify the entry is skipped.
7. Copy an existing history item from the UI and verify it is not immediately
   re-captured as a new entry.

## macOS manual smoke checklist

1. `npm run dev` — tray icon appears, no Dock icon, no menu bar.
2. Right-click the tray icon (left-click toggles the popover) → **Pair device…**
   → **I have an invite token**, claim it against a local server, copy text in
   any app, see the entry appear in the popover.
3. Pair a second instance via the short code path:
   - Launch instance B from the built binary, not a second `npm run dev` — see
     the two-instance recipe under Dev workflow.
   - Instance A: tray → **Pair device…** → **I want to pair another device**.
   - Enter the displayed code in instance B → **Pair**.
   - Both must advance to the paired state. Instance A only does so if it
     receives the `pair-claimed` event, which is the ACL regression guarded by
     `acl-tests` and `capability_guard`.
   - Confirm both popovers see entries from each other.
4. Revoke instance A from the server CLI; instance A's tray flips red and the
   popover shows the **Re-pair this device** banner.
5. Toggle **Capture clipboard changes** off; copying does nothing. Toggle on,
   copy a 1Password password (concealed flag), and verify that nothing reaches the server.
6. Force-quit during an offline-pending burst, restart; verify the pending
   queue flushes once connectivity returns.

## Build tools

The root `Makefile` provides macOS-only convenience targets. On Windows or Linux, use npm directly:
`npm --prefix clients/desktop run build` or other scripts from `clients/desktop/package.json`.
