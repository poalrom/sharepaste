# sharepaste - desktop client

Tauri 2 client for the sharepaste self-hosted clipboard sync server. macOS is
the feature-complete target. Windows currently supports compile and launch with
reduced clipboard scope: manual history copy works, but clipboard auto-capture
is still macOS-only.

Specs:

- `docs/superpowers/specs/2026-05-01-sharepaste-macos-design.md`
- `docs/superpowers/specs/2026-05-03-sharepaste-windows-compile-launch-design.md`

## Prerequisites

- Rust stable (`rustup` will pick up `rust-toolchain.toml`)
- Node 20+
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

`SHAREPASTE_DATA_DIR=/tmp/sp1 npm run dev` runs an isolated profile so you can
launch a second instance and test multi-account / pairing on one machine.

## Build

```sh
npm run build
```

macOS output: `src-tauri/target/release/bundle/macos/sharepaste.app`
and a `.dmg` alongside.

Windows output depends on the selected Tauri bundle target. For the first
Windows pass, `cargo check` and `npm.cmd run tauri dev` are the supported
compile/launch checks.

## Install (unsigned)

```sh
xattr -d com.apple.quarantine /Applications/sharepaste.app
open /Applications/sharepaste.app
```

## Windows smoke checklist

1. `npm.cmd --prefix ui run build` - Vite production build succeeds.
2. `cd src-tauri && cargo check` - Rust/Tauri check succeeds.
3. `npm.cmd run tauri dev` - desktop shell opens.
4. Open the popover and verify the app does not start clipboard auto-capture.
5. Pair against a running server and copy an existing history item from the UI.

## macOS manual smoke checklist

1. `npm run dev` — tray icon appears, no Dock icon, no menu bar.
2. Open the popover, click **Pair a device**, claim an invite against a local
   server, copy text in any app, see entry appear in the popover.
3. Pair a second instance via the short code path:
   - Run a second instance with `SHAREPASTE_DATA_DIR=/tmp/sp-b npm run dev`.
   - From instance A, choose **Pair a device → I want to pair another device**.
   - Paste the displayed code into instance B.
   - Confirm both popovers see entries from each other.
4. Revoke instance A from the server CLI; instance A's tray flips red and the
   popover shows the **Re-pair this device** banner.
5. Toggle **Capture clipboard changes** off; copying does nothing. Toggle on,
   copy a 1Password password (concealed flag), confirm a `capture-skipped`
   toast and that nothing reaches the server.
6. Force-quit during an offline-pending burst, restart; verify the pending
   queue flushes once connectivity returns.
