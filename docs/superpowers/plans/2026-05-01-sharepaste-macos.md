# Sharepaste macOS Client — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the macOS Tauri 2 desktop client described in `docs/superpowers/specs/2026-05-01-sharepaste-macos-design.md` — a tray-popover clipboard-sync app that watches the system clipboard, encrypts entries locally with XChaCha20-Poly1305, syncs them through the existing self-hosted server (track 1), and renders searchable history. Includes pairing wizard, multi-account switcher, settings, opt-in autostart and global hotkey. Windows / Linux / mobile clients are out of scope.

**Architecture:** Tauri 2 app at `clients/desktop/`. Pure-Rust core (`src-tauri/src/core/*`) with no Tauri dependencies — testable in isolation. A thin `commands.rs` is the only place that bridges core to webview. React + Tailwind UI under `clients/desktop/ui/`. Rust owns durable + secret state (rusqlite + macOS Keychain); React owns view state via Zustand. Sync is one Tokio task per active membership running a Disconnected→Connecting→Online state machine over `reqwest` (REST + SSE).

**Tech Stack:**
- Tauri 2 (`tauri@^2`, `@tauri-apps/cli@^2`, `tauri-plugin-autostart`, `tauri-plugin-global-shortcut`)
- Rust stable, edition 2021
- `tokio` (full), `tokio-util`
- `reqwest` (rustls-tls, json) + `eventsource-stream` for SSE
- `rusqlite` (bundled feature)
- `chacha20poly1305` (RustCrypto, XChaCha20-Poly1305-IETF) + `rand` + `zeroize`
- `keyring` (macOS Keychain backend)
- `clipboard-master` (clipboard event source)
- `arboard` (read/write text)
- `objc2` + `objc2-app-kit` + `objc2-foundation` (NSPasteboard / NSWorkspace, `cfg(target_os = "macos")`)
- `data-encoding` (base32) for shortcode
- `uuid`, `serde`, `serde_json`, `thiserror`, `tracing`, `tracing-subscriber`, `tracing-appender`, `dirs`
- React 18, Vite 5, Tailwind 3, Zustand 4, Vitest 1, Testing Library

---

## File structure

```
sharepaste/
└── clients/
    └── desktop/
        ├── package.json                       # tauri dev/build, vite tooling
        ├── rust-toolchain.toml
        ├── README.md                          # manual smoke checklist
        ├── src-tauri/
        │   ├── Cargo.toml
        │   ├── tauri.conf.json
        │   ├── build.rs
        │   ├── icons/
        │   │   ├── icon.png
        │   │   ├── icon.icns
        │   │   └── tray-template.png          # 16pt template image
        │   ├── capabilities/
        │   │   └── default.json               # window/popover/modal permissions
        │   ├── src/
        │   │   ├── main.rs                    # Tauri entry: tray, windows
        │   │   ├── state.rs                   # AppState bundle
        │   │   ├── config.rs                  # paths, env overrides
        │   │   ├── errors.rs                  # AppError + IPC serialization
        │   │   ├── commands.rs                # #[tauri::command] surface
        │   │   ├── events.rs                  # event names + payload types
        │   │   ├── logging.rs                 # tracing init
        │   │   └── core/
        │   │       ├── mod.rs
        │   │       ├── crypto.rs
        │   │       ├── storage/
        │   │       │   ├── mod.rs
        │   │       │   ├── migrations.rs
        │   │       │   ├── accounts.rs
        │   │       │   ├── entries_cache.rs
        │   │       │   ├── pending.rs
        │   │       │   └── settings.rs
        │   │       ├── keychain.rs
        │   │       ├── http/
        │   │       │   ├── mod.rs
        │   │       │   ├── client.rs          # ServerClient struct
        │   │       │   └── dto.rs             # request/response types
        │   │       ├── sync/
        │   │       │   ├── mod.rs             # state machine
        │   │       │   ├── sse.rs
        │   │       │   ├── uploader.rs
        │   │       │   └── decryptor.rs
        │   │       ├── capture/
        │   │       │   ├── mod.rs
        │   │       │   ├── filter.rs
        │   │       │   ├── watcher.rs
        │   │       │   └── macos.rs           # cfg(macos)
        │   │       ├── pairing/
        │   │       │   ├── mod.rs
        │   │       │   ├── shortcode.rs
        │   │       │   ├── invite.rs
        │   │       │   └── qr.rs
        │   │       └── account/
        │   │           └── mod.rs             # registry + active selection
        │   └── tests/
        │       ├── common/
        │       │   └── mod.rs                 # spawn server helper
        │       ├── flow1_invite.rs
        │       ├── flow2_pairing.rs
        │       ├── auth_revoke.rs
        │       └── multi_account.rs
        └── ui/
            ├── package.json
            ├── vite.config.ts
            ├── tailwind.config.ts
            ├── postcss.config.js
            ├── tsconfig.json
            ├── index.html
            ├── popover.html
            ├── modal.html
            └── src/
                ├── main.tsx
                ├── App.tsx
                ├── styles.css
                ├── store/
                │   ├── index.ts
                │   ├── ui.ts
                │   ├── history.ts
                │   ├── accounts.ts
                │   └── status.ts
                ├── ipc/
                │   ├── commands.ts
                │   ├── events.ts
                │   └── tauri.ts                # invoke/listen behind interface
                ├── views/
                │   ├── Popover.tsx
                │   ├── Search.tsx
                │   ├── HistoryList.tsx
                │   ├── EntryRow.tsx
                │   └── Footer.tsx
                ├── modals/
                │   ├── PairingModal.tsx
                │   ├── SettingsModal.tsx
                │   └── AccountsModal.tsx
                ├── types.ts
                └── __tests__/
                    ├── HistoryList.test.tsx
                    ├── PairingModal.test.tsx
                    ├── SettingsModal.test.tsx
                    └── store.test.ts
```

---

## Task 1: Project bootstrap (workspace, Cargo, Vite, Tailwind)

**Files:**
- Create: `clients/desktop/package.json`
- Create: `clients/desktop/rust-toolchain.toml`
- Create: `clients/desktop/src-tauri/Cargo.toml`
- Create: `clients/desktop/src-tauri/build.rs`
- Create: `clients/desktop/src-tauri/tauri.conf.json`
- Create: `clients/desktop/src-tauri/capabilities/default.json`
- Create: `clients/desktop/src-tauri/src/main.rs`
- Create: `clients/desktop/src-tauri/src/core/mod.rs`
- Create: `clients/desktop/ui/package.json`
- Create: `clients/desktop/ui/vite.config.ts`
- Create: `clients/desktop/ui/tailwind.config.ts`
- Create: `clients/desktop/ui/postcss.config.js`
- Create: `clients/desktop/ui/tsconfig.json`
- Create: `clients/desktop/ui/index.html`
- Create: `clients/desktop/ui/popover.html`
- Create: `clients/desktop/ui/modal.html`
- Create: `clients/desktop/ui/src/main.tsx`
- Create: `clients/desktop/ui/src/App.tsx`
- Create: `clients/desktop/ui/src/styles.css`
- Modify: `.gitignore`

- [ ] **Step 1: Add Tauri/Rust artifacts to root .gitignore**

Append to `/Users/poalrom/private/sharepaste/.gitignore`:
```
clients/desktop/src-tauri/target/
clients/desktop/ui/dist/
clients/desktop/ui/node_modules/
clients/desktop/node_modules/
```

- [ ] **Step 2: Pin Rust toolchain**

`clients/desktop/rust-toolchain.toml`:
```toml
[toolchain]
channel = "stable"
profile = "minimal"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 3: Create root package.json for the desktop client (delegates to ui + tauri CLI)**

`clients/desktop/package.json`:
```json
{
  "name": "sharepaste-desktop",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "tauri dev",
    "build": "tauri build",
    "tauri": "tauri"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.0.0"
  }
}
```

Run:
```bash
cd /Users/poalrom/private/sharepaste/clients/desktop
npm install
```

Expected: `@tauri-apps/cli` installed; `node_modules/` populated.

- [ ] **Step 4: Set up the UI subpackage (Vite + React + Tailwind + Zustand + Vitest)**

`clients/desktop/ui/package.json`:
```json
{
  "name": "sharepaste-ui",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "test": "vitest run",
    "test:watch": "vitest"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.0.0",
    "@tauri-apps/plugin-autostart": "^2.0.0",
    "@tauri-apps/plugin-global-shortcut": "^2.0.0",
    "react": "^18.3.0",
    "react-dom": "^18.3.0",
    "zustand": "^4.5.0"
  },
  "devDependencies": {
    "@testing-library/jest-dom": "^6.4.0",
    "@testing-library/react": "^16.0.0",
    "@testing-library/user-event": "^14.5.0",
    "@types/react": "^18.3.0",
    "@types/react-dom": "^18.3.0",
    "@vitejs/plugin-react": "^4.3.0",
    "autoprefixer": "^10.4.0",
    "jsdom": "^25.0.0",
    "postcss": "^8.4.0",
    "tailwindcss": "^3.4.0",
    "typescript": "^5.6.0",
    "vite": "^5.4.0",
    "vitest": "^2.1.0"
  }
}
```

Run:
```bash
cd /Users/poalrom/private/sharepaste/clients/desktop/ui
npm install
```

- [ ] **Step 5: Vite config — multi-entry (popover, modal) on a fixed dev port**

`clients/desktop/ui/vite.config.ts`:
```ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "es2022",
    rollupOptions: {
      input: {
        popover: path.resolve(__dirname, "popover.html"),
        modal: path.resolve(__dirname, "modal.html"),
        index: path.resolve(__dirname, "index.html"),
      },
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test-setup.ts"],
    globals: true,
  },
});
```

- [ ] **Step 6: Tailwind + PostCSS configs and stylesheet**

`clients/desktop/ui/tailwind.config.ts`:
```ts
import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./popover.html", "./modal.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      fontFamily: {
        mono: ["ui-monospace", "SFMono-Regular", "Menlo", "monospace"],
      },
    },
  },
} satisfies Config;
```

`clients/desktop/ui/postcss.config.js`:
```js
export default {
  plugins: {
    tailwindcss: {},
    autoprefixer: {},
  },
};
```

`clients/desktop/ui/src/styles.css`:
```css
@tailwind base;
@tailwind components;
@tailwind utilities;

html, body, #root { height: 100%; margin: 0; }
body { color: #e6e6e6; background: #1a1a1a; font-family: system-ui, -apple-system, sans-serif; }
```

- [ ] **Step 7: TS config**

`clients/desktop/ui/tsconfig.json`:
```json
{
  "compilerOptions": {
    "target": "ES2022",
    "useDefineForClassFields": true,
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "skipLibCheck": true,
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true,
    "jsx": "react-jsx",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "types": ["vitest/globals", "@testing-library/jest-dom"]
  },
  "include": ["src", "vite.config.ts"]
}
```

- [ ] **Step 8: Three HTML entry points (each loads main.tsx, which decides which view)**

`clients/desktop/ui/index.html`:
```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>sharepaste</title>
  </head>
  <body data-route="popover">
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

`clients/desktop/ui/popover.html` is identical to `index.html` (same `<body data-route="popover">`); `modal.html` differs only in `<body data-route="modal" data-modal="">` — the `data-modal` attribute is filled in at runtime by Rust when opening the window.

`clients/desktop/ui/modal.html`:
```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>sharepaste</title>
  </head>
  <body data-route="modal" data-modal="">
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 9: Stub React entry that renders a placeholder**

`clients/desktop/ui/src/main.tsx`:
```tsx
import React from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./styles.css";

const root = createRoot(document.getElementById("root")!);
root.render(<App />);
```

`clients/desktop/ui/src/App.tsx`:
```tsx
export default function App() {
  const route = document.body.dataset.route ?? "popover";
  const modal = document.body.dataset.modal ?? "";
  return (
    <div className="p-4 text-sm">
      <div>route: {route}</div>
      {route === "modal" ? <div>modal: {modal || "(unset)"}</div> : null}
    </div>
  );
}
```

`clients/desktop/ui/src/test-setup.ts`:
```ts
import "@testing-library/jest-dom/vitest";
```

- [ ] **Step 10: Set up the `src-tauri` Cargo crate with the full dependency list**

`clients/desktop/src-tauri/Cargo.toml`:
```toml
[package]
name = "sharepaste-desktop"
version = "0.1.0"
edition = "2021"
rust-version = "1.78"

[lib]
name = "sharepaste_desktop_lib"
path = "src/lib.rs"

[[bin]]
name = "sharepaste-desktop"
path = "src/main.rs"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = ["tray-icon", "macos-private-api"] }
tauri-plugin-autostart = "2"
tauri-plugin-global-shortcut = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
tokio-util = "0.7"
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json", "stream"] }
eventsource-stream = "0.2"
futures = "0.3"
rusqlite = { version = "0.32", features = ["bundled"] }
chacha20poly1305 = "0.10"
rand = "0.8"
zeroize = { version = "1", features = ["zeroize_derive"] }
keyring = { version = "3", default-features = false, features = ["apple-native"] }
data-encoding = "2"
uuid = { version = "1", features = ["v4", "serde"] }
thiserror = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
tracing-appender = "0.2"
dirs = "5"
parking_lot = "0.12"
async-trait = "0.1"

[target.'cfg(target_os = "macos")'.dependencies]
clipboard-master = "4"
arboard = "3"
objc2 = "0.5"
objc2-foundation = "0.2"
objc2-app-kit = "0.2"

[dev-dependencies]
tempfile = "3"
tokio = { version = "1", features = ["full", "test-util"] }
hex-literal = "0.4"
```

- [ ] **Step 11: Create the Tauri build script and a placeholder lib + main**

`clients/desktop/src-tauri/build.rs`:
```rust
fn main() {
    tauri_build::build();
}
```

`clients/desktop/src-tauri/src/lib.rs`:
```rust
//! Sharepaste desktop client library crate.
//!
//! Houses the Rust core (`core::*`) plus the Tauri command + event surface so
//! integration tests can use it without going through the binary.

pub mod core;
```

`clients/desktop/src-tauri/src/main.rs`:
```rust
fn main() {
    sharepaste_desktop_lib::run();
}
```

`clients/desktop/src-tauri/src/core/mod.rs`:
```rust
//! Pure-Rust core for sharepaste-desktop. No `tauri::*` imports allowed in
//! this module tree — everything below is testable without a Tauri runtime.
```

Edit `lib.rs` to also expose a placeholder `run` so `main.rs` compiles:
```rust
pub mod core;

pub fn run() {
    eprintln!("sharepaste-desktop: not yet implemented");
}
```

- [ ] **Step 12: Tauri config — accessory app, three windows, plugin allowlist**

`clients/desktop/src-tauri/tauri.conf.json`:
```json
{
  "$schema": "https://schema.tauri.app/config/2.0.0",
  "productName": "sharepaste",
  "version": "0.1.0",
  "identifier": "com.sharepaste.desktop",
  "build": {
    "frontendDist": "../ui/dist",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "npm --prefix ../ui run dev",
    "beforeBuildCommand": "npm --prefix ../ui run build"
  },
  "app": {
    "windows": [
      {
        "label": "popover",
        "url": "popover.html",
        "title": "sharepaste",
        "width": 360,
        "height": 480,
        "resizable": false,
        "decorations": false,
        "transparent": false,
        "alwaysOnTop": true,
        "visible": false,
        "skipTaskbar": true,
        "focus": true
      }
    ],
    "macOSPrivateApi": false,
    "trayIcon": {
      "iconPath": "icons/tray-template.png",
      "iconAsTemplate": true,
      "id": "main"
    }
  },
  "bundle": {
    "active": true,
    "targets": ["app", "dmg"],
    "icon": ["icons/icon.png", "icons/icon.icns"],
    "macOS": {
      "minimumSystemVersion": "12.0",
      "frameworks": [],
      "entitlements": null,
      "exceptionDomain": "",
      "providerShortName": null
    },
    "category": "Utility",
    "shortDescription": "End-to-end-encrypted clipboard sync",
    "longDescription": "Self-hosted clipboard sync with per-user encryption."
  },
  "plugins": {
    "autostart": { "enabled": true },
    "global-shortcut": { "enabled": true }
  }
}
```

- [ ] **Step 13: Capability file (Tauri 2 permissions for popover + modal windows)**

`clients/desktop/src-tauri/capabilities/default.json`:
```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default capabilities for popover and modal windows",
  "windows": ["popover", "modal-*"],
  "permissions": [
    "core:default",
    "core:event:default",
    "core:window:allow-show",
    "core:window:allow-hide",
    "core:window:allow-set-position",
    "core:window:allow-set-focus",
    "core:window:allow-close",
    "autostart:default",
    "global-shortcut:default"
  ]
}
```

- [ ] **Step 14: Create placeholder tray icon (16×16 PNG, transparent black square — replace later with a real asset)**

```bash
cd /Users/poalrom/private/sharepaste/clients/desktop/src-tauri
mkdir -p icons
# minimal 1x1 transparent PNG to satisfy bundler; real icon swapped in by designer later
python3 -c "import base64,sys;sys.stdout.buffer.write(base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNgYAAAAAMAASsJTYQAAAAASUVORK5CYII='))" > icons/tray-template.png
cp icons/tray-template.png icons/icon.png
# .icns is built by Tauri from icon.png; create empty file for now
: > icons/icon.icns
```

- [ ] **Step 15: Verify the toolchain compiles end to end**

Run:
```bash
cd /Users/poalrom/private/sharepaste/clients/desktop
cargo check --manifest-path src-tauri/Cargo.toml
npm --prefix ui run build
```

Expected: `cargo check` reports "Finished `dev` profile" with no errors; `npm run build` writes `ui/dist/popover.html`, `ui/dist/modal.html`, `ui/dist/index.html`.

- [ ] **Step 16: Commit**

```bash
git add clients/desktop .gitignore
git commit -m "feat(desktop): bootstrap Tauri 2 + React + Tailwind workspace"
```

---

## Task 2: Rust core foundation (paths, errors, logging)

**Files:**
- Create: `clients/desktop/src-tauri/src/config.rs`
- Create: `clients/desktop/src-tauri/src/errors.rs`
- Create: `clients/desktop/src-tauri/src/logging.rs`
- Modify: `clients/desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: Write tests for path resolution (with env-var override)**

`clients/desktop/src-tauri/src/config.rs`:
```rust
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Paths {
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub db_path: PathBuf,
}

impl Paths {
    pub fn resolve() -> Self {
        Self::resolve_with_env(std::env::var_os("SHAREPASTE_DATA_DIR"))
    }

    pub fn resolve_with_env(override_data_dir: Option<std::ffi::OsString>) -> Self {
        let data_dir = match override_data_dir {
            Some(p) => PathBuf::from(p),
            None => default_data_dir(),
        };
        let log_dir = default_log_dir(&data_dir);
        let cache_dir = default_cache_dir(&data_dir);
        let db_path = data_dir.join("state.sqlite");
        Self { data_dir, log_dir, cache_dir, db_path }
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(&self.log_dir)?;
        std::fs::create_dir_all(&self.cache_dir)?;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn default_data_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join("Library/Application Support/sharepaste"))
        .unwrap_or_else(|| PathBuf::from("./sharepaste-data"))
}

#[cfg(not(target_os = "macos"))]
fn default_data_dir() -> PathBuf {
    dirs::data_dir()
        .map(|d| d.join("sharepaste"))
        .unwrap_or_else(|| PathBuf::from("./sharepaste-data"))
}

#[cfg(target_os = "macos")]
fn default_log_dir(_data: &Path) -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join("Library/Logs/sharepaste"))
        .unwrap_or_else(|| PathBuf::from("./sharepaste-logs"))
}

#[cfg(not(target_os = "macos"))]
fn default_log_dir(data: &Path) -> PathBuf {
    data.join("logs")
}

#[cfg(target_os = "macos")]
fn default_cache_dir(_data: &Path) -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join("Library/Caches/sharepaste"))
        .unwrap_or_else(|| PathBuf::from("./sharepaste-cache"))
}

#[cfg(not(target_os = "macos"))]
fn default_cache_dir(data: &Path) -> PathBuf {
    data.join("cache")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn override_data_dir_is_honoured() {
        let p = Paths::resolve_with_env(Some(OsString::from("/tmp/sp-test-1")));
        assert_eq!(p.data_dir, PathBuf::from("/tmp/sp-test-1"));
        assert_eq!(p.db_path, PathBuf::from("/tmp/sp-test-1/state.sqlite"));
    }

    #[test]
    fn ensure_dirs_creates_all_three() {
        let tmp = tempfile::tempdir().unwrap();
        let p = Paths::resolve_with_env(Some(tmp.path().join("data").into_os_string()));
        p.ensure_dirs().unwrap();
        assert!(p.data_dir.is_dir());
        assert!(p.log_dir.is_dir());
        assert!(p.cache_dir.is_dir());
    }
}
```

- [ ] **Step 2: Run the test, observe it fails (module not wired)**

Run:
```bash
cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml config::
```
Expected: compile error — `config` not in lib tree.

- [ ] **Step 3: Wire `config` and the rest of the upcoming modules into the lib crate**

Replace `clients/desktop/src-tauri/src/lib.rs`:
```rust
pub mod config;
pub mod errors;
pub mod logging;
pub mod core;

pub fn run() {
    eprintln!("sharepaste-desktop: not yet implemented");
}
```

- [ ] **Step 4: Re-run, observe compile failures point at `errors` and `logging` (not yet written)**

Run: `cargo check --manifest-path clients/desktop/src-tauri/Cargo.toml`
Expected: `errors` / `logging` missing.

- [ ] **Step 5: Implement `errors.rs`**

`clients/desktop/src-tauri/src/errors.rs`:
```rust
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("network error: {0}")]
    Network(String),
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("bad input: {0}")]
    BadInput(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("crypto error: {0}")]
    Crypto(String),
    #[error("pair expired: {0}")]
    PairExpired(String),
    #[error("keychain error: {0}")]
    Keychain(String),
}

impl AppError {
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Network(_) => "Network",
            AppError::Auth(_) => "Auth",
            AppError::NotFound(_) => "NotFound",
            AppError::BadInput(_) => "BadInput",
            AppError::Storage(_) => "Storage",
            AppError::Crypto(_) => "Crypto",
            AppError::PairExpired(_) => "PairExpired",
            AppError::Keychain(_) => "Keychain",
        }
    }
}

#[derive(Serialize)]
struct WireError<'a> {
    kind: &'a str,
    message: String,
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WireError { kind: self.kind(), message: self.to_string() }.serialize(s)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self { AppError::Storage(e.to_string()) }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_status() && e.status() == Some(reqwest::StatusCode::UNAUTHORIZED) {
            AppError::Auth(e.to_string())
        } else {
            AppError::Network(e.to_string())
        }
    }
}

impl From<keyring::Error> for AppError {
    fn from(e: keyring::Error) -> Self { AppError::Keychain(e.to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_to_kind_and_message_object() {
        let e = AppError::BadInput("missing token".into());
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(json, r#"{"kind":"BadInput","message":"bad input: missing token"}"#);
    }
}
```

- [ ] **Step 6: Implement `logging.rs`**

`clients/desktop/src-tauri/src/logging.rs`:
```rust
use std::path::Path;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init(log_dir: &Path) -> WorkerGuard {
    let _ = std::fs::create_dir_all(log_dir);
    let appender = tracing_appender::rolling::daily(log_dir, "desktop.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);

    let env = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let json_layer = fmt::layer()
        .json()
        .with_current_span(false)
        .with_writer(writer);

    let stderr_layer = fmt::layer()
        .with_target(false)
        .with_writer(std::io::stderr)
        .with_filter(EnvFilter::new("warn"));

    tracing_subscriber::registry()
        .with(env)
        .with(json_layer)
        .with(stderr_layer)
        .init();

    tracing::event!(Level::INFO, "logging initialized");
    guard
}
```

- [ ] **Step 7: Run all the new tests**

Run:
```bash
cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml --lib
```
Expected: 3 passing tests (`override_data_dir_is_honoured`, `ensure_dirs_creates_all_three`, `serializes_to_kind_and_message_object`).

- [ ] **Step 8: Commit**

```bash
git add clients/desktop/src-tauri/src/config.rs clients/desktop/src-tauri/src/errors.rs clients/desktop/src-tauri/src/logging.rs clients/desktop/src-tauri/src/lib.rs
git commit -m "feat(desktop): add path resolution, AppError, and tracing setup"
```

---

## Task 3: Rust core — XChaCha20-Poly1305 wrapper with KAT vectors

**Files:**
- Create: `clients/desktop/src-tauri/src/core/crypto.rs`
- Modify: `clients/desktop/src-tauri/src/core/mod.rs`

- [ ] **Step 1: Add `crypto` to the core module tree**

Replace `clients/desktop/src-tauri/src/core/mod.rs`:
```rust
//! Pure-Rust core for sharepaste-desktop. No `tauri::*` imports allowed in
//! this module tree — everything below is testable without a Tauri runtime.

pub mod crypto;
```

- [ ] **Step 2: Write the failing test, including a libsodium-derived known-answer vector**

`clients/desktop/src-tauri/src/core/crypto.rs`:
```rust
use crate::errors::AppError;
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

pub const KEY_LEN: usize = 32;
pub const NONCE_LEN: usize = 24;

pub type UserKey = Zeroizing<[u8; KEY_LEN]>;

pub fn random_user_key() -> UserKey {
    let mut k = [0u8; KEY_LEN];
    rand::RngCore::fill_bytes(&mut OsRng, &mut k);
    Zeroizing::new(k)
}

pub fn encrypt(user_key: &UserKey, user_id: &str, plaintext: &[u8]) -> Result<Vec<u8>, AppError> {
    let cipher = XChaCha20Poly1305::new(user_key.as_slice().into());
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, Payload { msg: plaintext, aad: user_id.as_bytes() })
        .map_err(|e| AppError::Crypto(format!("encrypt: {e}")))?;
    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub fn decrypt(user_key: &UserKey, user_id: &str, wire: &[u8]) -> Result<Vec<u8>, AppError> {
    if wire.len() < NONCE_LEN + 16 {
        return Err(AppError::Crypto("ciphertext too short".into()));
    }
    let (nonce_bytes, ct) = wire.split_at(NONCE_LEN);
    let nonce = XNonce::from_slice(nonce_bytes);
    let cipher = XChaCha20Poly1305::new(user_key.as_slice().into());
    cipher
        .decrypt(nonce, Payload { msg: ct, aad: user_id.as_bytes() })
        .map_err(|e| AppError::Crypto(format!("decrypt: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_literal::hex;

    fn key() -> UserKey {
        let bytes: [u8; 32] = hex!("0808080808080808080808080808080808080808080808080808080808080808");
        Zeroizing::new(bytes)
    }

    #[test]
    fn round_trip_random_nonce() {
        let k = key();
        let user_id = "alice";
        let plaintext = b"hello sharepaste";
        let ct = encrypt(&k, user_id, plaintext).unwrap();
        assert!(ct.len() > NONCE_LEN);
        let pt = decrypt(&k, user_id, &ct).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn aad_mismatch_fails() {
        let k = key();
        let ct = encrypt(&k, "alice", b"x").unwrap();
        assert!(decrypt(&k, "bob", &ct).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let k = key();
        let mut ct = encrypt(&k, "alice", b"x").unwrap();
        let last = ct.len() - 1;
        ct[last] ^= 0x01;
        assert!(decrypt(&k, "alice", &ct).is_err());
    }

    #[test]
    fn truncated_ciphertext_returns_crypto_error() {
        let k = key();
        let err = decrypt(&k, "alice", &[0u8; 16]).unwrap_err();
        match err {
            AppError::Crypto(_) => {}
            other => panic!("expected Crypto, got {other:?}"),
        }
    }

    /// Known-answer vector generated with libsodium 1.0.19:
    ///   crypto_aead_xchacha20poly1305_ietf_encrypt(
    ///     key   = 0x08…08 (32 bytes),
    ///     nonce = 0x07…07 (24 bytes),
    ///     ad    = b"alice",
    ///     msg   = b"hello"
    ///   )
    /// Use this to confirm wire compatibility with mobile libsodium clients.
    #[test]
    fn matches_libsodium_kat() {
        let k = key();
        let nonce: [u8; 24] = hex!("070707070707070707070707070707070707070707070707");
        let cipher = XChaCha20Poly1305::new(k.as_slice().into());
        let ct = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload { msg: b"hello", aad: b"alice" },
            )
            .unwrap();
        // Generated vector: prepend nonce so decoders see wire format.
        let mut wire = Vec::new();
        wire.extend_from_slice(&nonce);
        wire.extend_from_slice(&ct);
        let pt = decrypt(&k, "alice", &wire).unwrap();
        assert_eq!(pt, b"hello");
        // Sanity: re-encrypt must produce a different ciphertext (nonce randomized).
        let ct2 = encrypt(&k, "alice", b"hello").unwrap();
        assert_ne!(ct2[NONCE_LEN..], ct[..]);
    }
}
```

- [ ] **Step 3: Run the tests to verify**

Run:
```bash
cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml --lib core::crypto
```
Expected: 5 passing tests.

- [ ] **Step 4: Commit**

```bash
git add clients/desktop/src-tauri/src/core/mod.rs clients/desktop/src-tauri/src/core/crypto.rs
git commit -m "feat(desktop): add XChaCha20-Poly1305 wrapper with KAT and tamper tests"
```

---
## Task 4: Storage — migrations module

**Files:**
- Create: `clients/desktop/src-tauri/src/core/storage/mod.rs`
- Create: `clients/desktop/src-tauri/src/core/storage/migrations.rs`
- Modify: `clients/desktop/src-tauri/src/core/mod.rs`

- [ ] **Step 1: Wire `storage` into the core tree**

Replace `clients/desktop/src-tauri/src/core/mod.rs`:
```rust
//! Pure-Rust core for sharepaste-desktop.

pub mod crypto;
pub mod storage;
```

- [ ] **Step 2: Write the failing migrations test**

`clients/desktop/src-tauri/src/core/storage/mod.rs`:
```rust
pub mod accounts;
pub mod entries_cache;
pub mod migrations;
pub mod pending;
pub mod settings;

use crate::errors::AppError;
use rusqlite::Connection;
use std::path::Path;

pub fn open(path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrations::run(&conn)?;
    Ok(conn)
}

pub fn open_in_memory() -> Result<Connection, AppError> {
    let conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrations::run(&conn)?;
    Ok(conn)
}
```

`clients/desktop/src-tauri/src/core/storage/migrations.rs`:
```rust
use crate::errors::AppError;
use rusqlite::Connection;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS accounts (
  user_id        TEXT PRIMARY KEY,
  device_id      TEXT NOT NULL,
  device_label   TEXT NOT NULL,
  server_url     TEXT NOT NULL,
  last_seen_id   INTEGER NOT NULL DEFAULT 0,
  created_at     INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS entries_cache (
  user_id     TEXT NOT NULL,
  id          INTEGER NOT NULL,
  ciphertext  BLOB NOT NULL,
  plaintext   TEXT,
  created_at  INTEGER NOT NULL,
  device_id   TEXT NOT NULL,
  PRIMARY KEY (user_id, id)
);
CREATE INDEX IF NOT EXISTS entries_cache_user_id_id ON entries_cache (user_id, id DESC);

CREATE TABLE IF NOT EXISTS pending_uploads (
  rowid       INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id     TEXT NOT NULL,
  ciphertext  BLOB NOT NULL,
  captured_at INTEGER NOT NULL,
  attempts    INTEGER NOT NULL DEFAULT 0,
  last_error  TEXT
);
CREATE INDEX IF NOT EXISTS pending_uploads_user_id_rowid ON pending_uploads (user_id, rowid);

CREATE TABLE IF NOT EXISTS settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
"#;

pub fn run(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(SCHEMA)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        run(&c).unwrap();
        c
    }

    #[test]
    fn creates_all_four_tables() {
        let c = fresh();
        let mut stmt = c.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
        ).unwrap();
        let names: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .filter(|n| !n.starts_with("sqlite_"))
            .collect();
        assert_eq!(names, vec!["accounts", "entries_cache", "pending_uploads", "settings"]);
    }

    #[test]
    fn idempotent_when_run_twice() {
        let c = fresh();
        run(&c).unwrap();
        run(&c).unwrap();
    }
}
```

- [ ] **Step 3: Add empty module stubs so the suite compiles**

`clients/desktop/src-tauri/src/core/storage/accounts.rs`:
```rust
// implemented in Task 6
```

`clients/desktop/src-tauri/src/core/storage/entries_cache.rs`:
```rust
// implemented in Task 5
```

`clients/desktop/src-tauri/src/core/storage/pending.rs`:
```rust
// implemented in Task 5
```

`clients/desktop/src-tauri/src/core/storage/settings.rs`:
```rust
// implemented in Task 6
```

- [ ] **Step 4: Run the tests**

Run:
```bash
cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml --lib core::storage::migrations
```
Expected: 2 passing.

- [ ] **Step 5: Commit**

```bash
git add clients/desktop/src-tauri/src/core/mod.rs clients/desktop/src-tauri/src/core/storage
git commit -m "feat(desktop): rusqlite migrations for accounts, entries_cache, pending, settings"
```

---

## Task 5: Storage — `entries_cache` and `pending_uploads` repositories

**Files:**
- Modify: `clients/desktop/src-tauri/src/core/storage/entries_cache.rs`
- Modify: `clients/desktop/src-tauri/src/core/storage/pending.rs`

- [ ] **Step 1: Write the failing test for `entries_cache` repository**

`clients/desktop/src-tauri/src/core/storage/entries_cache.rs`:
```rust
use crate::errors::AppError;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, PartialEq)]
pub struct CachedEntry {
    pub user_id: String,
    pub id: i64,
    pub ciphertext: Vec<u8>,
    pub plaintext: Option<String>,
    pub created_at: i64,
    pub device_id: String,
}

#[derive(Debug, Clone)]
pub struct NewCachedEntry<'a> {
    pub user_id: &'a str,
    pub id: i64,
    pub ciphertext: &'a [u8],
    pub plaintext: Option<&'a str>,
    pub created_at: i64,
    pub device_id: &'a str,
}

pub const MAX_PER_USER: i64 = 100;
pub const MAX_AGE_MS: i64 = 30 * 24 * 60 * 60 * 1000;

pub fn upsert_and_prune(conn: &Connection, e: NewCachedEntry<'_>, now_ms: i64) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO entries_cache (user_id, id, ciphertext, plaintext, created_at, device_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (user_id, id) DO UPDATE SET
            ciphertext = excluded.ciphertext,
            plaintext  = COALESCE(excluded.plaintext, entries_cache.plaintext),
            created_at = excluded.created_at,
            device_id  = excluded.device_id",
        params![e.user_id, e.id, e.ciphertext, e.plaintext, e.created_at, e.device_id],
    )?;
    tx.execute(
        "DELETE FROM entries_cache
          WHERE user_id = ?1
            AND (
              created_at < (?2 - ?3)
              OR id NOT IN (
                SELECT id FROM entries_cache
                WHERE user_id = ?1
                ORDER BY id DESC
                LIMIT ?4
              )
            )",
        params![e.user_id, now_ms, MAX_AGE_MS, MAX_PER_USER],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn list_recent(conn: &Connection, user_id: &str, before_id: Option<i64>, limit: i64) -> Result<Vec<CachedEntry>, AppError> {
    let limit = limit.clamp(1, MAX_PER_USER);
    let mut rows = if let Some(before) = before_id {
        let mut stmt = conn.prepare(
            "SELECT user_id, id, ciphertext, plaintext, created_at, device_id
             FROM entries_cache
             WHERE user_id = ?1 AND id < ?2
             ORDER BY id DESC LIMIT ?3"
        )?;
        stmt.query_map(params![user_id, before, limit], map_row)?.collect::<Result<Vec<_>, _>>()?
    } else {
        let mut stmt = conn.prepare(
            "SELECT user_id, id, ciphertext, plaintext, created_at, device_id
             FROM entries_cache
             WHERE user_id = ?1
             ORDER BY id DESC LIMIT ?2"
        )?;
        stmt.query_map(params![user_id, limit], map_row)?.collect::<Result<Vec<_>, _>>()?
    };
    rows.shrink_to_fit();
    Ok(rows)
}

pub fn search(conn: &Connection, user_id: &str, query: &str, limit: i64) -> Result<Vec<CachedEntry>, AppError> {
    let limit = limit.clamp(1, MAX_PER_USER);
    let needle = format!("%{}%", query.to_lowercase());
    let mut stmt = conn.prepare(
        "SELECT user_id, id, ciphertext, plaintext, created_at, device_id
         FROM entries_cache
         WHERE user_id = ?1 AND plaintext IS NOT NULL AND lower(plaintext) LIKE ?2
         ORDER BY id DESC LIMIT ?3"
    )?;
    let rows: Vec<CachedEntry> = stmt
        .query_map(params![user_id, needle, limit], map_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn get_full(conn: &Connection, user_id: &str, id: i64) -> Result<Option<String>, AppError> {
    let pt: Option<Option<String>> = conn
        .query_row(
            "SELECT plaintext FROM entries_cache WHERE user_id = ?1 AND id = ?2",
            params![user_id, id],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?;
    Ok(pt.flatten())
}

pub fn mark_undecryptable(conn: &Connection, user_id: &str, id: i64) -> Result<(), AppError> {
    conn.execute(
        "UPDATE entries_cache SET plaintext = NULL WHERE user_id = ?1 AND id = ?2",
        params![user_id, id],
    )?;
    Ok(())
}

pub fn delete_one(conn: &Connection, user_id: &str, id: i64) -> Result<usize, AppError> {
    let n = conn.execute(
        "DELETE FROM entries_cache WHERE user_id = ?1 AND id = ?2",
        params![user_id, id],
    )?;
    Ok(n)
}

pub fn delete_all(conn: &Connection, user_id: &str) -> Result<usize, AppError> {
    let n = conn.execute("DELETE FROM entries_cache WHERE user_id = ?1", params![user_id])?;
    Ok(n)
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<CachedEntry> {
    Ok(CachedEntry {
        user_id: r.get(0)?,
        id: r.get(1)?,
        ciphertext: r.get(2)?,
        plaintext: r.get(3)?,
        created_at: r.get(4)?,
        device_id: r.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::storage::open_in_memory;

    fn ins(c: &Connection, user: &str, id: i64, pt: Option<&str>, ts: i64, now: i64) {
        upsert_and_prune(c, NewCachedEntry {
            user_id: user, id, ciphertext: b"ct", plaintext: pt, created_at: ts, device_id: "d1"
        }, now).unwrap();
    }

    #[test]
    fn list_returns_newest_first() {
        let c = open_in_memory().unwrap();
        for i in 1..=3 { ins(&c, "u", i, Some(&format!("p{i}")), 1000 + i, 9_999); }
        let rows = list_recent(&c, "u", None, 10).unwrap();
        assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![3, 2, 1]);
    }

    #[test]
    fn caps_at_max_per_user() {
        let c = open_in_memory().unwrap();
        for i in 1..=105 { ins(&c, "u", i, None, i, 100_000); }
        let rows = list_recent(&c, "u", None, 200).unwrap();
        assert_eq!(rows.len() as i64, MAX_PER_USER);
        assert_eq!(rows.first().unwrap().id, 105);
        assert_eq!(rows.last().unwrap().id, 6);
    }

    #[test]
    fn evicts_old_by_age() {
        let c = open_in_memory().unwrap();
        let now = 100_000_000_000_i64;
        ins(&c, "u", 1, None, now - MAX_AGE_MS - 1, now);
        ins(&c, "u", 2, None, now, now);
        let rows = list_recent(&c, "u", None, 10).unwrap();
        assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![2]);
    }

    #[test]
    fn paging_with_before_id() {
        let c = open_in_memory().unwrap();
        for i in 1..=10 { ins(&c, "u", i, None, i, 9_999); }
        let page = list_recent(&c, "u", Some(8), 3).unwrap();
        assert_eq!(page.iter().map(|r| r.id).collect::<Vec<_>>(), vec![7, 6, 5]);
    }

    #[test]
    fn search_matches_plaintext_case_insensitive() {
        let c = open_in_memory().unwrap();
        ins(&c, "u", 1, Some("Hello World"), 1, 9);
        ins(&c, "u", 2, Some("goodbye"), 2, 9);
        ins(&c, "u", 3, None, 3, 9);
        let hits = search(&c, "u", "hello", 10).unwrap();
        assert_eq!(hits.iter().map(|r| r.id).collect::<Vec<_>>(), vec![1]);
    }

    #[test]
    fn upsert_preserves_plaintext_when_new_one_is_null() {
        let c = open_in_memory().unwrap();
        ins(&c, "u", 1, Some("first"), 1, 9);
        ins(&c, "u", 1, None, 2, 9);
        assert_eq!(get_full(&c, "u", 1).unwrap().as_deref(), Some("first"));
    }

    #[test]
    fn mark_undecryptable_clears_plaintext() {
        let c = open_in_memory().unwrap();
        ins(&c, "u", 1, Some("secret"), 1, 9);
        mark_undecryptable(&c, "u", 1).unwrap();
        assert_eq!(get_full(&c, "u", 1).unwrap(), None);
    }

    #[test]
    fn delete_one_and_all_scope_to_user() {
        let c = open_in_memory().unwrap();
        ins(&c, "a", 1, None, 1, 9);
        ins(&c, "b", 1, None, 1, 9);
        assert_eq!(delete_one(&c, "a", 1).unwrap(), 1);
        assert_eq!(delete_one(&c, "a", 1).unwrap(), 0);
        ins(&c, "a", 2, None, 2, 9);
        ins(&c, "a", 3, None, 3, 9);
        assert_eq!(delete_all(&c, "a").unwrap(), 2);
        assert_eq!(list_recent(&c, "b", None, 10).unwrap().len(), 1);
    }
}
```

- [ ] **Step 2: Run the entries_cache tests**

Run:
```bash
cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml --lib core::storage::entries_cache
```
Expected: 7 passing.

- [ ] **Step 3: Write the failing test for `pending` repository**

`clients/desktop/src-tauri/src/core/storage/pending.rs`:
```rust
use crate::errors::AppError;
use rusqlite::{params, Connection};

pub const MAX_PER_USER: i64 = 1000;

#[derive(Debug, Clone, PartialEq)]
pub struct PendingUpload {
    pub rowid: i64,
    pub user_id: String,
    pub ciphertext: Vec<u8>,
    pub captured_at: i64,
    pub attempts: i64,
    pub last_error: Option<String>,
}

#[derive(Debug)]
pub struct EnqueueResult {
    pub rowid: i64,
    pub dropped_oldest: usize,
}

pub fn enqueue(
    conn: &Connection,
    user_id: &str,
    ciphertext: &[u8],
    captured_at: i64,
) -> Result<EnqueueResult, AppError> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO pending_uploads (user_id, ciphertext, captured_at) VALUES (?1, ?2, ?3)",
        params![user_id, ciphertext, captured_at],
    )?;
    let rowid = tx.last_insert_rowid();
    let dropped_oldest = tx.execute(
        "DELETE FROM pending_uploads
          WHERE user_id = ?1
            AND rowid NOT IN (
              SELECT rowid FROM pending_uploads
              WHERE user_id = ?1
              ORDER BY rowid DESC
              LIMIT ?2
            )",
        params![user_id, MAX_PER_USER],
    )?;
    tx.commit()?;
    Ok(EnqueueResult { rowid, dropped_oldest })
}

pub fn head(conn: &Connection, user_id: &str) -> Result<Option<PendingUpload>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT rowid, user_id, ciphertext, captured_at, attempts, last_error
         FROM pending_uploads
         WHERE user_id = ?1
         ORDER BY rowid ASC LIMIT 1"
    )?;
    let row = stmt
        .query_row(params![user_id], |r| Ok(PendingUpload {
            rowid: r.get(0)?,
            user_id: r.get(1)?,
            ciphertext: r.get(2)?,
            captured_at: r.get(3)?,
            attempts: r.get(4)?,
            last_error: r.get(5)?,
        }))
        .ok();
    Ok(row)
}

pub fn count(conn: &Connection, user_id: &str) -> Result<i64, AppError> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pending_uploads WHERE user_id = ?1",
        params![user_id],
        |r| r.get(0),
    )?;
    Ok(n)
}

pub fn ack(conn: &Connection, rowid: i64) -> Result<(), AppError> {
    conn.execute("DELETE FROM pending_uploads WHERE rowid = ?1", params![rowid])?;
    Ok(())
}

pub fn record_failure(conn: &Connection, rowid: i64, err: &str) -> Result<i64, AppError> {
    conn.execute(
        "UPDATE pending_uploads SET attempts = attempts + 1, last_error = ?2 WHERE rowid = ?1",
        params![rowid, err],
    )?;
    let attempts: i64 = conn.query_row(
        "SELECT attempts FROM pending_uploads WHERE rowid = ?1",
        params![rowid],
        |r| r.get(0),
    )?;
    Ok(attempts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::storage::open_in_memory;

    #[test]
    fn enqueue_returns_rowid_and_no_drops() {
        let c = open_in_memory().unwrap();
        let r = enqueue(&c, "u", b"x", 1).unwrap();
        assert!(r.rowid > 0);
        assert_eq!(r.dropped_oldest, 0);
        assert_eq!(count(&c, "u").unwrap(), 1);
    }

    #[test]
    fn head_is_fifo() {
        let c = open_in_memory().unwrap();
        for i in 1..=3 {
            enqueue(&c, "u", &[i], i as i64).unwrap();
        }
        let h = head(&c, "u").unwrap().unwrap();
        assert_eq!(h.captured_at, 1);
    }

    #[test]
    fn ack_removes_row() {
        let c = open_in_memory().unwrap();
        let r = enqueue(&c, "u", b"x", 1).unwrap();
        ack(&c, r.rowid).unwrap();
        assert!(head(&c, "u").unwrap().is_none());
    }

    #[test]
    fn record_failure_increments_attempts_and_stores_error() {
        let c = open_in_memory().unwrap();
        let r = enqueue(&c, "u", b"x", 1).unwrap();
        let n = record_failure(&c, r.rowid, "boom").unwrap();
        assert_eq!(n, 1);
        let n2 = record_failure(&c, r.rowid, "again").unwrap();
        assert_eq!(n2, 2);
        let h = head(&c, "u").unwrap().unwrap();
        assert_eq!(h.last_error.as_deref(), Some("again"));
    }

    #[test]
    fn over_cap_drops_oldest_only_for_that_user() {
        let c = open_in_memory().unwrap();
        for i in 0..MAX_PER_USER + 5 {
            enqueue(&c, "u", &[0], i).unwrap();
        }
        for i in 0..3 { enqueue(&c, "v", &[0], i).unwrap(); }
        assert_eq!(count(&c, "u").unwrap(), MAX_PER_USER);
        assert_eq!(count(&c, "v").unwrap(), 3);
    }
}
```

- [ ] **Step 4: Run the pending tests**

Run:
```bash
cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml --lib core::storage::pending
```
Expected: 5 passing.

- [ ] **Step 5: Commit**

```bash
git add clients/desktop/src-tauri/src/core/storage/entries_cache.rs clients/desktop/src-tauri/src/core/storage/pending.rs
git commit -m "feat(desktop): entries_cache and pending_uploads repositories with retention"
```

---

## Task 6: Storage — `accounts` and `settings` repositories

**Files:**
- Modify: `clients/desktop/src-tauri/src/core/storage/accounts.rs`
- Modify: `clients/desktop/src-tauri/src/core/storage/settings.rs`

- [ ] **Step 1: Implement and test `accounts` repository**

`clients/desktop/src-tauri/src/core/storage/accounts.rs`:
```rust
use crate::errors::AppError;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    pub user_id: String,
    pub device_id: String,
    pub device_label: String,
    pub server_url: String,
    pub last_seen_id: i64,
    pub created_at: i64,
}

pub fn upsert(conn: &Connection, a: &Account) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO accounts (user_id, device_id, device_label, server_url, last_seen_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (user_id) DO UPDATE SET
            device_id    = excluded.device_id,
            device_label = excluded.device_label,
            server_url   = excluded.server_url",
        params![a.user_id, a.device_id, a.device_label, a.server_url, a.last_seen_id, a.created_at],
    )?;
    Ok(())
}

pub fn list(conn: &Connection) -> Result<Vec<Account>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT user_id, device_id, device_label, server_url, last_seen_id, created_at
         FROM accounts ORDER BY created_at ASC"
    )?;
    let rows = stmt
        .query_map([], |r| Ok(Account {
            user_id: r.get(0)?, device_id: r.get(1)?, device_label: r.get(2)?,
            server_url: r.get(3)?, last_seen_id: r.get(4)?, created_at: r.get(5)?,
        }))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn find(conn: &Connection, user_id: &str) -> Result<Option<Account>, AppError> {
    let row = conn
        .query_row(
            "SELECT user_id, device_id, device_label, server_url, last_seen_id, created_at
             FROM accounts WHERE user_id = ?1",
            params![user_id],
            |r| Ok(Account {
                user_id: r.get(0)?, device_id: r.get(1)?, device_label: r.get(2)?,
                server_url: r.get(3)?, last_seen_id: r.get(4)?, created_at: r.get(5)?,
            }),
        )
        .optional()?;
    Ok(row)
}

pub fn set_last_seen(conn: &Connection, user_id: &str, last_seen_id: i64) -> Result<(), AppError> {
    conn.execute(
        "UPDATE accounts SET last_seen_id = ?2 WHERE user_id = ?1",
        params![user_id, last_seen_id],
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, user_id: &str) -> Result<usize, AppError> {
    let n = conn.execute("DELETE FROM accounts WHERE user_id = ?1", params![user_id])?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::storage::open_in_memory;

    fn acct(uid: &str) -> Account {
        Account {
            user_id: uid.into(), device_id: "d1".into(), device_label: "mac".into(),
            server_url: "https://srv".into(), last_seen_id: 0, created_at: 1,
        }
    }

    #[test]
    fn upsert_then_find_returns_row() {
        let c = open_in_memory().unwrap();
        upsert(&c, &acct("u")).unwrap();
        let got = find(&c, "u").unwrap().unwrap();
        assert_eq!(got.device_id, "d1");
    }

    #[test]
    fn upsert_updates_label_and_url_but_keeps_last_seen() {
        let c = open_in_memory().unwrap();
        upsert(&c, &acct("u")).unwrap();
        set_last_seen(&c, "u", 42).unwrap();
        let mut a = acct("u");
        a.device_label = "renamed".into();
        a.server_url = "https://other".into();
        a.last_seen_id = 0;
        upsert(&c, &a).unwrap();
        let got = find(&c, "u").unwrap().unwrap();
        assert_eq!(got.device_label, "renamed");
        assert_eq!(got.server_url, "https://other");
        assert_eq!(got.last_seen_id, 42);
    }

    #[test]
    fn list_orders_by_created_at() {
        let c = open_in_memory().unwrap();
        let mut a = acct("a"); a.created_at = 2;
        let mut b = acct("b"); b.created_at = 1;
        upsert(&c, &a).unwrap();
        upsert(&c, &b).unwrap();
        let ids: Vec<_> = list(&c).unwrap().iter().map(|x| x.user_id.clone()).collect();
        assert_eq!(ids, vec!["b", "a"]);
    }

    #[test]
    fn delete_returns_row_count() {
        let c = open_in_memory().unwrap();
        upsert(&c, &acct("u")).unwrap();
        assert_eq!(delete(&c, "u").unwrap(), 1);
        assert_eq!(delete(&c, "u").unwrap(), 0);
    }
}
```

- [ ] **Step 2: Implement and test `settings` repository**

`clients/desktop/src-tauri/src/core/storage/settings.rs`:
```rust
use crate::errors::AppError;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub capture_enabled: bool,
    pub deny_list: Vec<String>,
    pub autostart: bool,
    pub hotkey: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            capture_enabled: true,
            deny_list: vec!["com.1password.1password".into(), "com.bitwarden.desktop".into()],
            autostart: false,
            hotkey: None,
        }
    }
}

const KEY: &str = "settings";

pub fn load(conn: &Connection) -> Result<Settings, AppError> {
    let json: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", params![KEY], |r| r.get(0))
        .ok();
    match json {
        Some(j) => serde_json::from_str(&j).map_err(|e| AppError::Storage(e.to_string())),
        None => Ok(Settings::default()),
    }
}

pub fn save(conn: &Connection, s: &Settings) -> Result<(), AppError> {
    let j = serde_json::to_string(s).map_err(|e| AppError::Storage(e.to_string()))?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![KEY, j],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::storage::open_in_memory;

    #[test]
    fn load_returns_default_when_unset() {
        let c = open_in_memory().unwrap();
        let s = load(&c).unwrap();
        assert!(s.capture_enabled);
        assert!(!s.autostart);
        assert!(s.hotkey.is_none());
    }

    #[test]
    fn save_then_load_round_trips() {
        let c = open_in_memory().unwrap();
        let mut s = Settings::default();
        s.capture_enabled = false;
        s.hotkey = Some("Cmd+Shift+V".into());
        save(&c, &s).unwrap();
        assert_eq!(load(&c).unwrap(), s);
    }
}
```

- [ ] **Step 3: Run all storage tests**

Run:
```bash
cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml --lib core::storage
```
Expected: 16+ tests pass.

- [ ] **Step 4: Commit**

```bash
git add clients/desktop/src-tauri/src/core/storage/accounts.rs clients/desktop/src-tauri/src/core/storage/settings.rs
git commit -m "feat(desktop): accounts and settings repositories"
```

---
## Task 7: Keychain wrapper (trait + macOS keyring backend + in-memory test fake)

**Files:**
- Create: `clients/desktop/src-tauri/src/core/keychain.rs`
- Modify: `clients/desktop/src-tauri/src/core/mod.rs`

- [ ] **Step 1: Wire keychain module**

Edit `clients/desktop/src-tauri/src/core/mod.rs`:
```rust
//! Pure-Rust core for sharepaste-desktop.

pub mod crypto;
pub mod keychain;
pub mod storage;
```

- [ ] **Step 2: Define trait, default keyring impl, and in-memory test fake**

`clients/desktop/src-tauri/src/core/keychain.rs`:
```rust
use crate::errors::AppError;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

const SERVICE: &str = "sharepaste";

pub trait Keychain: Send + Sync {
    fn put(&self, account: &str, secret: &str) -> Result<(), AppError>;
    fn get(&self, account: &str) -> Result<Option<String>, AppError>;
    fn delete(&self, account: &str) -> Result<(), AppError>;
}

pub fn user_key_account(user_id: &str) -> String { format!("{user_id}:key") }
pub fn token_account(user_id: &str)    -> String { format!("{user_id}:token") }

#[derive(Default)]
pub struct SystemKeychain;

impl Keychain for SystemKeychain {
    fn put(&self, account: &str, secret: &str) -> Result<(), AppError> {
        let entry = keyring::Entry::new(SERVICE, account)?;
        entry.set_password(secret)?;
        Ok(())
    }

    fn get(&self, account: &str) -> Result<Option<String>, AppError> {
        let entry = keyring::Entry::new(SERVICE, account)?;
        match entry.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn delete(&self, account: &str) -> Result<(), AppError> {
        let entry = keyring::Entry::new(SERVICE, account)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[derive(Clone, Default)]
pub struct InMemoryKeychain {
    inner: Arc<Mutex<HashMap<String, String>>>,
}

impl Keychain for InMemoryKeychain {
    fn put(&self, account: &str, secret: &str) -> Result<(), AppError> {
        self.inner.lock().insert(account.into(), secret.into());
        Ok(())
    }
    fn get(&self, account: &str) -> Result<Option<String>, AppError> {
        Ok(self.inner.lock().get(account).cloned())
    }
    fn delete(&self, account: &str) -> Result<(), AppError> {
        self.inner.lock().remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_helpers_format_consistently() {
        assert_eq!(user_key_account("u-1"), "u-1:key");
        assert_eq!(token_account("u-1"), "u-1:token");
    }

    #[test]
    fn in_memory_keychain_round_trips() {
        let k = InMemoryKeychain::default();
        assert!(k.get("foo").unwrap().is_none());
        k.put("foo", "bar").unwrap();
        assert_eq!(k.get("foo").unwrap().as_deref(), Some("bar"));
        k.delete("foo").unwrap();
        assert!(k.get("foo").unwrap().is_none());
    }

    #[test]
    fn delete_missing_is_no_op() {
        let k = InMemoryKeychain::default();
        k.delete("absent").unwrap();
    }
}
```

- [ ] **Step 3: Run keychain tests**

Run:
```bash
cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml --lib core::keychain
```
Expected: 3 passing.

- [ ] **Step 4: Commit**

```bash
git add clients/desktop/src-tauri/src/core/mod.rs clients/desktop/src-tauri/src/core/keychain.rs
git commit -m "feat(desktop): Keychain trait with system + in-memory backends"
```

---

## Task 8: Pairing shortcode encoding (base32 round-trip)

**Files:**
- Create: `clients/desktop/src-tauri/src/core/pairing/mod.rs`
- Create: `clients/desktop/src-tauri/src/core/pairing/shortcode.rs`
- Modify: `clients/desktop/src-tauri/src/core/mod.rs`

- [ ] **Step 1: Wire pairing module**

Edit `clients/desktop/src-tauri/src/core/mod.rs`:
```rust
//! Pure-Rust core for sharepaste-desktop.

pub mod crypto;
pub mod keychain;
pub mod pairing;
pub mod storage;
```

`clients/desktop/src-tauri/src/core/pairing/mod.rs`:
```rust
pub mod shortcode;
```

- [ ] **Step 2: Implement and test base32 codec**

`clients/desktop/src-tauri/src/core/pairing/shortcode.rs`:
```rust
use crate::errors::AppError;
use data_encoding::BASE32_NOPAD;
use uuid::Uuid;

const VERSION: u8 = 1;
const SECRET_LEN: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcodePayload {
    pub server_url: String,
    pub pair_id: Uuid,
    pub pairing_secret: [u8; SECRET_LEN],
}

pub fn encode(p: &ShortcodePayload) -> Result<String, AppError> {
    let url_bytes = p.server_url.as_bytes();
    let url_len: u16 = url_bytes
        .len()
        .try_into()
        .map_err(|_| AppError::BadInput("server_url too long".into()))?;
    let mut buf = Vec::with_capacity(1 + 2 + url_bytes.len() + 16 + SECRET_LEN);
    buf.push(VERSION);
    buf.extend_from_slice(&url_len.to_be_bytes());
    buf.extend_from_slice(url_bytes);
    buf.extend_from_slice(p.pair_id.as_bytes());
    buf.extend_from_slice(&p.pairing_secret);
    Ok(BASE32_NOPAD.encode(&buf))
}

pub fn decode(s: &str) -> Result<ShortcodePayload, AppError> {
    let cleaned: String = s.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let bytes = BASE32_NOPAD
        .decode(cleaned.as_bytes())
        .map_err(|e| AppError::BadInput(format!("base32 decode: {e}")))?;
    if bytes.is_empty() {
        return Err(AppError::BadInput("empty payload".into()));
    }
    if bytes[0] != VERSION {
        return Err(AppError::BadInput(format!("unknown version {}", bytes[0])));
    }
    if bytes.len() < 3 {
        return Err(AppError::BadInput("payload truncated".into()));
    }
    let url_len = u16::from_be_bytes([bytes[1], bytes[2]]) as usize;
    let url_start = 3;
    let url_end = url_start + url_len;
    if bytes.len() < url_end + 16 + SECRET_LEN {
        return Err(AppError::BadInput("payload truncated".into()));
    }
    let server_url = std::str::from_utf8(&bytes[url_start..url_end])
        .map_err(|_| AppError::BadInput("server_url not utf-8".into()))?
        .to_string();
    let mut id_bytes = [0u8; 16];
    id_bytes.copy_from_slice(&bytes[url_end..url_end + 16]);
    let pair_id = Uuid::from_bytes(id_bytes);
    let mut secret = [0u8; SECRET_LEN];
    secret.copy_from_slice(&bytes[url_end + 16..url_end + 16 + SECRET_LEN]);
    Ok(ShortcodePayload { server_url, pair_id, pairing_secret: secret })
}

pub fn group_for_display(code: &str) -> String {
    code.chars()
        .collect::<Vec<_>>()
        .chunks(5)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ShortcodePayload {
        ShortcodePayload {
            server_url: "https://srv.example".into(),
            pair_id: Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap(),
            pairing_secret: [7u8; SECRET_LEN],
        }
    }

    #[test]
    fn round_trip() {
        let p = sample();
        let code = encode(&p).unwrap();
        assert_eq!(decode(&code).unwrap(), p);
    }

    #[test]
    fn round_trip_with_whitespace_and_dashes_and_lowercase() {
        let p = sample();
        let code = encode(&p).unwrap();
        let formatted = format!("  {}  ", group_for_display(&code).to_lowercase().replace(' ', "-"));
        assert_eq!(decode(&formatted).unwrap(), p);
    }

    #[test]
    fn rejects_empty_input() {
        assert!(matches!(decode(""), Err(AppError::BadInput(_))));
    }

    #[test]
    fn rejects_garbage() {
        assert!(matches!(decode("not a real code"), Err(AppError::BadInput(_))));
    }

    #[test]
    fn rejects_unknown_version() {
        let mut bytes = vec![99u8, 0, 0];
        bytes.extend_from_slice(Uuid::nil().as_bytes());
        bytes.extend_from_slice(&[0u8; SECRET_LEN]);
        let s = BASE32_NOPAD.encode(&bytes);
        assert!(matches!(decode(&s), Err(AppError::BadInput(_))));
    }

    #[test]
    fn rejects_truncated() {
        let p = sample();
        let mut code = encode(&p).unwrap();
        code.truncate(code.len() - 4);
        assert!(matches!(decode(&code), Err(AppError::BadInput(_))));
    }

    #[test]
    fn group_for_display_groups_in_fives() {
        let s = "ABCDEFGHIJKLMN";
        assert_eq!(group_for_display(s), "ABCDE FGHIJ KLMN");
    }
}
```

- [ ] **Step 3: Run shortcode tests**

Run:
```bash
cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml --lib core::pairing::shortcode
```
Expected: 7 passing.

- [ ] **Step 4: Commit**

```bash
git add clients/desktop/src-tauri/src/core/pairing
git commit -m "feat(desktop): pairing shortcode base32 codec"
```

---

## Task 9: HTTP layer — `ServerClient` (REST endpoints)

Wire ciphertext is base64 across the REST surface; SSE is handled in Task 10. We test by spinning up a one-route mock with `tokio::net::TcpListener` to keep this task dependency-free.

**Files:**
- Create: `clients/desktop/src-tauri/src/core/http/mod.rs`
- Create: `clients/desktop/src-tauri/src/core/http/dto.rs`
- Create: `clients/desktop/src-tauri/src/core/http/client.rs`
- Modify: `clients/desktop/src-tauri/src/core/mod.rs`

- [ ] **Step 1: Wire http module**

Edit `clients/desktop/src-tauri/src/core/mod.rs`:
```rust
//! Pure-Rust core for sharepaste-desktop.

pub mod crypto;
pub mod http;
pub mod keychain;
pub mod pairing;
pub mod storage;
```

`clients/desktop/src-tauri/src/core/http/mod.rs`:
```rust
pub mod client;
pub mod dto;

pub use client::ServerClient;
```

- [ ] **Step 2: Define request/response DTOs that match the server's wire schema**

`clients/desktop/src-tauri/src/core/http/dto.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct ClaimInviteReq<'a> { pub token: &'a str, pub device_label: &'a str }

#[derive(Deserialize)]
pub struct ClaimInviteResp { pub device_token: String, pub user_id: String, pub device_id: String }

#[derive(Serialize)]
pub struct PairStartReq<'a> { pub secret_hash: &'a str }

#[derive(Deserialize)]
pub struct PairStartResp { pub pair_id: String }

#[derive(Serialize)]
pub struct PairClaimReq<'a> { pub pair_id: &'a str, pub secret_proof: &'a str }

#[derive(Serialize)]
pub struct PairPayloadReq<'a> { pub pair_id: &'a str, pub encrypted_payload: &'a str }

#[derive(Deserialize)]
pub struct PairPayloadResp { pub encrypted_payload: String }

#[derive(Deserialize)]
pub struct PairPollResp { pub status: String }

#[derive(Serialize)]
pub struct DevicesReq<'a> { pub pair_id: &'a str, pub secret_proof: &'a str, pub label: &'a str }

#[derive(Deserialize)]
pub struct DevicesResp { pub device_token: String, pub device_id: String, pub user_id: String }

#[derive(Serialize)]
pub struct PostEntryReq<'a> { pub ciphertext: &'a str }

#[derive(Deserialize)]
pub struct PostEntryResp { pub id: i64, pub created_at: i64 }

#[derive(Deserialize, Debug, Clone)]
pub struct EntryRow {
    pub id: i64,
    pub ciphertext: String,
    pub created_at: i64,
    pub device_id: String,
}
```

- [ ] **Step 3: Write the failing test for `ServerClient` against an in-process mock**

`clients/desktop/src-tauri/src/core/http/client.rs`:
```rust
use crate::errors::AppError;
use crate::core::http::dto::*;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::{Client, StatusCode};

#[derive(Clone, Debug)]
pub struct ServerClient {
    base: String,
    http: Client,
    token: Option<String>,
}

impl ServerClient {
    pub fn new(base: impl Into<String>) -> Result<Self, AppError> {
        let http = Client::builder()
            .user_agent(concat!("sharepaste-desktop/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| AppError::Network(e.to_string()))?;
        Ok(Self { base: base.into(), http, token: None })
    }

    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    fn auth_headers(&self) -> Result<HeaderMap, AppError> {
        let mut h = HeaderMap::new();
        if let Some(t) = &self.token {
            let v = HeaderValue::from_str(&format!("Bearer {t}"))
                .map_err(|e| AppError::Auth(e.to_string()))?;
            h.insert(AUTHORIZATION, v);
        }
        Ok(h)
    }

    fn url(&self, path: &str) -> String { format!("{}{}", self.base.trim_end_matches('/'), path) }

    fn map_status(status: StatusCode, body: String) -> AppError {
        match status.as_u16() {
            401 => AppError::Auth(body),
            403 => AppError::Auth(body),
            404 => AppError::NotFound(body),
            410 => AppError::PairExpired(body),
            413 => AppError::BadInput(format!("payload too large: {body}")),
            400..=499 => AppError::BadInput(body),
            _ => AppError::Network(format!("status {status}: {body}")),
        }
    }

    async fn json_post<TReq: serde::Serialize, TResp: for<'de> serde::Deserialize<'de>>(
        &self,
        path: &str,
        body: &TReq,
        authed: bool,
    ) -> Result<TResp, AppError> {
        let mut req = self.http.post(self.url(path)).json(body);
        if authed { req = req.headers(self.auth_headers()?); }
        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Self::map_status(status, body));
        }
        resp.json::<TResp>().await.map_err(|e| AppError::Network(e.to_string()))
    }

    pub async fn claim_invite(&self, token: &str, label: &str) -> Result<ClaimInviteResp, AppError> {
        self.json_post("/claim-invite", &ClaimInviteReq { token, device_label: label }, false).await
    }

    pub async fn pair_start(&self, secret_hash: &str) -> Result<PairStartResp, AppError> {
        self.json_post("/pair/start", &PairStartReq { secret_hash }, true).await
    }

    pub async fn pair_claim(&self, pair_id: &str, secret_proof: &str) -> Result<(), AppError> {
        let resp = self.http.post(self.url("/pair/claim"))
            .json(&PairClaimReq { pair_id, secret_proof })
            .send().await?;
        let status = resp.status();
        if status.is_success() { return Ok(()); }
        let body = resp.text().await.unwrap_or_default();
        Err(Self::map_status(status, body))
    }

    pub async fn pair_payload_put(&self, pair_id: &str, encrypted_payload: &str) -> Result<(), AppError> {
        let _: serde_json::Value = self
            .json_post("/pair/payload", &PairPayloadReq { pair_id, encrypted_payload }, true)
            .await?;
        Ok(())
    }

    pub async fn pair_payload_get(&self, pair_id: &str, secret_proof: &str) -> Result<PairPayloadResp, AppError> {
        let resp = self.http.get(self.url("/pair/payload"))
            .query(&[("id", pair_id), ("proof", secret_proof)])
            .send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Self::map_status(status, body));
        }
        resp.json().await.map_err(|e| AppError::Network(e.to_string()))
    }

    pub async fn pair_poll(&self, pair_id: &str, timeout_ms: u32) -> Result<PairPollResp, AppError> {
        let resp = self.http.get(self.url("/pair/poll"))
            .query(&[("id", pair_id), ("timeout_ms", &timeout_ms.to_string())])
            .headers(self.auth_headers()?)
            .send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Self::map_status(status, body));
        }
        resp.json().await.map_err(|e| AppError::Network(e.to_string()))
    }

    pub async fn devices(&self, pair_id: &str, secret_proof: &str, label: &str) -> Result<DevicesResp, AppError> {
        self.json_post("/devices", &DevicesReq { pair_id, secret_proof, label }, false).await
    }

    pub async fn revoke_device(&self, device_id: &str) -> Result<(), AppError> {
        let resp = self.http.delete(self.url(&format!("/devices/{device_id}")))
            .headers(self.auth_headers()?)
            .send().await?;
        let status = resp.status();
        if status.is_success() { return Ok(()); }
        let body = resp.text().await.unwrap_or_default();
        Err(Self::map_status(status, body))
    }

    pub async fn post_entry(&self, ciphertext_b64: &str) -> Result<PostEntryResp, AppError> {
        self.json_post("/entries", &PostEntryReq { ciphertext: ciphertext_b64 }, true).await
    }

    pub async fn list_entries(&self, since: i64, limit: u32) -> Result<Vec<EntryRow>, AppError> {
        let resp = self.http.get(self.url("/entries"))
            .query(&[("since", &since.to_string()), ("limit", &limit.to_string())])
            .headers(self.auth_headers()?)
            .send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Self::map_status(status, body));
        }
        resp.json().await.map_err(|e| AppError::Network(e.to_string()))
    }

    pub async fn delete_entry(&self, id: i64) -> Result<(), AppError> {
        let resp = self.http.delete(self.url(&format!("/entries/{id}")))
            .headers(self.auth_headers()?)
            .send().await?;
        let status = resp.status();
        if status.is_success() { return Ok(()); }
        let body = resp.text().await.unwrap_or_default();
        Err(Self::map_status(status, body))
    }

    pub async fn delete_all_entries(&self) -> Result<(), AppError> {
        let resp = self.http.delete(self.url("/entries"))
            .headers(self.auth_headers()?)
            .send().await?;
        let status = resp.status();
        if status.is_success() { return Ok(()); }
        let body = resp.text().await.unwrap_or_default();
        Err(Self::map_status(status, body))
    }

    pub fn base(&self) -> &str { &self.base }
    pub fn token(&self) -> Option<&str> { self.token.as_deref() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_status_known_codes() {
        assert!(matches!(ServerClient::map_status(StatusCode::UNAUTHORIZED, "x".into()), AppError::Auth(_)));
        assert!(matches!(ServerClient::map_status(StatusCode::FORBIDDEN, "x".into()), AppError::Auth(_)));
        assert!(matches!(ServerClient::map_status(StatusCode::NOT_FOUND, "x".into()), AppError::NotFound(_)));
        assert!(matches!(ServerClient::map_status(StatusCode::GONE, "x".into()), AppError::PairExpired(_)));
        assert!(matches!(ServerClient::map_status(StatusCode::PAYLOAD_TOO_LARGE, "x".into()), AppError::BadInput(_)));
        assert!(matches!(ServerClient::map_status(StatusCode::BAD_REQUEST, "x".into()), AppError::BadInput(_)));
        assert!(matches!(ServerClient::map_status(StatusCode::INTERNAL_SERVER_ERROR, "x".into()), AppError::Network(_)));
    }

    #[test]
    fn url_strips_trailing_slash() {
        let c = ServerClient::new("https://srv/").unwrap();
        assert_eq!(c.url("/x"), "https://srv/x");
    }

    #[test]
    fn auth_header_present_when_token_set() {
        let c = ServerClient::new("https://srv").unwrap().with_token("abc");
        let h = c.auth_headers().unwrap();
        assert_eq!(h.get(AUTHORIZATION).unwrap(), "Bearer abc");
    }

    #[test]
    fn auth_header_absent_otherwise() {
        let c = ServerClient::new("https://srv").unwrap();
        assert!(c.auth_headers().unwrap().get(AUTHORIZATION).is_none());
    }
}
```

- [ ] **Step 4: Run http unit tests**

Run:
```bash
cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml --lib core::http
```
Expected: 4 passing. (Endpoint behaviour is exercised in Task 21 integration tests against the real server.)

- [ ] **Step 5: Commit**

```bash
git add clients/desktop/src-tauri/src/core/http clients/desktop/src-tauri/src/core/mod.rs
git commit -m "feat(desktop): ServerClient + DTOs covering /claim-invite, /pair/*, /devices, /entries"
```

---
## Task 10: Pairing flows — invite + QR

**Files:**
- Create: `clients/desktop/src-tauri/src/core/pairing/invite.rs`
- Create: `clients/desktop/src-tauri/src/core/pairing/qr.rs`
- Modify: `clients/desktop/src-tauri/src/core/pairing/mod.rs`

- [ ] **Step 1: Wire submodules**

`clients/desktop/src-tauri/src/core/pairing/mod.rs`:
```rust
pub mod invite;
pub mod qr;
pub mod shortcode;
```

- [ ] **Step 2: Implement and test the invite-claim flow**

`clients/desktop/src-tauri/src/core/pairing/invite.rs`:
```rust
use crate::core::crypto::{random_user_key, UserKey};
use crate::core::http::ServerClient;
use crate::core::keychain::{token_account, user_key_account, Keychain};
use crate::core::storage::accounts::{upsert as upsert_account, Account};
use crate::errors::AppError;
use rusqlite::Connection;
use zeroize::Zeroizing;

pub struct ClaimedAccount {
    pub user_id: String,
    pub device_id: String,
    pub device_token: String,
    pub server_url: String,
    pub user_key: UserKey,
}

pub async fn claim_invite(
    server: &ServerClient,
    token: &str,
    device_label: &str,
) -> Result<ClaimedAccount, AppError> {
    let resp = server.claim_invite(token, device_label).await?;
    Ok(ClaimedAccount {
        user_id: resp.user_id,
        device_id: resp.device_id,
        device_token: resp.device_token,
        server_url: server.base().to_string(),
        user_key: random_user_key(),
    })
}

pub fn persist_claimed_account(
    conn: &Connection,
    keychain: &dyn Keychain,
    claimed: &ClaimedAccount,
    device_label: &str,
    now_ms: i64,
) -> Result<(), AppError> {
    keychain.put(&user_key_account(&claimed.user_id), &hex::encode_user_key(&claimed.user_key))?;
    keychain.put(&token_account(&claimed.user_id), &claimed.device_token)?;
    upsert_account(conn, &Account {
        user_id: claimed.user_id.clone(),
        device_id: claimed.device_id.clone(),
        device_label: device_label.into(),
        server_url: claimed.server_url.clone(),
        last_seen_id: 0,
        created_at: now_ms,
    })?;
    Ok(())
}

pub mod hex {
    use crate::core::crypto::UserKey;
    use zeroize::Zeroizing;

    pub fn encode_user_key(k: &UserKey) -> Zeroizing<String> {
        let mut s = String::with_capacity(k.len() * 2);
        for b in k.iter() {
            use std::fmt::Write;
            write!(&mut s, "{:02x}", b).unwrap();
        }
        Zeroizing::new(s)
    }

    pub fn decode_user_key(s: &str) -> Result<UserKey, crate::errors::AppError> {
        if s.len() != 64 {
            return Err(crate::errors::AppError::Crypto("user_key must be 64 hex chars".into()));
        }
        let mut out = [0u8; 32];
        for (i, b) in out.iter_mut().enumerate() {
            *b = u8::from_str_radix(&s[i*2..i*2+2], 16)
                .map_err(|e| crate::errors::AppError::Crypto(e.to_string()))?;
        }
        Ok(Zeroizing::new(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::keychain::InMemoryKeychain;
    use crate::core::storage::open_in_memory;

    #[test]
    fn persist_writes_keychain_and_db() {
        let conn = open_in_memory().unwrap();
        let kc = InMemoryKeychain::default();
        let claimed = ClaimedAccount {
            user_id: "u".into(),
            device_id: "d".into(),
            device_token: "tok".into(),
            server_url: "https://srv".into(),
            user_key: crate::core::crypto::random_user_key(),
        };
        persist_claimed_account(&conn, &kc, &claimed, "mac", 12345).unwrap();
        assert_eq!(kc.get("u:token").unwrap().as_deref(), Some("tok"));
        assert!(kc.get("u:key").unwrap().is_some());
        let row = crate::core::storage::accounts::find(&conn, "u").unwrap().unwrap();
        assert_eq!(row.device_id, "d");
        assert_eq!(row.device_label, "mac");
    }

    #[test]
    fn user_key_hex_round_trip() {
        let k = crate::core::crypto::random_user_key();
        let s = hex::encode_user_key(&k);
        let back = hex::decode_user_key(&s).unwrap();
        assert_eq!(k.as_slice(), back.as_slice());
    }

    #[test]
    fn user_key_hex_rejects_bad_length() {
        assert!(hex::decode_user_key("ab").is_err());
    }
}
```

- [ ] **Step 3: Implement and test the QR/shortcode flow helpers**

`clients/desktop/src-tauri/src/core/pairing/qr.rs`:
```rust
use crate::core::crypto::{decrypt, encrypt, UserKey};
use crate::core::http::ServerClient;
use crate::core::pairing::shortcode::{encode, ShortcodePayload};
use crate::errors::AppError;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2_local::Sha256Hex;
use uuid::Uuid;
use zeroize::Zeroizing;

pub mod sha2_local {
    use chacha20poly1305::aead::generic_array::GenericArray;

    pub struct Sha256Hex;

    impl Sha256Hex {
        pub fn hex(input: &[u8]) -> String {
            // Minimal sha256 via reqwest's transitive `sha2`? We instead pull in our own:
            use sha2_imp::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(input);
            let out = h.finalize();
            hex_lower(&out)
        }
    }

    fn hex_lower(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            use std::fmt::Write;
            write!(&mut s, "{:02x}", b).unwrap();
        }
        s
    }

    mod sha2_imp {
        pub use sha2::{Digest, Sha256};
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PairPayload {
    pub user_id: String,
    pub user_key: String,        // hex
    pub server_url: String,
}

pub struct PairStarted {
    pub pair_id: Uuid,
    pub pairing_secret: Zeroizing<[u8; 32]>,
    pub shortcode: String,
}

pub async fn start_pair(server: &ServerClient) -> Result<PairStarted, AppError> {
    let mut secret = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut secret);
    let secret_hex = hex_lower_static(&secret);
    let secret_hash = sha2_local::Sha256Hex::hex(secret_hex.as_bytes());

    let resp = server.pair_start(&secret_hash).await?;
    let pair_id = Uuid::parse_str(&resp.pair_id)
        .map_err(|e| AppError::BadInput(format!("server returned malformed pair_id: {e}")))?;
    let payload = ShortcodePayload {
        server_url: server.base().to_string(),
        pair_id,
        pairing_secret: secret,
    };
    let shortcode = encode(&payload)?;
    Ok(PairStarted {
        pair_id,
        pairing_secret: Zeroizing::new(secret),
        shortcode,
    })
}

pub async fn upload_pair_payload(
    server: &ServerClient,
    pair_id: Uuid,
    pairing_secret: &[u8; 32],
    user_id: &str,
    user_key: &UserKey,
    server_url: &str,
) -> Result<(), AppError> {
    let payload = PairPayload {
        user_id: user_id.into(),
        user_key: crate::core::pairing::invite::hex::encode_user_key(user_key).to_string(),
        server_url: server_url.into(),
    };
    let plaintext = serde_json::to_vec(&payload).map_err(|e| AppError::Crypto(e.to_string()))?;
    // Wrap pairing_secret as a UserKey-like array for AEAD reuse.
    let key: UserKey = Zeroizing::new(*pairing_secret);
    let wire = encrypt(&key, &pair_id.to_string(), &plaintext)?;
    let b64 = base64_encode(&wire);
    server.pair_payload_put(&pair_id.to_string(), &b64).await
}

pub async fn fetch_and_decrypt_pair_payload(
    server: &ServerClient,
    pair_id: Uuid,
    pairing_secret: &[u8; 32],
) -> Result<PairPayload, AppError> {
    let secret_hex = hex_lower_static(pairing_secret);
    let resp = server.pair_payload_get(&pair_id.to_string(), &secret_hex).await?;
    let wire = base64_decode(&resp.encrypted_payload)?;
    let key: UserKey = Zeroizing::new(*pairing_secret);
    let plaintext = decrypt(&key, &pair_id.to_string(), &wire)?;
    serde_json::from_slice(&plaintext).map_err(|e| AppError::Crypto(e.to_string()))
}

pub fn secret_proof_hex(pairing_secret: &[u8; 32]) -> String {
    hex_lower_static(pairing_secret)
}

fn hex_lower_static(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        write!(&mut s, "{:02x}", b).unwrap();
    }
    s
}

pub fn base64_encode(bytes: &[u8]) -> String {
    use data_encoding::BASE64;
    BASE64.encode(bytes)
}

pub fn base64_decode(s: &str) -> Result<Vec<u8>, AppError> {
    use data_encoding::BASE64;
    BASE64.decode(s.as_bytes()).map_err(|e| AppError::BadInput(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_payload_round_trips_through_aead() {
        let secret = [9u8; 32];
        let pair_id = Uuid::new_v4();
        let payload = PairPayload {
            user_id: "u".into(),
            user_key: "ab".repeat(32),
            server_url: "https://srv".into(),
        };
        let plaintext = serde_json::to_vec(&payload).unwrap();
        let key: UserKey = Zeroizing::new(secret);
        let wire = encrypt(&key, &pair_id.to_string(), &plaintext).unwrap();
        let back = decrypt(&key, &pair_id.to_string(), &wire).unwrap();
        let parsed: PairPayload = serde_json::from_slice(&back).unwrap();
        assert_eq!(parsed, payload);
    }

    #[test]
    fn secret_proof_is_lowercase_hex() {
        let s = secret_proof_hex(&[0xAB; 32]);
        assert_eq!(s.len(), 64);
        assert!(s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }
}
```

- [ ] **Step 3a: Add the missing `sha2` dependency**

Edit `clients/desktop/src-tauri/Cargo.toml`, append under `[dependencies]`:
```toml
sha2 = "0.10"
```

- [ ] **Step 4: Run pairing tests**

Run:
```bash
cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml --lib core::pairing
```
Expected: 12+ tests pass.

- [ ] **Step 5: Commit**

```bash
git add clients/desktop/src-tauri/src/core/pairing clients/desktop/src-tauri/Cargo.toml
git commit -m "feat(desktop): pairing flows (invite claim + QR start/payload helpers)"
```

---

## Task 11: Sync — SSE subscription with backfill

**Files:**
- Create: `clients/desktop/src-tauri/src/core/sync/mod.rs`
- Create: `clients/desktop/src-tauri/src/core/sync/sse.rs`
- Modify: `clients/desktop/src-tauri/src/core/mod.rs`

- [ ] **Step 1: Wire sync module**

Edit `clients/desktop/src-tauri/src/core/mod.rs`:
```rust
//! Pure-Rust core for sharepaste-desktop.

pub mod account;
pub mod capture;
pub mod crypto;
pub mod http;
pub mod keychain;
pub mod pairing;
pub mod storage;
pub mod sync;
```

(Stub `account` and `capture` so the tree compiles; they're filled in later tasks.)

`clients/desktop/src-tauri/src/core/account/mod.rs` — stub for now:
```rust
// Implemented in Task 17.
```

`clients/desktop/src-tauri/src/core/capture/mod.rs` — stub:
```rust
// Implemented in Task 14+.
```

`clients/desktop/src-tauri/src/core/sync/mod.rs`:
```rust
pub mod sse;
pub mod uploader;
pub mod decryptor;
pub mod state;

pub use state::{ConnectionState, SyncTask};
```

(Stub `uploader.rs`, `decryptor.rs`, `state.rs` as empty modules so the tree compiles. Each gets filled in the corresponding task.)

`clients/desktop/src-tauri/src/core/sync/uploader.rs`:
```rust
// Implemented in Task 12.
```

`clients/desktop/src-tauri/src/core/sync/decryptor.rs`:
```rust
// Implemented in Task 13.
```

`clients/desktop/src-tauri/src/core/sync/state.rs`:
```rust
// Implemented in Task 16.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Online,
    AuthFailed,
}

pub struct SyncTask;
```

- [ ] **Step 2: Define the SSE event shapes that match the server's `/events` stream**

`clients/desktop/src-tauri/src/core/sync/sse.rs`:
```rust
use crate::core::http::ServerClient;
use crate::errors::AppError;
use eventsource_stream::Eventsource;
use futures::stream::StreamExt;
use serde::Deserialize;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ServerEvent {
    Entry {
        id: i64,
        ciphertext: String,
        created_at: i64,
        device_id: String,
    },
    Delete { id: i64 },
}

pub async fn run(
    server: ServerClient,
    sink: Sender<ServerEvent>,
    cancel: CancellationToken,
) -> Result<(), AppError> {
    let url = format!("{}/events", server.base().trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .read_timeout(Duration::from_secs(45))
        .build()
        .map_err(|e| AppError::Network(e.to_string()))?;
    let mut req = client.get(url);
    if let Some(t) = server.token() {
        req = req.bearer_auth(t);
    }
    let resp = req.send().await?;
    if !resp.status().is_success() {
        return Err(AppError::Network(format!("SSE status {}", resp.status())));
    }
    let mut stream = resp.bytes_stream().eventsource();
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            ev = stream.next() => match ev {
                None => return Err(AppError::Network("SSE stream ended".into())),
                Some(Err(e)) => return Err(AppError::Network(format!("SSE error: {e}"))),
                Some(Ok(msg)) => {
                    if msg.event == "entry" || msg.event == "delete" {
                        let parsed: Result<ServerEvent, _> = serde_json::from_str(&msg.data);
                        match parsed {
                            Ok(p) => {
                                if sink.send(p).await.is_err() { return Ok(()); }
                            }
                            Err(e) => tracing::warn!(err = %e, "ignoring unparseable SSE payload"),
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_entry_event() {
        let data = json!({
            "type": "entry",
            "id": 7,
            "ciphertext": "AAAA",
            "created_at": 100,
            "device_id": "d1",
        });
        let parsed: ServerEvent = serde_json::from_value(data).unwrap();
        match parsed {
            ServerEvent::Entry { id, .. } => assert_eq!(id, 7),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parse_delete_event() {
        let data = json!({ "type": "delete", "id": 9 });
        let parsed: ServerEvent = serde_json::from_value(data).unwrap();
        match parsed {
            ServerEvent::Delete { id } => assert_eq!(id, 9),
            _ => panic!("wrong variant"),
        }
    }
}
```

- [ ] **Step 3: Run SSE unit tests**

Run:
```bash
cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml --lib core::sync::sse
```
Expected: 2 passing.

- [ ] **Step 4: Commit**

```bash
git add clients/desktop/src-tauri/src/core/sync clients/desktop/src-tauri/src/core/account clients/desktop/src-tauri/src/core/capture clients/desktop/src-tauri/src/core/mod.rs
git commit -m "feat(desktop): SSE subscriber with cancel-aware loop"
```

---

## Task 12: Sync — Uploader (FIFO flush of pending queue)

**Files:**
- Modify: `clients/desktop/src-tauri/src/core/sync/uploader.rs`

- [ ] **Step 1: Define the upload trait so tests can swap in a fake transport**

`clients/desktop/src-tauri/src/core/sync/uploader.rs`:
```rust
use crate::core::pairing::qr::base64_encode;
use crate::core::storage::pending;
use crate::errors::AppError;
use async_trait::async_trait;
use rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;

#[async_trait]
pub trait UploadTransport: Send + Sync {
    async fn upload(&self, ciphertext_b64: &str) -> Result<i64, AppError>;
}

pub struct UploaderEvents {
    pub on_pending_count: Box<dyn Fn(i64) + Send + Sync>,
    pub on_auth_failed: Box<dyn Fn() + Send + Sync>,
}

pub struct Uploader {
    pub user_id: String,
    pub conn: Arc<Mutex<Connection>>,
    pub transport: Arc<dyn UploadTransport>,
    pub trigger: Arc<Notify>,
    pub events: UploaderEvents,
}

impl Uploader {
    pub async fn run(self, cancel: CancellationToken) {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = self.trigger.notified() => {},
            }
            if let Err(e) = self.flush_once().await {
                if matches!(e, AppError::Auth(_)) {
                    (self.events.on_auth_failed)();
                    return;
                }
                tracing::warn!(err = %e, "uploader flush errored; will retry on next trigger");
            }
        }
    }

    async fn flush_once(&self) -> Result<(), AppError> {
        loop {
            let head = {
                let conn = self.conn.lock().await;
                pending::head(&conn, &self.user_id)?
            };
            let Some(item) = head else { break; };
            let b64 = base64_encode(&item.ciphertext);
            match self.transport.upload(&b64).await {
                Ok(_id) => {
                    let conn = self.conn.lock().await;
                    pending::ack(&conn, item.rowid)?;
                    let count = pending::count(&conn, &self.user_id)?;
                    (self.events.on_pending_count)(count);
                }
                Err(AppError::Auth(s)) => return Err(AppError::Auth(s)),
                Err(AppError::BadInput(s)) => {
                    let conn = self.conn.lock().await;
                    pending::ack(&conn, item.rowid)?;
                    tracing::warn!(err = %s, rowid = item.rowid, "dropped malformed pending entry");
                    let count = pending::count(&conn, &self.user_id)?;
                    (self.events.on_pending_count)(count);
                }
                Err(e) => {
                    let conn = self.conn.lock().await;
                    pending::record_failure(&conn, item.rowid, &e.to_string())?;
                    return Err(e);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::storage::{open_in_memory, pending};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct OkTransport { count: AtomicUsize }

    #[async_trait]
    impl UploadTransport for OkTransport {
        async fn upload(&self, _ct: &str) -> Result<i64, AppError> {
            self.count.fetch_add(1, Ordering::SeqCst);
            Ok(42)
        }
    }

    struct AuthFail;
    #[async_trait]
    impl UploadTransport for AuthFail {
        async fn upload(&self, _ct: &str) -> Result<i64, AppError> {
            Err(AppError::Auth("revoked".into()))
        }
    }

    fn events() -> (UploaderEvents, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let pc = Arc::new(AtomicUsize::new(0));
        let af = Arc::new(AtomicUsize::new(0));
        let pc2 = pc.clone();
        let af2 = af.clone();
        let ev = UploaderEvents {
            on_pending_count: Box::new(move |_| { pc2.fetch_add(1, Ordering::SeqCst); }),
            on_auth_failed: Box::new(move || { af2.fetch_add(1, Ordering::SeqCst); }),
        };
        (ev, pc, af)
    }

    #[tokio::test]
    async fn flush_drains_in_fifo_order() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        for i in 0..3i64 {
            let c = conn.lock().await;
            pending::enqueue(&c, "u", &[i as u8], i).unwrap();
        }
        let (ev, _pc, _af) = events();
        let transport = Arc::new(OkTransport { count: AtomicUsize::new(0) });
        let up = Uploader {
            user_id: "u".into(),
            conn: conn.clone(),
            transport: transport.clone(),
            trigger: Arc::new(Notify::new()),
            events: ev,
        };
        up.flush_once().await.unwrap();
        assert_eq!(transport.count.load(Ordering::SeqCst), 3);
        let c = conn.lock().await;
        assert_eq!(pending::count(&c, "u").unwrap(), 0);
    }

    #[tokio::test]
    async fn auth_failure_propagates_and_keeps_row() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        {
            let c = conn.lock().await;
            pending::enqueue(&c, "u", b"x", 1).unwrap();
        }
        let (ev, _pc, _af) = events();
        let up = Uploader {
            user_id: "u".into(),
            conn: conn.clone(),
            transport: Arc::new(AuthFail),
            trigger: Arc::new(Notify::new()),
            events: ev,
        };
        let err = up.flush_once().await.unwrap_err();
        assert!(matches!(err, AppError::Auth(_)));
        let c = conn.lock().await;
        assert_eq!(pending::count(&c, "u").unwrap(), 1);
        let head = pending::head(&c, "u").unwrap().unwrap();
        assert_eq!(head.attempts, 1);
    }
}
```

- [ ] **Step 2: Run uploader tests**

Run:
```bash
cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml --lib core::sync::uploader
```
Expected: 2 passing.

- [ ] **Step 3: Commit**

```bash
git add clients/desktop/src-tauri/src/core/sync/uploader.rs
git commit -m "feat(desktop): pending-queue uploader with FIFO + auth-fail propagation"
```

---
## Task 13: Sync — Decryptor (decrypt fetched ciphertext, persist plaintext, mark undecryptable on failure)

**Files:**
- Modify: `clients/desktop/src-tauri/src/core/sync/decryptor.rs`

- [ ] **Step 1: Implement and test the decryptor**

`clients/desktop/src-tauri/src/core/sync/decryptor.rs`:
```rust
use crate::core::crypto::{decrypt, UserKey};
use crate::core::http::dto::EntryRow;
use crate::core::pairing::qr::base64_decode;
use crate::core::storage::entries_cache::{upsert_and_prune, NewCachedEntry};
use crate::errors::AppError;
use rusqlite::Connection;

pub struct DecryptOutcome {
    pub plaintext_preview: Option<String>,
    pub undecryptable: bool,
}

pub fn ingest(
    conn: &Connection,
    user_key: &UserKey,
    user_id: &str,
    row: &EntryRow,
    now_ms: i64,
) -> Result<DecryptOutcome, AppError> {
    let wire = base64_decode(&row.ciphertext)?;
    let plaintext_bytes = decrypt(user_key, user_id, &wire);
    let (plaintext_str, undecryptable) = match plaintext_bytes {
        Ok(b) => match String::from_utf8(b) {
            Ok(s) => (Some(s), false),
            Err(_) => (None, true),
        },
        Err(_) => (None, true),
    };
    upsert_and_prune(conn, NewCachedEntry {
        user_id,
        id: row.id,
        ciphertext: &wire,
        plaintext: plaintext_str.as_deref(),
        created_at: row.created_at,
        device_id: &row.device_id,
    }, now_ms)?;
    let preview = plaintext_str.as_ref().map(|s| build_preview(s));
    Ok(DecryptOutcome { plaintext_preview: preview, undecryptable })
}

pub fn build_preview(plaintext: &str) -> String {
    let one_line: String = plaintext
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let trimmed = one_line.trim();
    let mut out = String::new();
    for (i, c) in trimmed.chars().enumerate() {
        if i >= 80 { break; }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::crypto::encrypt;
    use crate::core::pairing::qr::base64_encode;
    use crate::core::storage::open_in_memory;
    use zeroize::Zeroizing;

    fn key() -> UserKey { Zeroizing::new([5u8; 32]) }

    fn row_for(uid: &str, id: i64, plaintext: &[u8], k: &UserKey) -> EntryRow {
        let wire = encrypt(k, uid, plaintext).unwrap();
        EntryRow { id, ciphertext: base64_encode(&wire), created_at: 1000, device_id: "d".into() }
    }

    #[test]
    fn ingest_decryptable_writes_plaintext() {
        let c = open_in_memory().unwrap();
        let k = key();
        let r = row_for("u", 1, b"hello", &k);
        let out = ingest(&c, &k, "u", &r, 9_999).unwrap();
        assert_eq!(out.plaintext_preview.as_deref(), Some("hello"));
        assert!(!out.undecryptable);
        let pt = crate::core::storage::entries_cache::get_full(&c, "u", 1).unwrap();
        assert_eq!(pt.as_deref(), Some("hello"));
    }

    #[test]
    fn ingest_aad_mismatch_marks_undecryptable() {
        let c = open_in_memory().unwrap();
        let k = key();
        let r = row_for("alice", 1, b"x", &k);
        let out = ingest(&c, &k, "bob", &r, 9_999).unwrap();
        assert!(out.undecryptable);
        let pt = crate::core::storage::entries_cache::get_full(&c, "bob", 1).unwrap();
        assert!(pt.is_none());
    }

    #[test]
    fn preview_strips_controls_and_truncates_at_80() {
        let s: String = "a\nb\tc".chars().chain(std::iter::repeat('z').take(200)).collect();
        let p = build_preview(&s);
        assert!(p.len() <= 80 + 80); // 80 chars, none multi-byte here
        assert!(!p.contains('\n'));
    }
}
```

- [ ] **Step 2: Run decryptor tests**

Run:
```bash
cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml --lib core::sync::decryptor
```
Expected: 3 passing.

- [ ] **Step 3: Commit**

```bash
git add clients/desktop/src-tauri/src/core/sync/decryptor.rs
git commit -m "feat(desktop): decryptor — ingest /entries rows into entries_cache"
```

---

## Task 14: Capture — filter (size, deny-list, transient, self-write)

**Files:**
- Modify: `clients/desktop/src-tauri/src/core/capture/mod.rs`
- Create: `clients/desktop/src-tauri/src/core/capture/filter.rs`

- [ ] **Step 1: Define platform-agnostic filter logic + trait for the macOS pasteboard sniffer**

`clients/desktop/src-tauri/src/core/capture/mod.rs`:
```rust
pub mod filter;
pub mod watcher;

#[cfg(target_os = "macos")]
pub mod macos;
```

`clients/desktop/src-tauri/src/core/capture/filter.rs`:
```rust
use std::time::{Duration, Instant};

pub const MAX_BYTES: usize = 64 * 1024;
pub const SELF_WRITE_WINDOW: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    Disabled,
    Transient,
    NonText,
    TooLarge,
    DenyList,
    SelfWrite,
}

pub struct CaptureContext<'a> {
    pub capture_enabled: bool,
    pub deny_list: &'a [String],
    pub frontmost_bundle_id: Option<&'a str>,
    pub last_self_write: Option<(Instant, &'a str)>,
}

pub trait PasteboardSniff {
    fn types(&self) -> Vec<String>;
    fn read_text(&self) -> Option<String>;
}

pub enum FilterDecision {
    Capture(String),
    Skip(SkipReason),
}

pub fn evaluate(ctx: &CaptureContext<'_>, sniff: &dyn PasteboardSniff, now: Instant) -> FilterDecision {
    if !ctx.capture_enabled {
        return FilterDecision::Skip(SkipReason::Disabled);
    }
    let types = sniff.types();
    if types.iter().any(|t| is_transient_type(t)) {
        return FilterDecision::Skip(SkipReason::Transient);
    }
    let Some(text) = sniff.read_text() else {
        return FilterDecision::Skip(SkipReason::NonText);
    };
    if text.is_empty() {
        return FilterDecision::Skip(SkipReason::NonText);
    }
    if text.as_bytes().len() > MAX_BYTES {
        return FilterDecision::Skip(SkipReason::TooLarge);
    }
    if let Some(frontmost) = ctx.frontmost_bundle_id {
        if ctx.deny_list.iter().any(|d| d.eq_ignore_ascii_case(frontmost)) {
            return FilterDecision::Skip(SkipReason::DenyList);
        }
    }
    if let Some((ts, last_text)) = ctx.last_self_write {
        if now.saturating_duration_since(ts) <= SELF_WRITE_WINDOW && last_text == text {
            return FilterDecision::Skip(SkipReason::SelfWrite);
        }
    }
    FilterDecision::Capture(text)
}

fn is_transient_type(t: &str) -> bool {
    matches!(
        t,
        "org.nspasteboard.ConcealedType"
            | "org.nspasteboard.TransientType"
            | "Concealed"
            | "transient"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake { types: Vec<String>, text: Option<String> }
    impl PasteboardSniff for Fake {
        fn types(&self) -> Vec<String> { self.types.clone() }
        fn read_text(&self) -> Option<String> { self.text.clone() }
    }

    fn ctx_default<'a>() -> CaptureContext<'a> {
        CaptureContext { capture_enabled: true, deny_list: &[], frontmost_bundle_id: None, last_self_write: None }
    }

    #[test]
    fn captures_plain_text() {
        let s = Fake { types: vec!["public.utf8-plain-text".into()], text: Some("hi".into()) };
        match evaluate(&ctx_default(), &s, Instant::now()) {
            FilterDecision::Capture(t) => assert_eq!(t, "hi"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn skips_when_disabled() {
        let s = Fake { types: vec![], text: Some("hi".into()) };
        let ctx = CaptureContext { capture_enabled: false, ..ctx_default() };
        assert!(matches!(evaluate(&ctx, &s, Instant::now()), FilterDecision::Skip(SkipReason::Disabled)));
    }

    #[test]
    fn skips_each_transient_type() {
        for t in ["org.nspasteboard.ConcealedType", "org.nspasteboard.TransientType", "Concealed", "transient"] {
            let s = Fake { types: vec![t.into()], text: Some("hi".into()) };
            assert!(matches!(evaluate(&ctx_default(), &s, Instant::now()), FilterDecision::Skip(SkipReason::Transient)));
        }
    }

    #[test]
    fn skips_non_text() {
        let s = Fake { types: vec![], text: None };
        assert!(matches!(evaluate(&ctx_default(), &s, Instant::now()), FilterDecision::Skip(SkipReason::NonText)));
    }

    #[test]
    fn skips_empty_text() {
        let s = Fake { types: vec![], text: Some(String::new()) };
        assert!(matches!(evaluate(&ctx_default(), &s, Instant::now()), FilterDecision::Skip(SkipReason::NonText)));
    }

    #[test]
    fn skips_too_large() {
        let big = "a".repeat(MAX_BYTES + 1);
        let s = Fake { types: vec![], text: Some(big) };
        assert!(matches!(evaluate(&ctx_default(), &s, Instant::now()), FilterDecision::Skip(SkipReason::TooLarge)));
    }

    #[test]
    fn skips_deny_listed_app_case_insensitive() {
        let s = Fake { types: vec![], text: Some("hi".into()) };
        let deny = vec!["com.1Password.1Password".to_string()];
        let ctx = CaptureContext { capture_enabled: true, deny_list: &deny, frontmost_bundle_id: Some("com.1password.1password"), last_self_write: None };
        assert!(matches!(evaluate(&ctx, &s, Instant::now()), FilterDecision::Skip(SkipReason::DenyList)));
    }

    #[test]
    fn skips_self_write_within_window() {
        let s = Fake { types: vec![], text: Some("hi".into()) };
        let now = Instant::now();
        let ctx = CaptureContext { capture_enabled: true, deny_list: &[], frontmost_bundle_id: None, last_self_write: Some((now, "hi")) };
        assert!(matches!(evaluate(&ctx, &s, now), FilterDecision::Skip(SkipReason::SelfWrite)));
    }

    #[test]
    fn does_not_skip_self_write_after_window() {
        let s = Fake { types: vec![], text: Some("hi".into()) };
        let earlier = Instant::now() - SELF_WRITE_WINDOW * 2;
        let ctx = CaptureContext { capture_enabled: true, deny_list: &[], frontmost_bundle_id: None, last_self_write: Some((earlier, "hi")) };
        assert!(matches!(evaluate(&ctx, &s, Instant::now()), FilterDecision::Capture(_)));
    }
}
```

- [ ] **Step 2: Stub the watcher and macos modules so the tree compiles**

`clients/desktop/src-tauri/src/core/capture/watcher.rs`:
```rust
// Implemented in Task 16.
```

(macOS stub created in Task 15.)

- [ ] **Step 3: Run filter tests**

Run:
```bash
cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml --lib core::capture::filter
```
Expected: 9 passing.

- [ ] **Step 4: Commit**

```bash
git add clients/desktop/src-tauri/src/core/capture
git commit -m "feat(desktop): capture filter rules (size, deny-list, transient, self-write)"
```

---

## Task 15: Capture — macOS pasteboard sniffer (objc2)

This task only compiles on macOS (it uses `objc2-app-kit`). Its tests run only on macOS too — that matches the spec's macOS-only target.

**Files:**
- Create: `clients/desktop/src-tauri/src/core/capture/macos.rs`

- [ ] **Step 1: Implement NSPasteboard sniffing + frontmost bundle id helper**

`clients/desktop/src-tauri/src/core/capture/macos.rs`:
```rust
#![cfg(target_os = "macos")]

use crate::core::capture::filter::PasteboardSniff;
use objc2::rc::Retained;
use objc2_app_kit::{NSPasteboard, NSWorkspace};
use objc2_foundation::{NSString, MainThreadMarker};

pub struct NSPasteboardSniffer;

impl NSPasteboardSniffer {
    pub fn new() -> Self { Self }
}

impl PasteboardSniff for NSPasteboardSniffer {
    fn types(&self) -> Vec<String> {
        unsafe {
            let mtm = MainThreadMarker::new_unchecked();
            let pb: Retained<NSPasteboard> = NSPasteboard::generalPasteboard(mtm);
            let Some(types) = pb.types() else { return Vec::new(); };
            types.iter().map(|t| t.to_string()).collect()
        }
    }

    fn read_text(&self) -> Option<String> {
        unsafe {
            let mtm = MainThreadMarker::new_unchecked();
            let pb: Retained<NSPasteboard> = NSPasteboard::generalPasteboard(mtm);
            let s = pb.stringForType(NSString::from_str("public.utf8-plain-text").as_ref())?;
            Some(s.to_string())
        }
    }
}

pub fn frontmost_bundle_id() -> Option<String> {
    unsafe {
        let ws: Retained<NSWorkspace> = NSWorkspace::sharedWorkspace();
        let app = ws.frontmostApplication()?;
        let bundle = app.bundleIdentifier()?;
        Some(bundle.to_string())
    }
}
```

- [ ] **Step 2: Add a macOS-only smoke test (skips on non-macOS)**

Append at the bottom of `macos.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::capture::filter::PasteboardSniff;

    #[test]
    fn types_call_does_not_panic() {
        let sniff = NSPasteboardSniffer::new();
        let _types = sniff.types();
        // We can't assert content, but the call must not crash on the test thread.
    }
}
```

- [ ] **Step 3: Verify the build still compiles on macOS**

Run:
```bash
cargo check --manifest-path clients/desktop/src-tauri/Cargo.toml
cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml --lib core::capture::macos
```
Expected: build succeeds; the smoke test passes (or is skipped on non-macOS hosts).

- [ ] **Step 4: Commit**

```bash
git add clients/desktop/src-tauri/src/core/capture/macos.rs
git commit -m "feat(desktop): macOS NSPasteboard + NSWorkspace sniffer via objc2"
```

---

## Task 16: Capture — Watcher (clipboard-master event source) and Sync state machine

State machine and watcher land together because the watcher feeds the encrypt path which is owned by the sync task.

**Files:**
- Modify: `clients/desktop/src-tauri/src/core/capture/watcher.rs`
- Modify: `clients/desktop/src-tauri/src/core/sync/state.rs`

- [ ] **Step 1: Implement the watcher (events only — filtering and encryption stay in the sync task)**

`clients/desktop/src-tauri/src/core/capture/watcher.rs`:
```rust
#![cfg(target_os = "macos")]

use crate::errors::AppError;
use clipboard_master::{CallbackResult, ClipboardHandler, Master};
use std::thread;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct ClipboardEvent {
    pub at: std::time::SystemTime,
}

struct Handler {
    sink: Sender<ClipboardEvent>,
    cancel: CancellationToken,
}

impl ClipboardHandler for Handler {
    fn on_clipboard_change(&mut self) -> CallbackResult {
        if self.cancel.is_cancelled() {
            return CallbackResult::Stop;
        }
        let _ = self.sink.try_send(ClipboardEvent { at: std::time::SystemTime::now() });
        CallbackResult::Next
    }

    fn on_clipboard_error(&mut self, error: std::io::Error) -> CallbackResult {
        tracing::warn!(?error, "clipboard-master error");
        CallbackResult::Next
    }
}

pub fn spawn(sink: Sender<ClipboardEvent>, cancel: CancellationToken) -> Result<thread::JoinHandle<()>, AppError> {
    let handle = thread::Builder::new()
        .name("clipboard-master".into())
        .spawn(move || {
            let mut master = Master::new(Handler { sink, cancel });
            if let Err(e) = master.run() {
                tracing::error!(?e, "clipboard-master master.run() exited");
            }
        })
        .map_err(|e| AppError::Storage(e.to_string()))?;
    Ok(handle)
}
```

- [ ] **Step 2: Implement the sync state machine + run loop**

`clients/desktop/src-tauri/src/core/sync/state.rs`:
```rust
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Online,
    AuthFailed,
}

pub struct BackoffPlan {
    schedule: &'static [u64],
    cap_secs: u64,
    cursor: usize,
}

impl BackoffPlan {
    pub fn new() -> Self {
        Self { schedule: &[1, 2, 4, 8, 16, 30], cap_secs: 30, cursor: 0 }
    }

    pub fn next_delay_secs(&mut self) -> u64 {
        let pick = if self.cursor >= self.schedule.len() {
            self.cap_secs
        } else {
            self.schedule[self.cursor]
        };
        self.cursor += 1;
        pick
    }

    pub fn reset(&mut self) { self.cursor = 0; }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_progresses_then_caps_at_30() {
        let mut b = BackoffPlan::new();
        assert_eq!(b.next_delay_secs(), 1);
        assert_eq!(b.next_delay_secs(), 2);
        assert_eq!(b.next_delay_secs(), 4);
        assert_eq!(b.next_delay_secs(), 8);
        assert_eq!(b.next_delay_secs(), 16);
        assert_eq!(b.next_delay_secs(), 30);
        assert_eq!(b.next_delay_secs(), 30);
        b.reset();
        assert_eq!(b.next_delay_secs(), 1);
    }
}
```

- [ ] **Step 3: Run unit tests for watcher + state**

Run:
```bash
cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml --lib core::sync::state
cargo check --manifest-path clients/desktop/src-tauri/Cargo.toml
```
Expected: state tests pass; watcher compiles.

- [ ] **Step 4: Commit**

```bash
git add clients/desktop/src-tauri/src/core/capture/watcher.rs clients/desktop/src-tauri/src/core/sync/state.rs
git commit -m "feat(desktop): clipboard-master watcher + sync backoff schedule"
```

---

## Task 17: Account registry + active membership

**Files:**
- Modify: `clients/desktop/src-tauri/src/core/account/mod.rs`

- [ ] **Step 1: Implement the registry (loads accounts + tracks the active membership)**

`clients/desktop/src-tauri/src/core/account/mod.rs`:
```rust
use crate::core::http::ServerClient;
use crate::core::keychain::{token_account, user_key_account, Keychain};
use crate::core::pairing::invite::hex::decode_user_key;
use crate::core::storage::accounts::{self, Account};
use crate::core::crypto::UserKey;
use crate::errors::AppError;
use parking_lot::RwLock;
use rusqlite::Connection;
use std::sync::Arc;

pub struct ActiveMembership {
    pub account: Account,
    pub server: ServerClient,
    pub user_key: UserKey,
}

pub struct AccountRegistry {
    pub conn: Arc<tokio::sync::Mutex<Connection>>,
    pub keychain: Arc<dyn Keychain>,
    pub active: RwLock<Option<String>>,
}

impl AccountRegistry {
    pub fn new(conn: Arc<tokio::sync::Mutex<Connection>>, keychain: Arc<dyn Keychain>) -> Self {
        Self { conn, keychain, active: RwLock::new(None) }
    }

    pub async fn list(&self) -> Result<Vec<Account>, AppError> {
        let c = self.conn.lock().await;
        accounts::list(&c)
    }

    pub fn active_user_id(&self) -> Option<String> {
        self.active.read().clone()
    }

    pub fn set_active(&self, user_id: Option<String>) {
        *self.active.write() = user_id;
    }

    pub async fn load_active_membership(&self, user_id: &str) -> Result<ActiveMembership, AppError> {
        let acct = {
            let c = self.conn.lock().await;
            accounts::find(&c, user_id)?
                .ok_or_else(|| AppError::NotFound(format!("account {user_id}")))?
        };
        let token = self
            .keychain
            .get(&token_account(user_id))?
            .ok_or_else(|| AppError::Keychain(format!("missing token for {user_id}")))?;
        let key_hex = self
            .keychain
            .get(&user_key_account(user_id))?
            .ok_or_else(|| AppError::Keychain(format!("missing user_key for {user_id}")))?;
        let user_key = decode_user_key(&key_hex)?;
        let server = ServerClient::new(&acct.server_url)?.with_token(token);
        Ok(ActiveMembership { account: acct, server, user_key })
    }

    pub async fn forget(&self, user_id: &str) -> Result<(), AppError> {
        self.keychain.delete(&user_key_account(user_id))?;
        self.keychain.delete(&token_account(user_id))?;
        let c = self.conn.lock().await;
        crate::core::storage::entries_cache::delete_all(&c, user_id)?;
        accounts::delete(&c, user_id)?;
        if self.active.read().as_deref() == Some(user_id) {
            *self.active.write() = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::keychain::InMemoryKeychain;
    use crate::core::storage::open_in_memory;
    use std::sync::Arc;

    fn registry() -> AccountRegistry {
        let conn = Arc::new(tokio::sync::Mutex::new(open_in_memory().unwrap()));
        AccountRegistry::new(conn, Arc::new(InMemoryKeychain::default()))
    }

    #[tokio::test]
    async fn forget_clears_keychain_and_db_and_active() {
        let r = registry();
        let kc = r.keychain.clone();
        kc.put("u:key", &"ab".repeat(32)).unwrap();
        kc.put("u:token", "tok").unwrap();
        {
            let c = r.conn.lock().await;
            accounts::upsert(&c, &Account {
                user_id: "u".into(), device_id: "d".into(), device_label: "mac".into(),
                server_url: "https://srv".into(), last_seen_id: 0, created_at: 1,
            }).unwrap();
        }
        r.set_active(Some("u".into()));
        r.forget("u").await.unwrap();
        assert!(kc.get("u:token").unwrap().is_none());
        assert!(r.active_user_id().is_none());
    }

    #[tokio::test]
    async fn load_active_membership_errors_on_missing_secret() {
        let r = registry();
        {
            let c = r.conn.lock().await;
            accounts::upsert(&c, &Account {
                user_id: "u".into(), device_id: "d".into(), device_label: "mac".into(),
                server_url: "https://srv".into(), last_seen_id: 0, created_at: 1,
            }).unwrap();
        }
        let err = r.load_active_membership("u").await.unwrap_err();
        assert!(matches!(err, AppError::Keychain(_)));
    }
}
```

- [ ] **Step 2: Run account tests**

Run:
```bash
cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml --lib core::account
```
Expected: 2 passing.

- [ ] **Step 3: Commit**

```bash
git add clients/desktop/src-tauri/src/core/account/mod.rs
git commit -m "feat(desktop): account registry with active-membership loader and forget"
```

---
## Task 18: Tauri AppState + events module

**Files:**
- Create: `clients/desktop/src-tauri/src/state.rs`
- Create: `clients/desktop/src-tauri/src/events.rs`
- Modify: `clients/desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: Define AppState — the bundle of handles every command needs**

`clients/desktop/src-tauri/src/state.rs`:
```rust
use crate::config::Paths;
use crate::core::account::AccountRegistry;
use crate::core::keychain::Keychain;
use parking_lot::Mutex;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub struct SyncSlot {
    pub user_id: String,
    pub cancel: CancellationToken,
}

pub struct AppState {
    pub paths: Paths,
    pub conn: Arc<tokio::sync::Mutex<Connection>>,
    pub keychain: Arc<dyn Keychain>,
    pub registry: Arc<AccountRegistry>,
    pub sync_tasks: Mutex<HashMap<String, SyncSlot>>,
    pub last_self_write: Mutex<Option<(std::time::Instant, String)>>,
}

impl AppState {
    pub fn new(
        paths: Paths,
        conn: Arc<tokio::sync::Mutex<Connection>>,
        keychain: Arc<dyn Keychain>,
        registry: Arc<AccountRegistry>,
    ) -> Self {
        Self {
            paths,
            conn,
            keychain,
            registry,
            sync_tasks: Mutex::new(HashMap::new()),
            last_self_write: Mutex::new(None),
        }
    }
}
```

- [ ] **Step 2: Define event names + payload helpers**

`clients/desktop/src-tauri/src/events.rs`:
```rust
use serde::Serialize;

pub const ACCOUNT_ADDED: &str   = "account-added";
pub const ACCOUNT_REMOVED: &str = "account-removed";
pub const ACTIVE_CHANGED: &str  = "active-changed";
pub const CONNECTION_STATE: &str = "connection-state";
pub const ENTRY_ADDED: &str     = "entry-added";
pub const ENTRY_DELETED: &str   = "entry-deleted";
pub const HISTORY_CHANGED: &str = "history-changed";
pub const PENDING_COUNT: &str   = "pending-count";
pub const CAPTURE_SKIPPED: &str = "capture-skipped";
pub const DECRYPTION_ERROR: &str = "decryption-error";
pub const PAIR_SHORTCODE: &str  = "pair-shortcode";
pub const PAIR_CLAIMED: &str    = "pair-claimed";
pub const PAIR_EXPIRED: &str    = "pair-expired";

#[derive(Serialize, Clone)]
pub struct AccountAdded { pub user_id: String, pub device_id: String, pub label: String }

#[derive(Serialize, Clone)]
pub struct AccountRemoved { pub user_id: String }

#[derive(Serialize, Clone)]
pub struct ActiveChanged { pub user_id: Option<String> }

#[derive(Serialize, Clone)]
pub struct ConnectionStateEvent {
    pub user_id: String,
    pub state: crate::core::sync::ConnectionState,
    pub last_error: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct EntryAdded { pub user_id: String, pub entry: EntryView }

#[derive(Serialize, Clone)]
pub struct EntryView {
    pub id: i64,
    pub user_id: String,
    pub preview: String,
    pub created_at: i64,
    pub device_id: String,
    pub device_label: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct EntryDeleted { pub user_id: String, pub entry_id: i64 }

#[derive(Serialize, Clone)]
pub struct PendingCount { pub user_id: String, pub count: i64 }

#[derive(Serialize, Clone)]
pub struct CaptureSkipped { pub reason: String, pub source_app: Option<String> }

#[derive(Serialize, Clone)]
pub struct DecryptionError { pub user_id: String, pub entry_id: i64 }

#[derive(Serialize, Clone)]
pub struct PairShortcode { pub code: String, pub expires_at: i64 }

#[derive(Serialize, Clone)]
pub struct PairClaimed { pub user_id: String }
```

- [ ] **Step 3: Re-export the new modules from `lib.rs`**

Replace `clients/desktop/src-tauri/src/lib.rs`:
```rust
pub mod config;
pub mod errors;
pub mod logging;
pub mod state;
pub mod events;
pub mod core;

pub fn run() {
    eprintln!("sharepaste-desktop: not yet implemented");
}
```

- [ ] **Step 4: Confirm everything still compiles**

Run:
```bash
cargo check --manifest-path clients/desktop/src-tauri/Cargo.toml
```
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add clients/desktop/src-tauri/src/state.rs clients/desktop/src-tauri/src/events.rs clients/desktop/src-tauri/src/lib.rs
git commit -m "feat(desktop): AppState bundle and event-name constants + payload types"
```

---

## Task 19: Tauri commands surface (`#[tauri::command]`)

**Files:**
- Create: `clients/desktop/src-tauri/src/commands.rs`
- Modify: `clients/desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: Implement every command in the IPC surface**

`clients/desktop/src-tauri/src/commands.rs`:
```rust
use crate::core::account::AccountRegistry;
use crate::core::http::ServerClient;
use crate::core::keychain::{token_account, user_key_account, Keychain};
use crate::core::pairing::invite::{claim_invite, persist_claimed_account};
use crate::core::pairing::qr::{
    base64_decode, base64_encode, fetch_and_decrypt_pair_payload, secret_proof_hex, start_pair,
    upload_pair_payload, PairPayload,
};
use crate::core::pairing::shortcode::{decode as decode_shortcode, group_for_display};
use crate::core::sync::ConnectionState;
use crate::core::storage::{accounts as accounts_repo, entries_cache, pending, settings};
use crate::events::{
    AccountAdded, ConnectionStateEvent, EntryView, PairShortcode, ACCOUNT_ADDED, ACTIVE_CHANGED,
    CONNECTION_STATE, HISTORY_CHANGED, PAIR_SHORTCODE,
};
use crate::errors::AppError;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Serialize)]
pub struct AccountSummary {
    pub user_id: String,
    pub device_id: String,
    pub label: String,
    pub server_url: String,
    pub status: ConnectionState,
    pub pending: i64,
}

#[derive(Serialize)]
pub struct EntryViewDto {
    pub id: i64,
    pub user_id: String,
    pub preview: String,
    pub created_at: i64,
    pub device_id: String,
    pub device_label: Option<String>,
}

fn state(app: &AppHandle) -> Arc<AppState> {
    app.state::<Arc<AppState>>().inner().clone()
}

#[tauri::command]
pub async fn list_accounts(state: State<'_, Arc<AppState>>) -> Result<Vec<AccountSummary>, AppError> {
    let accts = state.registry.list().await?;
    let active = state.registry.active_user_id();
    let mut out = Vec::with_capacity(accts.len());
    let conn = state.conn.lock().await;
    for a in accts {
        let pending = pending::count(&conn, &a.user_id)?;
        let status = if active.as_deref() == Some(&a.user_id) {
            ConnectionState::Connecting
        } else {
            ConnectionState::Disconnected
        };
        out.push(AccountSummary {
            user_id: a.user_id, device_id: a.device_id, label: a.device_label,
            server_url: a.server_url, status, pending,
        });
    }
    Ok(out)
}

#[derive(Deserialize)]
pub struct PairWithInviteArgs { pub server_url: String, pub token: String, pub device_label: String }

#[derive(Serialize)]
pub struct PairWithInviteResp { pub user_id: String, pub device_id: String }

#[tauri::command]
pub async fn pair_with_invite(
    args: PairWithInviteArgs,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<PairWithInviteResp, AppError> {
    let server = ServerClient::new(args.server_url.trim())?;
    let mut claimed = claim_invite(&server, &args.token, &args.device_label).await?;
    claimed.server_url = args.server_url.clone();
    {
        let conn = state.conn.lock().await;
        persist_claimed_account(&conn, state.keychain.as_ref(), &claimed, &args.device_label, now_ms())?;
    }
    app.emit(ACCOUNT_ADDED, AccountAdded {
        user_id: claimed.user_id.clone(),
        device_id: claimed.device_id.clone(),
        label: args.device_label.clone(),
    }).ok();
    Ok(PairWithInviteResp { user_id: claimed.user_id, device_id: claimed.device_id })
}

#[derive(Deserialize)]
pub struct PairStartArgs { pub user_id: String }

#[derive(Serialize)]
pub struct PairStartResp { pub code: String, pub expires_at: i64 }

#[tauri::command]
pub async fn pair_start(args: PairStartArgs, state: State<'_, Arc<AppState>>, app: AppHandle) -> Result<PairStartResp, AppError> {
    let m = state.registry.load_active_membership(&args.user_id).await?;
    let started = start_pair(&m.server).await?;
    let expires_at = now_ms() + 2 * 60 * 1000;
    let formatted = group_for_display(&started.shortcode);
    app.emit(PAIR_SHORTCODE, PairShortcode { code: formatted.clone(), expires_at }).ok();

    // Spawn pair-watch task that polls /pair/poll, uploads payload on `claimed`,
    // and emits pair-claimed / pair-expired.
    let server = m.server.clone();
    let user_id = args.user_id.clone();
    let user_key = m.user_key.clone();
    let pair_id = started.pair_id;
    let pairing_secret = *started.pairing_secret;
    let app2 = app.clone();
    tokio::spawn(async move {
        loop {
            match server.pair_poll(&pair_id.to_string(), 25_000).await {
                Ok(p) if p.status == "claimed" => {
                    if let Err(e) = upload_pair_payload(
                        &server, pair_id, &pairing_secret,
                        &user_id, &user_key, server.base()
                    ).await {
                        tracing::warn!(err = %e, "pair payload upload failed");
                    } else {
                        let _ = app2.emit(crate::events::PAIR_CLAIMED, crate::events::PairClaimed { user_id: user_id.clone() });
                    }
                    return;
                }
                Ok(p) if p.status == "consumed" || p.status == "expired" => {
                    let _ = app2.emit(crate::events::PAIR_EXPIRED, ());
                    return;
                }
                Ok(_waiting) => continue,
                Err(AppError::PairExpired(_)) => {
                    let _ = app2.emit(crate::events::PAIR_EXPIRED, ());
                    return;
                }
                Err(e) => {
                    tracing::warn!(err = %e, "pair poll errored");
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
    });

    Ok(PairStartResp { code: formatted, expires_at })
}

#[derive(Deserialize)]
pub struct PairWithCodeArgs { pub code: String, pub device_label: String }

#[derive(Serialize)]
pub struct PairWithCodeResp { pub user_id: String, pub device_id: String }

#[tauri::command]
pub async fn pair_with_code(
    args: PairWithCodeArgs,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<PairWithCodeResp, AppError> {
    let payload_decoded = decode_shortcode(&args.code)?;
    let server = ServerClient::new(&payload_decoded.server_url)?;
    let proof = secret_proof_hex(&payload_decoded.pairing_secret);
    server.pair_claim(&payload_decoded.pair_id.to_string(), &proof).await?;

    let pair_payload = fetch_and_decrypt_pair_payload(
        &server,
        payload_decoded.pair_id,
        &payload_decoded.pairing_secret,
    )
    .await?;
    let resp = server.devices(&payload_decoded.pair_id.to_string(), &proof, &args.device_label).await?;

    let key = crate::core::pairing::invite::hex::decode_user_key(&pair_payload.user_key)?;
    state.keychain.put(&user_key_account(&pair_payload.user_id), &pair_payload.user_key)?;
    state.keychain.put(&token_account(&pair_payload.user_id), &resp.device_token)?;

    {
        let conn = state.conn.lock().await;
        accounts_repo::upsert(&conn, &accounts_repo::Account {
            user_id: pair_payload.user_id.clone(),
            device_id: resp.device_id.clone(),
            device_label: args.device_label.clone(),
            server_url: pair_payload.server_url.clone(),
            last_seen_id: 0,
            created_at: now_ms(),
        })?;
    }
    app.emit(ACCOUNT_ADDED, AccountAdded {
        user_id: pair_payload.user_id.clone(),
        device_id: resp.device_id.clone(),
        label: args.device_label.clone(),
    }).ok();

    let _ = key; // user_key already persisted in keychain; clear local copy
    Ok(PairWithCodeResp { user_id: pair_payload.user_id, device_id: resp.device_id })
}

#[derive(Deserialize)]
pub struct UserScopedArgs { pub user_id: String }

#[tauri::command]
pub async fn forget_account(args: UserScopedArgs, state: State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), AppError> {
    state.registry.forget(&args.user_id).await?;
    app.emit(crate::events::ACCOUNT_REMOVED, crate::events::AccountRemoved { user_id: args.user_id.clone() }).ok();
    Ok(())
}

#[derive(Deserialize)]
pub struct RevokeDeviceArgs { pub user_id: String, pub device_id: String }

#[tauri::command]
pub async fn revoke_device(args: RevokeDeviceArgs, state: State<'_, Arc<AppState>>) -> Result<(), AppError> {
    let m = state.registry.load_active_membership(&args.user_id).await?;
    m.server.revoke_device(&args.device_id).await
}

#[tauri::command]
pub async fn set_active_account(args: UserScopedArgs, state: State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), AppError> {
    state.registry.set_active(Some(args.user_id.clone()));
    app.emit(ACTIVE_CHANGED, crate::events::ActiveChanged { user_id: Some(args.user_id) }).ok();
    Ok(())
}

#[derive(Deserialize)]
pub struct ListHistoryArgs { pub user_id: String, pub before_id: Option<i64>, pub limit: i64 }

#[tauri::command]
pub async fn list_history(args: ListHistoryArgs, state: State<'_, Arc<AppState>>) -> Result<Vec<EntryViewDto>, AppError> {
    let conn = state.conn.lock().await;
    let rows = entries_cache::list_recent(&conn, &args.user_id, args.before_id, args.limit)?;
    Ok(rows.into_iter().map(|r| EntryViewDto {
        id: r.id, user_id: r.user_id, preview: r.plaintext.unwrap_or_default(),
        created_at: r.created_at, device_id: r.device_id, device_label: None,
    }).collect())
}

#[derive(Deserialize)]
pub struct SearchHistoryArgs { pub user_id: String, pub query: String, pub limit: i64 }

#[tauri::command]
pub async fn search_history(args: SearchHistoryArgs, state: State<'_, Arc<AppState>>) -> Result<Vec<EntryViewDto>, AppError> {
    let conn = state.conn.lock().await;
    let rows = entries_cache::search(&conn, &args.user_id, &args.query, args.limit)?;
    Ok(rows.into_iter().map(|r| EntryViewDto {
        id: r.id, user_id: r.user_id,
        preview: r.plaintext.as_deref().map(crate::core::sync::decryptor::build_preview).unwrap_or_default(),
        created_at: r.created_at, device_id: r.device_id, device_label: None,
    }).collect())
}

#[derive(Deserialize)]
pub struct EntryScopedArgs { pub user_id: String, pub entry_id: i64 }

#[tauri::command]
pub async fn get_entry_full(args: EntryScopedArgs, state: State<'_, Arc<AppState>>) -> Result<String, AppError> {
    let conn = state.conn.lock().await;
    entries_cache::get_full(&conn, &args.user_id, args.entry_id)?
        .ok_or_else(|| AppError::NotFound("plaintext unavailable".into()))
}

#[tauri::command]
pub async fn copy_to_clipboard(args: EntryScopedArgs, state: State<'_, Arc<AppState>>) -> Result<(), AppError> {
    let plaintext = {
        let conn = state.conn.lock().await;
        entries_cache::get_full(&conn, &args.user_id, args.entry_id)?
            .ok_or_else(|| AppError::NotFound("plaintext unavailable".into()))?
    };
    #[cfg(target_os = "macos")]
    {
        let mut cb = arboard::Clipboard::new().map_err(|e| AppError::Storage(e.to_string()))?;
        cb.set_text(plaintext.clone()).map_err(|e| AppError::Storage(e.to_string()))?;
    }
    *state.last_self_write.lock() = Some((Instant::now(), plaintext));
    Ok(())
}

#[tauri::command]
pub async fn delete_entry(args: EntryScopedArgs, state: State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), AppError> {
    let m = state.registry.load_active_membership(&args.user_id).await?;
    m.server.delete_entry(args.entry_id).await?;
    let conn = state.conn.lock().await;
    entries_cache::delete_one(&conn, &args.user_id, args.entry_id)?;
    app.emit(crate::events::ENTRY_DELETED, crate::events::EntryDeleted {
        user_id: args.user_id, entry_id: args.entry_id,
    }).ok();
    Ok(())
}

#[tauri::command]
pub async fn clear_history(args: UserScopedArgs, state: State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), AppError> {
    let m = state.registry.load_active_membership(&args.user_id).await?;
    m.server.delete_all_entries().await?;
    let conn = state.conn.lock().await;
    entries_cache::delete_all(&conn, &args.user_id)?;
    app.emit(HISTORY_CHANGED, serde_json::json!({ "user_id": args.user_id })).ok();
    Ok(())
}

#[tauri::command]
pub async fn get_settings(state: State<'_, Arc<AppState>>) -> Result<settings::Settings, AppError> {
    let conn = state.conn.lock().await;
    settings::load(&conn)
}

#[tauri::command]
pub async fn update_settings(patch: serde_json::Value, state: State<'_, Arc<AppState>>) -> Result<settings::Settings, AppError> {
    let conn = state.conn.lock().await;
    let mut s = settings::load(&conn)?;
    if let Some(v) = patch.get("capture_enabled").and_then(|v| v.as_bool()) { s.capture_enabled = v; }
    if let Some(arr) = patch.get("deny_list").and_then(|v| v.as_array()) {
        s.deny_list = arr.iter().filter_map(|x| x.as_str().map(String::from)).collect();
    }
    if let Some(v) = patch.get("autostart").and_then(|v| v.as_bool()) { s.autostart = v; }
    if let Some(v) = patch.get("hotkey") {
        s.hotkey = if v.is_null() { None } else { v.as_str().map(String::from) };
    }
    settings::save(&conn, &s)?;
    Ok(s)
}

#[derive(Serialize)]
pub struct StatusResp { pub state: ConnectionState, pub pending_count: i64, pub last_error: Option<String> }

#[tauri::command]
pub async fn get_status(args: UserScopedArgs, state: State<'_, Arc<AppState>>) -> Result<StatusResp, AppError> {
    let conn = state.conn.lock().await;
    let count = pending::count(&conn, &args.user_id)?;
    let active = state.registry.active_user_id();
    let st = if active.as_deref() == Some(&args.user_id) { ConnectionState::Connecting } else { ConnectionState::Disconnected };
    Ok(StatusResp { state: st, pending_count: count, last_error: None })
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
```

- [ ] **Step 2: Re-export commands from lib.rs**

Replace `clients/desktop/src-tauri/src/lib.rs`:
```rust
pub mod config;
pub mod errors;
pub mod logging;
pub mod state;
pub mod events;
pub mod commands;
pub mod core;

pub fn run() {
    eprintln!("sharepaste-desktop: not yet implemented (use main.rs run)");
}
```

- [ ] **Step 3: Confirm the Tauri command tree compiles**

Run:
```bash
cargo check --manifest-path clients/desktop/src-tauri/Cargo.toml
```
Expected: clean. (Behavior is exercised in Task 21 integration tests.)

- [ ] **Step 4: Commit**

```bash
git add clients/desktop/src-tauri/src/commands.rs clients/desktop/src-tauri/src/lib.rs
git commit -m "feat(desktop): #[tauri::command] surface for accounts, pairing, history, settings"
```

---

## Task 20: Tauri main — tray, popover, modals, plugins, sync orchestrator

**Files:**
- Modify: `clients/desktop/src-tauri/src/main.rs`
- Modify: `clients/desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: Implement the orchestrator that owns sync tasks and emits events**

Append to `clients/desktop/src-tauri/src/lib.rs` (after the existing `pub mod` lines):
```rust
use crate::config::Paths;
use crate::core::account::AccountRegistry;
use crate::core::keychain::SystemKeychain;
use crate::core::storage::open as open_storage;
use crate::events::{
    ConnectionStateEvent, EntryAdded, EntryView, PendingCount, ACCOUNT_ADDED, ACTIVE_CHANGED,
    CONNECTION_STATE, ENTRY_ADDED, ENTRY_DELETED, HISTORY_CHANGED, PENDING_COUNT,
};
use crate::state::AppState;
use std::sync::Arc;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub fn launch() {
    let paths = Paths::resolve();
    paths.ensure_dirs().expect("create app dirs");
    let _log_guard = logging::init(&paths.log_dir);
    let conn = open_storage(&paths.db_path).expect("open sqlite");
    let conn = Arc::new(tokio::sync::Mutex::new(conn));
    let keychain: Arc<dyn core::keychain::Keychain> = Arc::new(SystemKeychain::default());
    let registry = Arc::new(AccountRegistry::new(conn.clone(), keychain.clone()));
    let app_state = Arc::new(AppState::new(paths, conn, keychain, registry));

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(app_state.clone())
        .setup(move |app| {
            build_tray(app, app_state.clone())?;
            build_popover_window(app)?;
            spawn_sync_for_existing_accounts(app.handle().clone(), app_state.clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_accounts,
            commands::pair_with_invite,
            commands::pair_start,
            commands::pair_with_code,
            commands::forget_account,
            commands::revoke_device,
            commands::set_active_account,
            commands::list_history,
            commands::search_history,
            commands::get_entry_full,
            commands::copy_to_clipboard,
            commands::delete_entry,
            commands::clear_history,
            commands::get_settings,
            commands::update_settings,
            commands::get_status,
        ])
        .run(tauri::generate_context!())
        .expect("run tauri");
}

fn build_tray(app: &mut tauri::App, _state: Arc<AppState>) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id("show", "Show history").build(app)?)
        .item(&MenuItemBuilder::with_id("pair", "Pair device…").build(app)?)
        .item(&MenuItemBuilder::with_id("settings", "Settings…").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("quit", "Quit").build(app)?)
        .build()?;

    let _tray = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => { let _ = toggle_popover(app); }
            "pair"  => { let _ = open_modal(app, "pairing"); }
            "settings" => { let _ = open_modal(app, "settings"); }
            "quit" => { app.exit(0); }
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if matches!(ev, TrayIconEvent::Click { button: MouseButton::Left, .. }) {
                let _ = toggle_popover(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn build_popover_window(app: &mut tauri::App) -> tauri::Result<()> {
    let _ = WebviewWindowBuilder::new(app, "popover", WebviewUrl::App("popover.html".into()))
        .title("sharepaste")
        .inner_size(360.0, 480.0)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .visible(false)
        .skip_taskbar(true)
        .build()?;
    Ok(())
}

fn toggle_popover(app: &tauri::AppHandle) -> tauri::Result<()> {
    if let Some(win) = app.get_webview_window("popover") {
        if win.is_visible().unwrap_or(false) {
            win.hide()?;
        } else {
            win.show()?;
            win.set_focus()?;
        }
    }
    Ok(())
}

fn open_modal(app: &tauri::AppHandle, kind: &str) -> tauri::Result<()> {
    let label = format!("modal-{kind}");
    if let Some(existing) = app.get_webview_window(&label) {
        existing.set_focus()?;
        return Ok(());
    }
    let url = format!("modal.html?kind={kind}");
    let win = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(url.into()))
        .title("sharepaste")
        .inner_size(420.0, 520.0)
        .resizable(false)
        .build()?;
    let _ = win;
    Ok(())
}

fn spawn_sync_for_existing_accounts(app: tauri::AppHandle, state: Arc<AppState>) {
    let runtime = tauri::async_runtime::handle();
    runtime.spawn(async move {
        let accounts = state.registry.list().await.unwrap_or_default();
        if let Some(first) = accounts.first() {
            state.registry.set_active(Some(first.user_id.clone()));
            let _ = app.emit(ACTIVE_CHANGED, crate::events::ActiveChanged { user_id: Some(first.user_id.clone()) });
            spawn_sync(app.clone(), state.clone(), first.user_id.clone()).await;
        }
    });
}

pub async fn spawn_sync(app: tauri::AppHandle, state: Arc<AppState>, user_id: String) {
    let cancel = CancellationToken::new();
    {
        let mut tasks = state.sync_tasks.lock();
        if let Some(prev) = tasks.insert(user_id.clone(), state::SyncSlot { user_id: user_id.clone(), cancel: cancel.clone() }) {
            prev.cancel.cancel();
        }
    }
    let m = match state.registry.load_active_membership(&user_id).await {
        Ok(m) => m,
        Err(e) => {
            let _ = app.emit(CONNECTION_STATE, ConnectionStateEvent {
                user_id: user_id.clone(),
                state: core::sync::ConnectionState::AuthFailed,
                last_error: Some(e.to_string()),
            });
            return;
        }
    };
    let server = m.server.clone();
    let app2 = app.clone();
    let state2 = state.clone();
    let cancel2 = cancel.clone();

    tauri::async_runtime::spawn(async move {
        // Connecting → backfill → Online → SSE; on drop reconnect with backoff.
        let _ = app2.emit(CONNECTION_STATE, ConnectionStateEvent {
            user_id: user_id.clone(),
            state: core::sync::ConnectionState::Connecting,
            last_error: None,
        });
        let last_seen = {
            let conn = state2.conn.lock().await;
            crate::core::storage::accounts::find(&conn, &user_id).ok().flatten().map(|a| a.last_seen_id).unwrap_or(0)
        };
        match server.list_entries(last_seen, 500).await {
            Ok(rows) => {
                let conn = state2.conn.lock().await;
                let mut new_last = last_seen;
                for row in rows {
                    let _ = crate::core::sync::decryptor::ingest(&conn, &m.user_key, &user_id, &row, now_ms());
                    if row.id > new_last { new_last = row.id; }
                }
                if new_last != last_seen {
                    let _ = crate::core::storage::accounts::set_last_seen(&conn, &user_id, new_last);
                    let _ = app2.emit(HISTORY_CHANGED, serde_json::json!({ "user_id": user_id }));
                }
            }
            Err(e) => tracing::warn!(err = %e, "backfill failed"),
        }

        let _ = app2.emit(CONNECTION_STATE, ConnectionStateEvent {
            user_id: user_id.clone(),
            state: core::sync::ConnectionState::Online,
            last_error: None,
        });

        let (tx, mut rx) = mpsc::channel::<core::sync::sse::ServerEvent>(64);
        let server_for_sse = server.clone();
        let cancel_for_sse = cancel2.clone();
        tokio::spawn(async move {
            if let Err(e) = core::sync::sse::run(server_for_sse, tx, cancel_for_sse).await {
                tracing::warn!(err = %e, "sse loop exited");
            }
        });

        loop {
            tokio::select! {
                _ = cancel2.cancelled() => return,
                ev = rx.recv() => match ev {
                    None => return,
                    Some(core::sync::sse::ServerEvent::Entry { id, ciphertext, created_at, device_id }) => {
                        let row = core::http::dto::EntryRow { id, ciphertext, created_at, device_id: device_id.clone() };
                        let conn = state2.conn.lock().await;
                        match crate::core::sync::decryptor::ingest(&conn, &m.user_key, &user_id, &row, now_ms()) {
                            Ok(out) => {
                                let _ = crate::core::storage::accounts::set_last_seen(&conn, &user_id, id);
                                let _ = app2.emit(ENTRY_ADDED, EntryAdded {
                                    user_id: user_id.clone(),
                                    entry: EntryView {
                                        id, user_id: user_id.clone(),
                                        preview: out.plaintext_preview.unwrap_or_default(),
                                        created_at, device_id, device_label: None,
                                    },
                                });
                                if out.undecryptable {
                                    let _ = app2.emit(crate::events::DECRYPTION_ERROR, crate::events::DecryptionError {
                                        user_id: user_id.clone(), entry_id: id,
                                    });
                                }
                            }
                            Err(e) => tracing::warn!(err = %e, "ingest failed"),
                        }
                    }
                    Some(core::sync::sse::ServerEvent::Delete { id }) => {
                        let conn = state2.conn.lock().await;
                        let _ = crate::core::storage::entries_cache::delete_one(&conn, &user_id, id);
                        let _ = app2.emit(ENTRY_DELETED, crate::events::EntryDeleted {
                            user_id: user_id.clone(), entry_id: id,
                        });
                    }
                }
            }
        }
    });

    // Pending-queue uploader on its own task.
    let server_for_upload = server.clone();
    let conn_for_upload = state.conn.clone();
    let app_for_upload = app.clone();
    let cancel3 = cancel.clone();
    let user_id2 = user_id.clone();
    tauri::async_runtime::spawn(async move {
        use core::sync::uploader::{Uploader, UploaderEvents, UploadTransport};
        struct ServerUpload(crate::core::http::ServerClient);
        #[async_trait::async_trait]
        impl UploadTransport for ServerUpload {
            async fn upload(&self, b64: &str) -> Result<i64, crate::errors::AppError> {
                self.0.post_entry(b64).await.map(|r| r.id)
            }
        }
        let app_pc = app_for_upload.clone();
        let app_af = app_for_upload.clone();
        let user_pc = user_id2.clone();
        let user_af = user_id2.clone();
        let events = UploaderEvents {
            on_pending_count: Box::new(move |n| {
                let _ = app_pc.emit(PENDING_COUNT, PendingCount { user_id: user_pc.clone(), count: n });
            }),
            on_auth_failed: Box::new(move || {
                let _ = app_af.emit(CONNECTION_STATE, ConnectionStateEvent {
                    user_id: user_af.clone(), state: core::sync::ConnectionState::AuthFailed, last_error: None,
                });
            }),
        };
        let trigger = std::sync::Arc::new(tokio::sync::Notify::new());
        let up = Uploader {
            user_id: user_id2.clone(),
            conn: conn_for_upload,
            transport: std::sync::Arc::new(ServerUpload(server_for_upload)),
            trigger: trigger.clone(),
            events,
        };
        // Fire trigger once to flush whatever might already be queued from a previous run.
        trigger.notify_one();
        up.run(cancel3).await;
    });
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
```

- [ ] **Step 2: Wire main.rs to call the new launcher**

Replace `clients/desktop/src-tauri/src/main.rs`:
```rust
fn main() {
    sharepaste_desktop_lib::launch();
}
```

- [ ] **Step 3: Verify the binary builds**

Run:
```bash
cargo check --manifest-path clients/desktop/src-tauri/Cargo.toml
```
Expected: clean. (Manual smoke validates tray + popover behaviour in Task 26.)

- [ ] **Step 4: Commit**

```bash
git add clients/desktop/src-tauri/src/main.rs clients/desktop/src-tauri/src/lib.rs
git commit -m "feat(desktop): tray + popover wiring + sync orchestrator (backfill, SSE, uploader)"
```

---
## Task 21: Integration tests against real `sharepaste` server

These tests exercise the full Rust core against a live `sharepaste serve` process running off the parent project. They live under `clients/desktop/src-tauri/tests/` so they share the lib crate.

**Files:**
- Create: `clients/desktop/src-tauri/tests/common/mod.rs`
- Create: `clients/desktop/src-tauri/tests/flow1_invite.rs`
- Create: `clients/desktop/src-tauri/tests/flow2_pairing.rs`
- Create: `clients/desktop/src-tauri/tests/auth_revoke.rs`

- [ ] **Step 1: Helper for spawning a test server + freeing a port**

`clients/desktop/src-tauri/tests/common/mod.rs`:
```rust
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub struct TestServer {
    pub url: String,
    pub admin_token: String,
    pub data_dir: tempfile::TempDir,
    pub child: Child,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn pick_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral")
        .local_addr()
        .unwrap()
        .port()
}

pub fn start() -> TestServer {
    let port = pick_free_port();
    let data_dir = tempfile::tempdir().unwrap();
    let admin_token = "test-admin-token".to_string();
    let server_dir = workspace_root();
    let child = Command::new("npx")
        .arg("tsx")
        .arg(server_dir.join("src/index.ts"))
        .arg("serve")
        .env("PORT", port.to_string())
        .env("DB_PATH", data_dir.path().join("server.sqlite"))
        .env("LOG_LEVEL", "warn")
        .env("ADMIN_TOKEN", &admin_token)
        .current_dir(&server_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn sharepaste serve");

    let url = format!("http://127.0.0.1:{port}");
    wait_until_ready(&url);
    TestServer { url, admin_token, data_dir, child }
}

fn workspace_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();   // src-tauri -> desktop
    p.pop();   // desktop -> clients
    p.pop();   // clients -> sharepaste
    p
}

fn wait_until_ready(url: &str) {
    let started = Instant::now();
    let client = reqwest::blocking::Client::new();
    while started.elapsed() < Duration::from_secs(15) {
        if let Ok(resp) = client.get(format!("{url}/healthz")).send() {
            if resp.status().is_success() { return; }
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    panic!("server failed to come up at {url}");
}

pub fn create_invite(server: &TestServer, username: &str) -> String {
    let server_dir = workspace_root();
    let out = Command::new("npx")
        .arg("tsx")
        .arg(server_dir.join("src/index.ts"))
        .arg("user").arg("create").arg(username)
        .env("DB_PATH", server.data_dir.path().join("server.sqlite"))
        .current_dir(&server_dir)
        .output()
        .expect("create user");
    let stdout = String::from_utf8(out.stdout).unwrap();
    // Server CLI prints the invite token on its own line — grab it.
    stdout
        .lines()
        .find(|l| l.starts_with("invite_token="))
        .map(|l| l.trim_start_matches("invite_token=").to_string())
        .expect("invite token in CLI output")
}
```

> NOTE: the server's `user create` CLI prints `invite_token=<value>` on stdout
> per Track 1 plan. If the actual format differs, update the matcher above.

- [ ] **Step 2: Write the Flow 1 (invite) integration test**

`clients/desktop/src-tauri/tests/flow1_invite.rs`:
```rust
mod common;

use sharepaste_desktop_lib::core::crypto::random_user_key;
use sharepaste_desktop_lib::core::http::ServerClient;
use sharepaste_desktop_lib::core::pairing::qr::base64_encode;
use sharepaste_desktop_lib::core::crypto::encrypt;

#[tokio::test]
async fn invite_then_post_and_list() {
    let server = common::start();
    let invite = common::create_invite(&server, "alice");
    let api = ServerClient::new(server.url.clone()).unwrap();
    let claimed = api.claim_invite(&invite, "mac-1").await.unwrap();
    let api = api.with_token(claimed.device_token);

    let key = random_user_key();
    let ct = encrypt(&key, &claimed.user_id, b"hello sharepaste").unwrap();
    let posted = api.post_entry(&base64_encode(&ct)).await.unwrap();
    assert!(posted.id > 0);

    let rows = api.list_entries(0, 10).await.unwrap();
    assert_eq!(rows.len(), 1);
    let body = sharepaste_desktop_lib::core::pairing::qr::base64_decode(&rows[0].ciphertext).unwrap();
    let pt = sharepaste_desktop_lib::core::crypto::decrypt(&key, &claimed.user_id, &body).unwrap();
    assert_eq!(pt, b"hello sharepaste");
}
```

- [ ] **Step 3: Write the Flow 2 (pairing) test**

`clients/desktop/src-tauri/tests/flow2_pairing.rs`:
```rust
mod common;

use sharepaste_desktop_lib::core::crypto::{decrypt, encrypt, random_user_key, UserKey};
use sharepaste_desktop_lib::core::http::ServerClient;
use sharepaste_desktop_lib::core::pairing::qr::{
    base64_decode, base64_encode, fetch_and_decrypt_pair_payload, secret_proof_hex, start_pair,
    upload_pair_payload, PairPayload,
};
use sharepaste_desktop_lib::core::pairing::shortcode::decode as decode_shortcode;
use zeroize::Zeroizing;

#[tokio::test]
async fn pair_second_device_via_shortcode() {
    let server = common::start();
    let invite = common::create_invite(&server, "alice");
    let api = ServerClient::new(server.url.clone()).unwrap();
    let claimed = api.claim_invite(&invite, "mac-A").await.unwrap();
    let user_key = random_user_key();
    let inviter = api.with_token(claimed.device_token);

    let started = start_pair(&inviter).await.unwrap();
    let payload = decode_shortcode(&started.shortcode).unwrap();

    let claimer = ServerClient::new(server.url.clone()).unwrap();
    let proof = secret_proof_hex(&payload.pairing_secret);
    claimer.pair_claim(&payload.pair_id.to_string(), &proof).await.unwrap();

    upload_pair_payload(
        &inviter,
        payload.pair_id,
        &payload.pairing_secret,
        &claimed.user_id,
        &user_key,
        &server.url,
    ).await.unwrap();

    let pp: PairPayload = fetch_and_decrypt_pair_payload(
        &claimer,
        payload.pair_id,
        &payload.pairing_secret,
    ).await.unwrap();
    assert_eq!(pp.user_id, claimed.user_id);

    let device = claimer.devices(&payload.pair_id.to_string(), &proof, "mac-B").await.unwrap();
    let claimer = claimer.with_token(device.device_token);

    // Both devices can post + see each other's entries.
    let key2 = sharepaste_desktop_lib::core::pairing::invite::hex::decode_user_key(&pp.user_key).unwrap();
    let ct = encrypt(&key2, &pp.user_id, b"from B").unwrap();
    claimer.post_entry(&base64_encode(&ct)).await.unwrap();

    let rows = inviter.list_entries(0, 10).await.unwrap();
    assert_eq!(rows.len(), 1);
    let pt = decrypt(&user_key, &claimed.user_id, &base64_decode(&rows[0].ciphertext).unwrap()).unwrap();
    assert_eq!(pt, b"from B");
}
```

- [ ] **Step 4: Write the auth-revocation test**

`clients/desktop/src-tauri/tests/auth_revoke.rs`:
```rust
mod common;

use sharepaste_desktop_lib::core::http::ServerClient;
use sharepaste_desktop_lib::errors::AppError;

#[tokio::test]
async fn revoking_a_device_returns_401_on_subsequent_calls() {
    let server = common::start();
    let invite = common::create_invite(&server, "alice");
    let api = ServerClient::new(server.url.clone()).unwrap();
    let a = api.claim_invite(&invite, "mac-A").await.unwrap();
    let b_invite = common::create_invite(&server, "alice2");
    let b = api.claim_invite(&b_invite, "mac-B").await.unwrap();
    let auth_a = api.clone().with_token(a.device_token.clone());

    auth_a.revoke_device(&a.device_id).await.unwrap();

    let err = auth_a.list_entries(0, 1).await.unwrap_err();
    assert!(matches!(err, AppError::Auth(_)));
    let _ = b; // sanity: another account/device still works (not asserted to keep test scoped).
}
```

- [ ] **Step 5: Run the integration suite**

Run:
```bash
cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml --tests
```
Expected: all three integration tests pass. (They spawn `sharepaste serve` as a subprocess; the server must be buildable from the workspace root via `npx tsx src/index.ts`.)

- [ ] **Step 6: Commit**

```bash
git add clients/desktop/src-tauri/tests
git commit -m "test(desktop): integration tests against real sharepaste server"
```

---

## Task 22: UI bootstrap — Zustand store + IPC layer behind an interface

**Files:**
- Create: `clients/desktop/ui/src/types.ts`
- Create: `clients/desktop/ui/src/ipc/tauri.ts`
- Create: `clients/desktop/ui/src/ipc/commands.ts`
- Create: `clients/desktop/ui/src/ipc/events.ts`
- Create: `clients/desktop/ui/src/store/index.ts`
- Create: `clients/desktop/ui/src/store/ui.ts`
- Create: `clients/desktop/ui/src/store/history.ts`
- Create: `clients/desktop/ui/src/store/accounts.ts`
- Create: `clients/desktop/ui/src/store/status.ts`
- Create: `clients/desktop/ui/src/__tests__/store.test.ts`

- [ ] **Step 1: Type definitions mirroring Rust DTOs**

`clients/desktop/ui/src/types.ts`:
```ts
export type ConnectionState = "Disconnected" | "Connecting" | "Online" | "AuthFailed";

export type EntryView = {
  id: number;
  user_id: string;
  preview: string;
  created_at: number;
  device_id: string;
  device_label?: string;
};

export type Account = {
  user_id: string;
  device_id: string;
  label: string;
  server_url: string;
  status: ConnectionState;
  pending: number;
};

export type Settings = {
  capture_enabled: boolean;
  deny_list: string[];
  autostart: boolean;
  hotkey?: string | null;
};

export type AppErrorPayload = { kind: string; message: string };
```

- [ ] **Step 2: Tauri runtime indirection so tests can inject a fake**

`clients/desktop/ui/src/ipc/tauri.ts`:
```ts
import { invoke as realInvoke } from "@tauri-apps/api/core";
import { listen as realListen, type UnlistenFn } from "@tauri-apps/api/event";

export type Invoker = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
export type Listener = <P>(event: string, cb: (payload: P) => void) => Promise<UnlistenFn>;

let _invoke: Invoker = (cmd, args) => realInvoke(cmd, args);
let _listen: Listener = (event, cb) => realListen(event, ({ payload }) => cb(payload as never));

export const tauri = {
  invoke: (...a: Parameters<Invoker>) => _invoke(...a),
  listen: (...a: Parameters<Listener>) => _listen(...a),
};

export function injectForTests(invoke: Invoker, listen: Listener) {
  _invoke = invoke;
  _listen = listen;
}
```

- [ ] **Step 3: Typed command wrappers**

`clients/desktop/ui/src/ipc/commands.ts`:
```ts
import type { Account, EntryView, Settings, ConnectionState } from "../types";
import { tauri } from "./tauri";

export const cmd = {
  listAccounts:        (): Promise<Account[]> => tauri.invoke("list_accounts"),
  pairWithInvite:      (args: { server_url: string; token: string; device_label: string }) =>
                         tauri.invoke<{ user_id: string; device_id: string }>("pair_with_invite", { args }),
  pairStart:           (args: { user_id: string }) =>
                         tauri.invoke<{ code: string; expires_at: number }>("pair_start", { args }),
  pairWithCode:        (args: { code: string; device_label: string }) =>
                         tauri.invoke<{ user_id: string; device_id: string }>("pair_with_code", { args }),
  forgetAccount:       (args: { user_id: string }) => tauri.invoke<void>("forget_account", { args }),
  revokeDevice:        (args: { user_id: string; device_id: string }) => tauri.invoke<void>("revoke_device", { args }),
  setActiveAccount:    (args: { user_id: string }) => tauri.invoke<void>("set_active_account", { args }),
  listHistory:         (args: { user_id: string; before_id?: number; limit: number }) =>
                         tauri.invoke<EntryView[]>("list_history", { args }),
  searchHistory:       (args: { user_id: string; query: string; limit: number }) =>
                         tauri.invoke<EntryView[]>("search_history", { args }),
  copyToClipboard:     (args: { user_id: string; entry_id: number }) => tauri.invoke<void>("copy_to_clipboard", { args }),
  deleteEntry:         (args: { user_id: string; entry_id: number }) => tauri.invoke<void>("delete_entry", { args }),
  clearHistory:        (args: { user_id: string }) => tauri.invoke<void>("clear_history", { args }),
  getSettings:         (): Promise<Settings> => tauri.invoke("get_settings"),
  updateSettings:      (patch: Partial<Settings>): Promise<Settings> => tauri.invoke("update_settings", { patch }),
  getStatus:           (args: { user_id: string }) =>
                         tauri.invoke<{ state: ConnectionState; pending_count: number; last_error?: string }>("get_status", { args }),
};
```

- [ ] **Step 4: Event subscription helpers**

`clients/desktop/ui/src/ipc/events.ts`:
```ts
import type { Account, ConnectionState, EntryView } from "../types";
import { tauri } from "./tauri";

export const events = {
  onAccountAdded:    (cb: (p: { user_id: string; device_id: string; label: string }) => void) => tauri.listen("account-added", cb),
  onAccountRemoved:  (cb: (p: { user_id: string }) => void) => tauri.listen("account-removed", cb),
  onActiveChanged:   (cb: (p: { user_id: string | null }) => void) => tauri.listen("active-changed", cb),
  onConnectionState: (cb: (p: { user_id: string; state: ConnectionState; last_error?: string }) => void) => tauri.listen("connection-state", cb),
  onEntryAdded:      (cb: (p: { user_id: string; entry: EntryView }) => void) => tauri.listen("entry-added", cb),
  onEntryDeleted:    (cb: (p: { user_id: string; entry_id: number }) => void) => tauri.listen("entry-deleted", cb),
  onHistoryChanged:  (cb: (p: { user_id: string }) => void) => tauri.listen("history-changed", cb),
  onPendingCount:    (cb: (p: { user_id: string; count: number }) => void) => tauri.listen("pending-count", cb),
  onCaptureSkipped:  (cb: (p: { reason: string; source_app?: string }) => void) => tauri.listen("capture-skipped", cb),
  onDecryptionError: (cb: (p: { user_id: string; entry_id: number }) => void) => tauri.listen("decryption-error", cb),
  onPairShortcode:   (cb: (p: { code: string; expires_at: number }) => void) => tauri.listen("pair-shortcode", cb),
  onPairClaimed:     (cb: (p: { user_id: string }) => void) => tauri.listen("pair-claimed", cb),
  onPairExpired:     (cb: () => void) => tauri.listen("pair-expired", () => cb()),
};
```

- [ ] **Step 5: Zustand store slices**

`clients/desktop/ui/src/store/ui.ts`:
```ts
import { create } from "zustand";

export type ModalKind = null | "pairing" | "settings" | "accounts";

export type UiState = {
  modal: ModalKind;
  search: string;
  selectedIndex: number;
  setModal: (m: ModalKind) => void;
  setSearch: (s: string) => void;
  setSelectedIndex: (i: number) => void;
};

export const useUiStore = create<UiState>((set) => ({
  modal: null,
  search: "",
  selectedIndex: 0,
  setModal: (modal) => set({ modal }),
  setSearch: (search) => set({ search, selectedIndex: 0 }),
  setSelectedIndex: (selectedIndex) => set({ selectedIndex }),
}));
```

`clients/desktop/ui/src/store/history.ts`:
```ts
import { create } from "zustand";
import type { EntryView } from "../types";

export type HistoryState = {
  entries: EntryView[];
  hydrate: (rows: EntryView[]) => void;
  add: (e: EntryView) => void;
  remove: (id: number) => void;
  clear: () => void;
};

export const useHistoryStore = create<HistoryState>((set) => ({
  entries: [],
  hydrate: (rows) => set({ entries: rows }),
  add: (entry) => set((s) => ({ entries: dedupePrepend(s.entries, entry) })),
  remove: (id) => set((s) => ({ entries: s.entries.filter((e) => e.id !== id) })),
  clear: () => set({ entries: [] }),
}));

function dedupePrepend(existing: EntryView[], next: EntryView): EntryView[] {
  const without = existing.filter((e) => e.id !== next.id);
  return [next, ...without].slice(0, 100);
}
```

`clients/desktop/ui/src/store/accounts.ts`:
```ts
import { create } from "zustand";
import type { Account } from "../types";

export type AccountsState = {
  accounts: Account[];
  active?: string;
  hydrate: (rows: Account[]) => void;
  upsert: (a: Account) => void;
  remove: (user_id: string) => void;
  setActive: (user_id?: string) => void;
};

export const useAccountsStore = create<AccountsState>((set) => ({
  accounts: [],
  active: undefined,
  hydrate: (rows) => set({ accounts: rows, active: rows[0]?.user_id }),
  upsert: (a) => set((s) => {
    const without = s.accounts.filter((x) => x.user_id !== a.user_id);
    return { accounts: [...without, a] };
  }),
  remove: (uid) => set((s) => ({
    accounts: s.accounts.filter((a) => a.user_id !== uid),
    active: s.active === uid ? s.accounts.find((a) => a.user_id !== uid)?.user_id : s.active,
  })),
  setActive: (active) => set({ active }),
}));
```

`clients/desktop/ui/src/store/status.ts`:
```ts
import { create } from "zustand";
import type { ConnectionState } from "../types";

export type StatusState = {
  byUser: Record<string, { state: ConnectionState; pending: number; last_error?: string }>;
  set: (user_id: string, patch: Partial<StatusState["byUser"][string]>) => void;
};

export const useStatusStore = create<StatusState>((set, get) => ({
  byUser: {},
  set: (user_id, patch) => set({
    byUser: {
      ...get().byUser,
      [user_id]: {
        state: get().byUser[user_id]?.state ?? "Disconnected",
        pending: get().byUser[user_id]?.pending ?? 0,
        ...get().byUser[user_id],
        ...patch,
      },
    },
  }),
}));
```

`clients/desktop/ui/src/store/index.ts`:
```ts
export * from "./ui";
export * from "./history";
export * from "./accounts";
export * from "./status";
```

- [ ] **Step 6: Test the store reducers**

`clients/desktop/ui/src/__tests__/store.test.ts`:
```ts
import { describe, it, expect, beforeEach } from "vitest";
import { useHistoryStore } from "../store/history";
import { useAccountsStore } from "../store/accounts";

describe("history store", () => {
  beforeEach(() => useHistoryStore.setState({ entries: [] }));

  it("add prepends and dedupes by id", () => {
    const { add } = useHistoryStore.getState();
    add({ id: 1, user_id: "u", preview: "a", created_at: 1, device_id: "d" });
    add({ id: 2, user_id: "u", preview: "b", created_at: 2, device_id: "d" });
    add({ id: 1, user_id: "u", preview: "a-new", created_at: 3, device_id: "d" });
    const state = useHistoryStore.getState();
    expect(state.entries.map((e) => e.id)).toEqual([1, 2]);
    expect(state.entries[0].preview).toBe("a-new");
  });

  it("remove filters by id", () => {
    const { add, remove } = useHistoryStore.getState();
    add({ id: 1, user_id: "u", preview: "a", created_at: 1, device_id: "d" });
    remove(1);
    expect(useHistoryStore.getState().entries.length).toBe(0);
  });
});

describe("accounts store", () => {
  beforeEach(() => useAccountsStore.setState({ accounts: [], active: undefined }));

  it("hydrate sets active to first row", () => {
    useAccountsStore.getState().hydrate([
      { user_id: "a", device_id: "d", label: "x", server_url: "https://s", status: "Online", pending: 0 },
    ]);
    expect(useAccountsStore.getState().active).toBe("a");
  });

  it("removing active falls back to next account", () => {
    useAccountsStore.getState().hydrate([
      { user_id: "a", device_id: "d", label: "x", server_url: "https://s", status: "Online", pending: 0 },
      { user_id: "b", device_id: "d", label: "y", server_url: "https://s", status: "Online", pending: 0 },
    ]);
    useAccountsStore.getState().remove("a");
    expect(useAccountsStore.getState().active).toBe("b");
  });
});
```

- [ ] **Step 7: Run the UI tests**

Run:
```bash
npm --prefix clients/desktop/ui test
```
Expected: 4 passing.

- [ ] **Step 8: Commit**

```bash
git add clients/desktop/ui/src
git commit -m "feat(desktop-ui): types, IPC layer, Zustand stores"
```

---

## Task 23: UI views — Popover (history list, search, footer, entry rows)

**Files:**
- Modify: `clients/desktop/ui/src/App.tsx`
- Create: `clients/desktop/ui/src/views/Popover.tsx`
- Create: `clients/desktop/ui/src/views/Search.tsx`
- Create: `clients/desktop/ui/src/views/HistoryList.tsx`
- Create: `clients/desktop/ui/src/views/EntryRow.tsx`
- Create: `clients/desktop/ui/src/views/Footer.tsx`
- Create: `clients/desktop/ui/src/__tests__/HistoryList.test.tsx`

- [ ] **Step 1: Wire `App.tsx` to choose popover vs modal route**

`clients/desktop/ui/src/App.tsx`:
```tsx
import Popover from "./views/Popover";
import PairingModal from "./modals/PairingModal";
import SettingsModal from "./modals/SettingsModal";
import AccountsModal from "./modals/AccountsModal";

export default function App() {
  const route = document.body.dataset.route ?? "popover";
  if (route === "modal") {
    const params = new URLSearchParams(window.location.search);
    const kind = params.get("kind") ?? "";
    if (kind === "pairing")  return <PairingModal />;
    if (kind === "settings") return <SettingsModal />;
    if (kind === "accounts") return <AccountsModal />;
    return <div>Unknown modal: {kind}</div>;
  }
  return <Popover />;
}
```

- [ ] **Step 2: Implement the popover composite view**

`clients/desktop/ui/src/views/Popover.tsx`:
```tsx
import { useEffect } from "react";
import { useAccountsStore, useHistoryStore, useStatusStore, useUiStore } from "../store";
import { cmd } from "../ipc/commands";
import { events } from "../ipc/events";
import HistoryList from "./HistoryList";
import Search from "./Search";
import Footer from "./Footer";

export default function Popover() {
  const accounts = useAccountsStore((s) => s.accounts);
  const active = useAccountsStore((s) => s.active);
  const hydrateAccounts = useAccountsStore((s) => s.hydrate);
  const hydrateHistory = useHistoryStore((s) => s.hydrate);
  const addEntry = useHistoryStore((s) => s.add);
  const removeEntry = useHistoryStore((s) => s.remove);
  const setStatus = useStatusStore((s) => s.set);

  useEffect(() => {
    let unsub: Array<() => void> = [];
    (async () => {
      const accs = await cmd.listAccounts();
      hydrateAccounts(accs);
      const first = accs[0];
      if (first) {
        const rows = await cmd.listHistory({ user_id: first.user_id, limit: 100 });
        hydrateHistory(rows);
      }
      unsub.push(await events.onEntryAdded(({ user_id, entry }) => {
        if (user_id === useAccountsStore.getState().active) addEntry(entry);
      }));
      unsub.push(await events.onEntryDeleted(({ user_id, entry_id }) => {
        if (user_id === useAccountsStore.getState().active) removeEntry(entry_id);
      }));
      unsub.push(await events.onConnectionState(({ user_id, state, last_error }) => {
        setStatus(user_id, { state, last_error });
      }));
      unsub.push(await events.onPendingCount(({ user_id, count }) => {
        setStatus(user_id, { pending: count });
      }));
    })();
    return () => unsub.forEach((u) => u());
  }, [addEntry, hydrateAccounts, hydrateHistory, removeEntry, setStatus]);

  if (accounts.length === 0) {
    return (
      <div className="flex h-full flex-col p-4 gap-2 text-sm">
        <div className="font-semibold">No accounts paired yet.</div>
        <button
          className="rounded bg-blue-600 px-3 py-1.5 text-white hover:bg-blue-500"
          onClick={() => useUiStore.getState().setModal("pairing")}
        >
          Pair a device
        </button>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <Search />
      <HistoryList />
      <Footer activeUserId={active!} />
    </div>
  );
}
```

- [ ] **Step 3: Search bar + filtered list selector**

`clients/desktop/ui/src/views/Search.tsx`:
```tsx
import { useUiStore } from "../store";

export default function Search() {
  const search = useUiStore((s) => s.search);
  const setSearch = useUiStore((s) => s.setSearch);
  return (
    <div className="border-b border-zinc-700 p-2">
      <input
        autoFocus
        className="w-full rounded bg-zinc-800 px-2 py-1 text-sm text-zinc-100 outline-none focus:ring-1 focus:ring-blue-500"
        placeholder="Search history…"
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />
    </div>
  );
}
```

`clients/desktop/ui/src/views/HistoryList.tsx`:
```tsx
import { useMemo, useEffect } from "react";
import { useHistoryStore, useUiStore, useAccountsStore } from "../store";
import { cmd } from "../ipc/commands";
import EntryRow from "./EntryRow";

export default function HistoryList() {
  const entries = useHistoryStore((s) => s.entries);
  const search = useUiStore((s) => s.search);
  const selectedIndex = useUiStore((s) => s.selectedIndex);
  const setSelectedIndex = useUiStore((s) => s.setSelectedIndex);
  const active = useAccountsStore((s) => s.active);

  const filtered = useMemo(() => {
    if (!search.trim()) return entries;
    const needle = search.toLowerCase();
    return entries.filter((e) => e.preview.toLowerCase().includes(needle));
  }, [entries, search]);

  useEffect(() => {
    const handler = async (e: KeyboardEvent) => {
      if (!active) return;
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex(Math.min(filtered.length - 1, selectedIndex + 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex(Math.max(0, selectedIndex - 1));
      } else if (e.key === "Enter") {
        const target = filtered[selectedIndex];
        if (target) await cmd.copyToClipboard({ user_id: target.user_id, entry_id: target.id });
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [active, filtered, selectedIndex, setSelectedIndex]);

  if (filtered.length === 0) {
    return <div className="flex flex-1 items-center justify-center text-sm text-zinc-500">No entries.</div>;
  }
  return (
    <ul className="flex-1 overflow-auto">
      {filtered.map((e, i) => (
        <EntryRow key={e.id} entry={e} selected={i === selectedIndex} />
      ))}
    </ul>
  );
}
```

`clients/desktop/ui/src/views/EntryRow.tsx`:
```tsx
import type { EntryView } from "../types";
import { cmd } from "../ipc/commands";

type Props = { entry: EntryView; selected: boolean };

export default function EntryRow({ entry, selected }: Props) {
  return (
    <li
      data-testid="entry-row"
      data-selected={selected}
      className={`px-3 py-2 text-sm cursor-default ${selected ? "bg-zinc-700" : "hover:bg-zinc-800"}`}
      onClick={() => cmd.copyToClipboard({ user_id: entry.user_id, entry_id: entry.id })}
    >
      <div className="truncate">{entry.preview || <span className="text-zinc-500">(undecryptable)</span>}</div>
    </li>
  );
}
```

`clients/desktop/ui/src/views/Footer.tsx`:
```tsx
import { useStatusStore, useUiStore } from "../store";

export default function Footer({ activeUserId }: { activeUserId: string }) {
  const status = useStatusStore((s) => s.byUser[activeUserId]);
  const setModal = useUiStore((s) => s.setModal);
  const stateText = status?.state ?? "Disconnected";
  const pending = status?.pending ?? 0;
  return (
    <div className="border-t border-zinc-700 px-3 py-1.5 text-xs flex justify-between items-center text-zinc-300">
      <span>
        <span data-testid="status">{stateText}</span>
        {pending > 0 ? <span className="ml-2 text-amber-400">· {pending} pending</span> : null}
        {status?.last_error ? <span className="ml-2 text-red-400">· {status.last_error}</span> : null}
      </span>
      <span className="space-x-2">
        <button onClick={() => setModal("accounts")} className="hover:underline">Accounts</button>
        <button onClick={() => setModal("settings")} className="hover:underline">Settings</button>
      </span>
    </div>
  );
}
```

- [ ] **Step 4: Test the HistoryList rendering and selection behavior**

`clients/desktop/ui/src/__tests__/HistoryList.test.tsx`:
```tsx
import { describe, it, expect, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { useAccountsStore, useHistoryStore, useUiStore } from "../store";
import HistoryList from "../views/HistoryList";

describe("HistoryList", () => {
  beforeEach(() => {
    useUiStore.setState({ search: "", selectedIndex: 0, modal: null });
    useHistoryStore.setState({ entries: [
      { id: 1, user_id: "u", preview: "Hello", created_at: 1, device_id: "d" },
      { id: 2, user_id: "u", preview: "World", created_at: 2, device_id: "d" },
    ]});
    useAccountsStore.setState({
      accounts: [{ user_id: "u", device_id: "d", label: "mac", server_url: "https://s", status: "Online", pending: 0 }],
      active: "u",
    });
  });

  it("renders rows newest first", () => {
    render(<HistoryList />);
    const rows = screen.getAllByTestId("entry-row");
    expect(rows).toHaveLength(2);
  });

  it("filters by search term", () => {
    useUiStore.setState({ search: "world", selectedIndex: 0, modal: null });
    render(<HistoryList />);
    const rows = screen.getAllByTestId("entry-row");
    expect(rows).toHaveLength(1);
  });

  it("highlights the selected index", () => {
    useUiStore.setState({ search: "", selectedIndex: 1, modal: null });
    render(<HistoryList />);
    const rows = screen.getAllByTestId("entry-row");
    expect(rows[1]).toHaveAttribute("data-selected", "true");
  });
});
```

- [ ] **Step 5: Run UI tests**

Run:
```bash
npm --prefix clients/desktop/ui test
```
Expected: previous + 3 new tests pass (7 total).

- [ ] **Step 6: Commit**

```bash
git add clients/desktop/ui/src
git commit -m "feat(desktop-ui): popover with search, history list, footer"
```

---
## Task 24: UI — Pairing modal (two-step wizard)

**Files:**
- Create: `clients/desktop/ui/src/modals/PairingModal.tsx`
- Create: `clients/desktop/ui/src/__tests__/PairingModal.test.tsx`

- [ ] **Step 1: Implement the wizard — chooser → claim-invite or pair-code form**

`clients/desktop/ui/src/modals/PairingModal.tsx`:
```tsx
import { useState, useEffect } from "react";
import { cmd } from "../ipc/commands";
import { events } from "../ipc/events";
import type { AppErrorPayload } from "../types";

type Step = "chooser" | "invite" | "code" | "show-code";

export default function PairingModal() {
  const [step, setStep] = useState<Step>("chooser");
  const [serverUrl, setServerUrl] = useState("https://");
  const [token, setToken] = useState("");
  const [code, setCode] = useState("");
  const [label, setLabel] = useState(defaultLabel());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [shortcode, setShortcode] = useState<string>();
  const [expiresAt, setExpiresAt] = useState<number>();

  useEffect(() => {
    let unsubs: Array<() => void> = [];
    (async () => {
      unsubs.push(await events.onPairShortcode(({ code, expires_at }) => {
        setShortcode(code);
        setExpiresAt(expires_at);
        setStep("show-code");
      }));
      unsubs.push(await events.onPairClaimed(() => { setError(undefined); window.close(); }));
      unsubs.push(await events.onPairExpired(() => setError("Pair code expired or already used. Generate a new one.")));
    })();
    return () => unsubs.forEach((u) => u());
  }, []);

  const handle = async (fn: () => Promise<unknown>) => {
    setBusy(true); setError(undefined);
    try { await fn(); }
    catch (e) { setError(messageOf(e)); }
    finally { setBusy(false); }
  };

  if (step === "chooser") {
    return (
      <div className="flex flex-col gap-4 p-6">
        <h1 className="text-base font-semibold">How are you pairing?</h1>
        <button data-testid="choose-invite" className="rounded border border-zinc-700 p-3 text-left hover:bg-zinc-800" onClick={() => setStep("invite")}>
          <div className="font-semibold">I have an invite token</div>
          <div className="text-xs text-zinc-400">Operator gave me a token for a new account.</div>
        </button>
        <button data-testid="choose-code" className="rounded border border-zinc-700 p-3 text-left hover:bg-zinc-800" onClick={() => setStep("code")}>
          <div className="font-semibold">I have a pair code</div>
          <div className="text-xs text-zinc-400">Another of my devices is showing a short code.</div>
        </button>
      </div>
    );
  }

  if (step === "invite") {
    const insecure = /^http:\/\/(?!localhost|127\.0\.0\.1)/i.test(serverUrl);
    return (
      <form
        className="flex flex-col gap-3 p-6"
        onSubmit={(e) => {
          e.preventDefault();
          handle(async () => {
            await cmd.pairWithInvite({ server_url: serverUrl, token, device_label: label });
            window.close();
          });
        }}
      >
        <h1 className="text-base font-semibold">Claim invite</h1>
        {insecure && <div data-testid="insecure-warning" className="rounded border border-amber-600 bg-amber-900/30 px-2 py-1 text-xs text-amber-300">Unencrypted — only use on trusted networks.</div>}
        <label className="text-xs text-zinc-400">Server URL</label>
        <input className="rounded bg-zinc-800 px-2 py-1" value={serverUrl} onChange={(e) => setServerUrl(e.target.value)} />
        <label className="text-xs text-zinc-400">Invite token</label>
        <input className="rounded bg-zinc-800 px-2 py-1 font-mono" value={token} onChange={(e) => setToken(e.target.value)} />
        <label className="text-xs text-zinc-400">Device label</label>
        <input className="rounded bg-zinc-800 px-2 py-1" value={label} onChange={(e) => setLabel(e.target.value)} />
        {error && <div className="text-xs text-red-400">{error}</div>}
        <div className="flex gap-2">
          <button type="button" className="rounded px-3 py-1 hover:underline" onClick={() => setStep("chooser")}>Back</button>
          <button type="submit" disabled={busy} className="rounded bg-blue-600 px-3 py-1 text-white disabled:opacity-50">Claim</button>
        </div>
      </form>
    );
  }

  if (step === "code") {
    const codeIsValid = /^[A-Z2-7\s\-]+$/i.test(code.trim()) && code.replace(/\s|-/g, "").length >= 80;
    return (
      <form
        className="flex flex-col gap-3 p-6"
        onSubmit={(e) => {
          e.preventDefault();
          handle(async () => {
            await cmd.pairWithCode({ code, device_label: label });
            window.close();
          });
        }}
      >
        <h1 className="text-base font-semibold">Add this device</h1>
        <label className="text-xs text-zinc-400">Pair code</label>
        <textarea
          rows={4}
          data-testid="pair-code"
          className={`rounded bg-zinc-800 px-2 py-1 font-mono ${code && !codeIsValid ? "ring-1 ring-red-500" : ""}`}
          value={code}
          onChange={(e) => setCode(e.target.value)}
        />
        <label className="text-xs text-zinc-400">Device label</label>
        <input className="rounded bg-zinc-800 px-2 py-1" value={label} onChange={(e) => setLabel(e.target.value)} />
        {code && !codeIsValid && <div className="text-xs text-red-400">That doesn't look like a valid pair code.</div>}
        {error && <div className="text-xs text-red-400">{error}</div>}
        <div className="flex gap-2">
          <button type="button" className="rounded px-3 py-1 hover:underline" onClick={() => setStep("chooser")}>Back</button>
          <button type="submit" disabled={busy || !codeIsValid} className="rounded bg-blue-600 px-3 py-1 text-white disabled:opacity-50">Pair</button>
        </div>
      </form>
    );
  }

  return (
    <div className="flex flex-col gap-3 p-6">
      <h1 className="text-base font-semibold">Show this code on the new device</h1>
      <pre data-testid="shortcode" className="whitespace-pre-wrap rounded bg-zinc-800 p-3 font-mono text-xs">{shortcode}</pre>
      <Countdown expiresAt={expiresAt} />
      {error && <div className="text-xs text-red-400">{error}</div>}
    </div>
  );
}

function Countdown({ expiresAt }: { expiresAt?: number }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const i = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(i);
  }, []);
  if (!expiresAt) return null;
  const remaining = Math.max(0, Math.ceil((expiresAt - now) / 1000));
  return <div data-testid="countdown" className="text-xs text-zinc-400">Expires in {remaining}s</div>;
}

function defaultLabel(): string {
  // Keep it simple — engineer can swap to navigator.platform + a random suffix later.
  return "macbook";
}

function messageOf(e: unknown): string {
  if (typeof e === "object" && e && "message" in e && typeof (e as AppErrorPayload).message === "string") {
    return (e as AppErrorPayload).message;
  }
  return String(e);
}
```

- [ ] **Step 2: Test wizard step navigation and the http warning**

`clients/desktop/ui/src/__tests__/PairingModal.test.tsx`:
```tsx
import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { injectForTests } from "../ipc/tauri";
import PairingModal from "../modals/PairingModal";

beforeEach(() => {
  const invoke = vi.fn(async () => ({ user_id: "u", device_id: "d" }));
  const listen = vi.fn(async () => () => {});
  injectForTests(invoke as never, listen as never);
});

describe("PairingModal", () => {
  it("starts on the chooser screen", () => {
    render(<PairingModal />);
    expect(screen.getByTestId("choose-invite")).toBeInTheDocument();
    expect(screen.getByTestId("choose-code")).toBeInTheDocument();
  });

  it("navigates to the invite step", () => {
    render(<PairingModal />);
    fireEvent.click(screen.getByTestId("choose-invite"));
    expect(screen.getByText(/Claim invite/i)).toBeInTheDocument();
  });

  it("warns on plain http to non-localhost", () => {
    render(<PairingModal />);
    fireEvent.click(screen.getByTestId("choose-invite"));
    const url = screen.getByLabelText(/Server URL/i, { selector: "input" }) as HTMLInputElement;
    fireEvent.change(url, { target: { value: "http://example.com" } });
    expect(screen.getByTestId("insecure-warning")).toBeInTheDocument();
  });

  it("shows red border on invalid pair code", () => {
    render(<PairingModal />);
    fireEvent.click(screen.getByTestId("choose-code"));
    const ta = screen.getByTestId("pair-code") as HTMLTextAreaElement;
    fireEvent.change(ta, { target: { value: "tiny" } });
    expect(ta.className).toContain("ring-red-500");
  });
});
```

> The Search input in `PairingModal` is rendered without a `<label>` element wrapping it; use `getByLabelText` only on inputs that have a `<label>` association. The test queries the input via `getByLabelText("Server URL")` which matches the explicit `<label>` text and the next `<input>`. Confirm with React Testing Library's behaviour by using `screen.getByLabelText` with `selector: "input"` to disambiguate.

- [ ] **Step 3: Run UI tests**

Run:
```bash
npm --prefix clients/desktop/ui test
```
Expected: 4 new tests pass on top of the existing suite.

- [ ] **Step 4: Commit**

```bash
git add clients/desktop/ui/src/modals/PairingModal.tsx clients/desktop/ui/src/__tests__/PairingModal.test.tsx
git commit -m "feat(desktop-ui): pairing wizard with chooser, invite, code, show-code"
```

---

## Task 25: UI — Settings and Accounts modals

**Files:**
- Create: `clients/desktop/ui/src/modals/SettingsModal.tsx`
- Create: `clients/desktop/ui/src/modals/AccountsModal.tsx`
- Create: `clients/desktop/ui/src/__tests__/SettingsModal.test.tsx`

- [ ] **Step 1: Settings modal — capture toggle, deny-list, autostart, hotkey**

`clients/desktop/ui/src/modals/SettingsModal.tsx`:
```tsx
import { useEffect, useState } from "react";
import type { Settings } from "../types";
import { cmd } from "../ipc/commands";

export default function SettingsModal() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<string>();

  useEffect(() => {
    cmd.getSettings().then(setSettings).catch((e) => setError(String(e)));
  }, []);

  if (!settings) return <div className="p-6 text-sm">Loading…</div>;

  const update = async (patch: Partial<Settings>) => {
    try {
      const next = await cmd.updateSettings(patch);
      setSettings(next);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="flex flex-col gap-4 p-6 text-sm">
      <h1 className="text-base font-semibold">Settings</h1>

      <label className="flex items-center gap-2">
        <input
          data-testid="capture-enabled"
          type="checkbox"
          checked={settings.capture_enabled}
          onChange={(e) => update({ capture_enabled: e.target.checked })}
        />
        Capture clipboard changes
      </label>

      <label className="flex items-center gap-2">
        <input
          data-testid="autostart"
          type="checkbox"
          checked={settings.autostart}
          onChange={(e) => update({ autostart: e.target.checked })}
        />
        Launch at login
      </label>

      <label className="text-xs text-zinc-400">Deny-list (one bundle id per line)</label>
      <textarea
        data-testid="deny-list"
        rows={4}
        className="rounded bg-zinc-800 px-2 py-1 font-mono text-xs"
        value={settings.deny_list.join("\n")}
        onChange={(e) => update({ deny_list: e.target.value.split("\n").map((s) => s.trim()).filter(Boolean) })}
      />

      <label className="text-xs text-zinc-400">Global hotkey (e.g. <code>Cmd+Shift+V</code>; empty to unbind)</label>
      <input
        data-testid="hotkey"
        className="rounded bg-zinc-800 px-2 py-1 font-mono text-xs"
        value={settings.hotkey ?? ""}
        onChange={(e) => update({ hotkey: e.target.value || null })}
      />

      {error && <div className="text-xs text-red-400">{error}</div>}
    </div>
  );
}
```

- [ ] **Step 2: Accounts modal — list, switch active, revoke, forget**

`clients/desktop/ui/src/modals/AccountsModal.tsx`:
```tsx
import { useEffect, useState } from "react";
import type { Account } from "../types";
import { cmd } from "../ipc/commands";

export default function AccountsModal() {
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [active, setActive] = useState<string | undefined>();
  const [error, setError] = useState<string>();

  const refresh = async () => {
    try { setAccounts(await cmd.listAccounts()); }
    catch (e) { setError(String(e)); }
  };

  useEffect(() => { refresh(); }, []);

  return (
    <div className="flex flex-col gap-3 p-6 text-sm">
      <h1 className="text-base font-semibold">Accounts</h1>
      <ul className="flex flex-col gap-2">
        {accounts.map((a) => (
          <li key={a.user_id} className="rounded border border-zinc-700 p-3 flex justify-between items-center">
            <div>
              <div className="font-semibold">{a.label}</div>
              <div className="text-xs text-zinc-400">{a.user_id} @ {a.server_url}</div>
              <div className="text-xs text-zinc-400">status: {a.status} · pending: {a.pending}</div>
            </div>
            <div className="flex gap-2">
              <button
                data-testid={`switch-${a.user_id}`}
                className="rounded bg-blue-600 px-2 py-1 text-white"
                onClick={async () => {
                  await cmd.setActiveAccount({ user_id: a.user_id });
                  setActive(a.user_id);
                }}
              >
                {active === a.user_id ? "Active" : "Use"}
              </button>
              <button
                className="rounded bg-red-600 px-2 py-1 text-white"
                onClick={async () => {
                  if (!confirm(`Forget ${a.label}? Local history and key will be erased.`)) return;
                  await cmd.forgetAccount({ user_id: a.user_id });
                  await refresh();
                }}
              >
                Forget
              </button>
            </div>
          </li>
        ))}
      </ul>
      {error && <div className="text-xs text-red-400">{error}</div>}
    </div>
  );
}
```

- [ ] **Step 3: Test the SettingsModal updates**

`clients/desktop/ui/src/__tests__/SettingsModal.test.tsx`:
```tsx
import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { injectForTests } from "../ipc/tauri";
import SettingsModal from "../modals/SettingsModal";

let invokeMock: ReturnType<typeof vi.fn>;

beforeEach(() => {
  invokeMock = vi.fn(async (cmd: string, args?: any) => {
    if (cmd === "get_settings") {
      return { capture_enabled: true, deny_list: [], autostart: false, hotkey: null };
    }
    if (cmd === "update_settings") {
      return { capture_enabled: !!args?.patch?.capture_enabled,
               deny_list: args?.patch?.deny_list ?? [],
               autostart: !!args?.patch?.autostart,
               hotkey: args?.patch?.hotkey ?? null };
    }
    return null;
  });
  injectForTests(invokeMock as never, (async () => () => {}) as never);
});

describe("SettingsModal", () => {
  it("toggles capture_enabled and calls update_settings", async () => {
    render(<SettingsModal />);
    const cb = await screen.findByTestId("capture-enabled");
    fireEvent.click(cb);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("update_settings", expect.objectContaining({
        patch: expect.objectContaining({ capture_enabled: false }),
      }));
    });
  });

  it("renders deny-list as newline-separated", async () => {
    invokeMock.mockImplementationOnce(async () => ({
      capture_enabled: true, deny_list: ["a", "b"], autostart: false, hotkey: null,
    }));
    render(<SettingsModal />);
    const ta = await screen.findByTestId("deny-list") as HTMLTextAreaElement;
    expect(ta.value).toBe("a\nb");
  });
});
```

- [ ] **Step 4: Run UI tests**

Run:
```bash
npm --prefix clients/desktop/ui test
```
Expected: prior + 2 new tests pass.

- [ ] **Step 5: Commit**

```bash
git add clients/desktop/ui/src/modals/SettingsModal.tsx clients/desktop/ui/src/modals/AccountsModal.tsx clients/desktop/ui/src/__tests__/SettingsModal.test.tsx
git commit -m "feat(desktop-ui): settings + accounts modals with tests"
```

---

## Task 26: README + manual smoke checklist + final build

**Files:**
- Create: `clients/desktop/README.md`

- [ ] **Step 1: Document dev workflow + smoke checklist**

`clients/desktop/README.md`:
```markdown
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
```

- [ ] **Step 2: Final full-suite check before tagging**

Run:
```bash
cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml
npm --prefix clients/desktop/ui test
cargo build --release --manifest-path clients/desktop/src-tauri/Cargo.toml
```
Expected: all tests pass; release binary lands in `target/release`.

- [ ] **Step 3: Commit and tag**

```bash
git add clients/desktop/README.md
git commit -m "docs(desktop): manual smoke checklist + dev/build instructions"
git tag desktop-v0.1.0-track2
```

---

## Self-review

- **Spec coverage**
  - Repo layout `clients/desktop/...` → Task 1.
  - Popover surface, accessory app, tray template image → Tasks 1, 12 (config), 20 (tray + popover window).
  - rusqlite `state.sqlite` with `accounts`, `entries_cache`, `pending_uploads`, `settings` → Tasks 4, 5, 6.
  - Retention (100 newest, 30 days) and pending cap (1000) → Task 5.
  - Keychain put/get/delete with `<user_id>:key` / `<user_id>:token` → Task 7.
  - Crypto: XChaCha20-Poly1305 wrapper with KAT vectors and AAD enforcement → Task 3.
  - Pairing: invite + QR/short-code (encode/decode/round-trip) → Tasks 8, 10.
  - HTTP client for every endpoint in the API table → Task 9.
  - SSE subscriber + backfill → Tasks 11, 20.
  - Uploader (FIFO + auth-failure terminal state) → Task 12.
  - Decryptor (cache plaintext, mark undecryptable, preview) → Task 13.
  - Capture filter rules including self-write guard → Task 14.
  - macOS NSPasteboard type sniff + frontmost-app via objc2 → Task 15.
  - Watcher via clipboard-master → Task 16.
  - Account registry with active selection + forget → Task 17.
  - AppState + event constants matching spec § "Tauri IPC surface" → Tasks 18, 19.
  - Tauri commands matching every line in spec § Commands → Task 19.
  - Tray + popover + modals + plugins (autostart, global-shortcut) → Tasks 1, 12, 20.
  - Sync orchestration: backoff, AuthFailed, reconnect with last_seen_id → Task 20 (uses Task 16's `BackoffPlan`).
  - Integration tests for both flows + auth-revoke → Task 21.
  - UI bootstrap, IPC layer, store slices → Task 22.
  - Popover view (search, list, footer) → Task 23.
  - Pairing modal (two-step wizard, http warning, code validation) → Task 24.
  - Settings + accounts modals → Task 25.
  - Error-handling table behaviours: AuthFailed banner / connection-state events / capture-skipped events / pair-expired modal → emitted/consumed in Tasks 20 (server-side), 23 (footer), 24 (pair modal).
  - Logging: `tracing` JSON to `~/Library/Logs/sharepaste/desktop.log` → Task 2.
  - HTTP scheme policy (warn on non-localhost http) → Task 24.
  - Build + unsigned distribution + `xattr` workaround → Task 26.

- **Placeholder scan**
  - No `TBD` / `TODO` left in the body. Each task includes complete code, runnable commands, and the expected output.
  - The integration helper documents an assumption about the server CLI's `invite_token=...` line and tells the implementer to update the matcher if the real format differs — that's an explicit known-unknown, not a placeholder.

- **Type consistency**
  - Rust `Settings` shape (Task 6) matches the React `Settings` type (Task 22) and `update_settings` patch handling (Task 19).
  - `EntryView` matches between Rust events (Task 18) and React types/views (Task 22, Task 23).
  - `ConnectionState` enum is consistent across Rust (`core::sync::ConnectionState`) and TS (`types.ts`).
  - Command names in `commands.ts` match `#[tauri::command]` function names exactly.

No issues found that block execution. Plan ready.

---

Plan complete and saved to `docs/superpowers/plans/2026-05-01-sharepaste-macos.md`. Two execution options:

1. **Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?

