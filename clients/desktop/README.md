# sharepaste — macOS desktop client

Tauri 2 client for the sharepaste self-hosted clipboard sync server. Builds an
unsigned `.app` and `.dmg`. Spec lives at
`docs/superpowers/specs/2026-05-01-sharepaste-macos-design.md`.

## Prerequisites

- macOS 12+ (Monterey)
- Rust stable (`rustup` will pick up `rust-toolchain.toml`)
- Node 20+
- Xcode command-line tools (`xcode-select --install`)
- A running sharepaste server (see top-level repo README; for dev, `npx tsx src/index.ts serve`)

## Dev workflow

```sh
# from the project root
cd clients/desktop
npm install
npm --prefix ui install
npm run dev    # boots Vite at :1420 + opens the Tauri shell
```

`SHAREPASTE_DATA_DIR=/tmp/sp1 npm run dev` runs an isolated profile so you can
launch a second instance and test multi-account / pairing on one machine.

## Build

```sh
npm run build
```

Output: `src-tauri/target/release/bundle/macos/sharepaste.app`
and a `.dmg` alongside.

## Install (unsigned)

```sh
xattr -d com.apple.quarantine /Applications/sharepaste.app
open /Applications/sharepaste.app
```

## Manual smoke checklist

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
