# Sharepaste macOS Client — Design

**Date:** 2026-05-01
**Status:** Draft, pending user review
**Track:** 2 of 5 (per `2026-05-01-sharepaste-design.md`)
**Depends on:** Track 1 (server + CLI) — frozen wire protocol.

## Summary

A macOS Tauri 2 desktop client that watches the system clipboard, encrypts entries with the per-user key, uploads to the self-hosted sharepaste server, and renders a menu-bar popover with searchable history. Pure-Rust crypto and HTTP layers, React + Tailwind UI. Designed so the Windows client (track 3) can reuse the same Rust core with thin platform-specific shims.

This design only covers macOS. Windows-specific concerns (clipboard chain listener, transient-flag detection on Win32) are out of scope but the module layout reserves seams.

## Goals

- Capture text clipboard changes, encrypt locally, upload to server, never leak plaintext to the wire.
- Render last 100 entries in a tray popover with sub-100ms open time.
- Survive offline gracefully: pending queue, automatic backfill on reconnect, no silent drops.
- Support multiple user accounts on a single device, with an account switcher.
- Self-host friendly: any user-controlled HTTPS server URL, no hard-coded endpoints.

## Non-goals

- Image, file, or rich clipboard.
- Background sync when app is quit (capture stops when app exits — by design).
- Code signing / notarization (deferred until distribution scale demands it).
- Auto-update (manual DMG download in MVP).
- Windows or Linux builds (separate tracks).
- CI configuration (left to operator preference; tests are runnable manually).
- Conflict resolution beyond server-assigned monotonic ids.

## Top-level decisions

| Area | Choice |
|---|---|
| Repo layout | Monorepo subdir at `clients/desktop/` |
| Tray surface | Menu-bar popover (accessory app, no Dock icon) |
| Local storage | rusqlite single file in `~/Library/Application Support/sharepaste/state.sqlite` |
| Crypto | RustCrypto `chacha20poly1305::XChaCha20Poly1305` (pure Rust) |
| Clipboard | `clipboard-master` (events) + `arboard` (read text) + `objc2` (NSPasteboard type sniff on macOS) |
| State ownership | Rust owns durable + secret state; React owns view state via Zustand |
| Pairing UX | Two-step wizard: chooser → flow form (invite token / pair code) |
| Distribution | Unsigned `.app` + `.dmg` via manual GitHub release |
| Login item / hotkey | Opt-in, default off |
| Auto-update | Disabled in MVP |
| macOS minimum | 12.0 (Monterey) |
| Bundle id | `com.sharepaste.desktop` |

## Architecture

```
┌──────────────────────── sharepaste.app (Tauri 2) ─────────────────────────┐
│                                                                            │
│   React + Tailwind (UI)              Rust core (no Tauri deps)            │
│   ├─ Popover                         ├─ crypto (chacha20poly1305)         │
│   ├─ Pair wizard                     ├─ storage (rusqlite)                 │
│   ├─ Accounts modal                  ├─ keychain (keyring crate)           │
│   ├─ Settings modal                  ├─ sync (state machine + reqwest)    │
│   └─ Zustand store                   ├─ capture (clipboard-master + objc2)│
│                                      ├─ pairing (invite + qr/shortcode)    │
│            ▲                         └─ account (membership registry)      │
│            │ Tauri commands + events    ▲                                  │
│            └────────────┬────────────────┘                                 │
│                         │                                                  │
│                  src-tauri/main.rs                                         │
│                  (commands.rs bridges core ↔ webview)                       │
└────────────┬─────────────────────────────────────────────────────────────────┘
             │ HTTPS (REST + SSE), Bearer <device_token>, ciphertext only
             ▼
        sharepaste server
```

The Rust core is independently testable without a Tauri runtime. `commands.rs` is the only file that imports both `tauri::*` and `core::*`.

## Repo layout

```
sharepaste/
  src/                              (track 1 — unchanged)
  clients/
    desktop/
      package.json                  (root scripts: tauri dev/build)
      rust-toolchain.toml
      src-tauri/
        Cargo.toml
        tauri.conf.json
        build.rs
        src/
          main.rs                   Tauri entry, tray, windows, command registry
          state.rs                  AppState (handles, channels, AppHandle)
          config.rs                 paths, env (DB_PATH override for tests)
          commands.rs               #[tauri::command] surface
          events.rs                 event names + payload types
          core/
            mod.rs
            crypto.rs               XChaCha20-Poly1305-IETF wrappers
            storage.rs              rusqlite migrations + repository
            keychain.rs             keyring crate (user_key, device_token per user)
            sync/
              mod.rs                state machine
              client.rs             reqwest HTTP client
              sse.rs                reqwest-eventsource subscription + backfill
              uploader.rs           pending-queue flusher
              decryptor.rs          decrypt + cache plaintext
            capture/
              mod.rs
              watcher.rs            clipboard-master event source
              filter.rs             size, transient, deny-list, dedup
              macos.rs              objc2 NSPasteboard type sniff (cfg macos)
            pairing/
              mod.rs
              invite.rs             POST /claim-invite
              qr.rs                 POST /pair/start, payload, devices
              shortcode.rs          base32 codec
            account/
              mod.rs                membership registry, active selection
        tests/                      integration tests (real server, real sqlite)
      ui/
        package.json
        vite.config.ts
        tailwind.config.ts
        index.html
        src/
          main.tsx
          App.tsx
          store/                    zustand slices: ui, history, accounts, status
          views/
            Popover.tsx
            Search.tsx
            HistoryList.tsx
            EntryRow.tsx
            Footer.tsx
          modals/
            Pairing.tsx             two-step wizard
            Settings.tsx
            Accounts.tsx
          ipc/
            commands.ts             typed wrappers around invoke()
            events.ts               listen() subscriptions → store dispatch
          types/                    DTOs mirroring Rust
```

## Local schema

```sql
CREATE TABLE accounts (
  user_id        TEXT PRIMARY KEY,
  device_id      TEXT NOT NULL,
  device_label   TEXT NOT NULL,
  server_url     TEXT NOT NULL,
  last_seen_id   INTEGER NOT NULL DEFAULT 0,
  created_at     INTEGER NOT NULL
);

CREATE TABLE entries_cache (
  user_id       TEXT NOT NULL,
  id            INTEGER NOT NULL,                -- server-assigned
  ciphertext    BLOB NOT NULL,
  plaintext     TEXT,                             -- decrypted; NULL until decryptor runs
  created_at    INTEGER NOT NULL,
  device_id     TEXT NOT NULL,
  PRIMARY KEY (user_id, id)
);
CREATE INDEX entries_cache_user_id_id ON entries_cache (user_id, id DESC);

CREATE TABLE pending_uploads (
  rowid         INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id       TEXT NOT NULL,
  ciphertext    BLOB NOT NULL,
  captured_at   INTEGER NOT NULL,
  attempts      INTEGER NOT NULL DEFAULT 0,
  last_error    TEXT
);
CREATE INDEX pending_uploads_user_id_rowid ON pending_uploads (user_id, rowid);

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
```

Secrets (`user_key`, `device_token`) live in the macOS Keychain — **not** in this database.

### Retention

`entries_cache` mirrors server retention: on every insert, drop rows beyond the 100 newest per user OR older than 30 days, in a single transaction. Same SQL pattern as the server's `insertAndPrune`.

`pending_uploads` capped at 1000 rows per user; oldest dropped with a `capture-skipped { reason: "queue_overflow" }` event surfaced as a once-per-session toast.

## Sync state machine

One Tokio task per active membership.

```
Disconnected ──── start ────▶ Connecting ──── ok ────▶ Online
     ▲                            │                       │
     │                            │ 401                   │ 401
     │                            ▼                       ▼
     │                       AuthFailed              AuthFailed
     │                                                    │
     │                            net error / SSE drop    │
     └────────────────────────────┴───────────────────────┘
                          backoff: 1, 2, 4, 8, 30s cap, ±20% jitter
```

- **Disconnected**: no token loaded, or membership inactive.
- **Connecting**: HTTP probe + SSE open; on success runs `GET /entries?since=last_seen_id&limit=500` paged backfill; updates `last_seen_id`.
- **Online**: SSE feeding events. Uploader flushes `pending_uploads` head FIFO. Heartbeat watchdog 30s (no SSE traffic for 30s → reconnect).
- **AuthFailed**: terminal until user re-pairs. Tray badge red. Banner offers "Re-pair this device".

Switching accounts cancels the running task via `CancellationToken` and spawns a new one for the new active membership.

## Capture pipeline

```
clipboard-master (changeCount poll) ──▶ filter ──▶ encrypt ──▶ pending_uploads ──▶ uploader ──▶ POST /entries
```

### Filter rules (in order, short-circuit)

1. `settings.capture_enabled = false` → drop.
2. macOS: read `NSPasteboard.types` via `objc2`. Drop if any of `org.nspasteboard.ConcealedType`, `org.nspasteboard.TransientType`, `Concealed`, `transient`. Log with reason.
3. `arboard::get_text()`. Non-text → drop silently.
4. UTF-8 byte length > 64 KB → drop, emit `capture-skipped { reason: "too_large" }`.
5. Frontmost-app bundle id (via `objc2 NSWorkspace.frontmostApplication` — no special permission required) matches `settings.deny_list` → drop.
6. **Self-write guard**: drop if plaintext + timestamp matches `last_self_write` within 1s — this is our own paste-back from `copy_to_clipboard` command.

### Encryption

XChaCha20-Poly1305-IETF, 24-byte fresh random nonce per entry, AAD = `user_id` UTF-8 bytes. Wire ciphertext = `nonce || aead_output`, base64 at HTTP layer. `user_key` loaded from Keychain at sync-task start, held in `Zeroizing<[u8; 32]>` for task lifetime.

### Concurrency

Watcher → bounded `mpsc<ClipboardEvent>` (cap 32) → filter+encrypt task → single sqlite write txn into `pending_uploads` → `Notify` to uploader. Backpressure: drop oldest if channel full and log.

## Pairing on desktop

Modal layout: two-step wizard. Step 1 = chooser (invite token / pair code). Step 2 = form for the chosen flow.

### Flow 1 — claim invite

```
React (Step 2)              Rust commands.rs              Server
[token, label] ──invoke("pair_with_invite")──▶
                              gen user_key
                              POST /claim-invite ─────────▶
                                              ◀── { device_token, user_id, device_id }
                              Keychain put (user_id+":key", user_id+":token")
                              INSERT accounts
                              spawn sync task
                              emit "account-added"
                       ◀── Ok
```

### Flow 2 — add this device using a pair code from another device

```
Inviter (existing device)                          Claimer (this device)
[Add device] ──pair_start──▶                       [paste code] ──pair_with_code──▶
  gen pairing_secret                                 decode shortcode
  POST /pair/start { hash } ─▶ { pair_id }           POST /pair/claim { pair_id, secret_proof } ─▶ 200
  encode shortcode                                   GET /pair/payload?id&proof
  poll GET /pair/poll                                  ◀── { encrypted_payload }
                                                     decrypt → { user_id, user_key, server_url }
                                                     POST /devices { pair_id, secret_proof, label } ─▶ { device_token, device_id, user_id }
                                                     Keychain put, INSERT accounts, spawn sync task
                                                     emit "account-added"
  poll → { status: "claimed" }
  encrypt {user_id, user_key, server_url} with pairing_secret
  POST /pair/payload                       ─▶ 200
  emit "pair-claimed"
```

Inviter's `pair-watch` task polls every 2s, exits on `consumed` / `expired` / on its own payload upload completion. Claimer never long-polls.

### Shortcode encoding

Base32 (RFC 4648, no padding) of `version_byte || server_url_len:u16be || server_url_utf8 || pair_id_uuid_bytes || pairing_secret_32`. Decoder strips whitespace and case-folds to upper. Sample length ~120 chars; rendered in groups of 5 in monospace for transcription.

## Tauri IPC surface

### Commands (React → Rust)

```
pair_with_invite({ server_url, token, device_label }) -> { user_id, device_id }
pair_start({ user_id }) -> { code, expires_at }
pair_with_code({ code, device_label }) -> { user_id, device_id }
pair_cancel({ user_id })

list_accounts() -> Account[]
set_active_account({ user_id })
forget_account({ user_id })
revoke_device({ user_id, device_id })

list_history({ user_id, before_id?, limit }) -> EntryView[]
search_history({ user_id, query, limit }) -> EntryView[]
get_entry_full({ user_id, entry_id }) -> string
copy_to_clipboard({ user_id, entry_id })
delete_entry({ user_id, entry_id })
clear_history({ user_id })

get_settings() -> Settings
update_settings(Partial<Settings>) -> Settings
get_status({ user_id }) -> { state, pending_count, last_error? }
```

All commands return `Result<T, AppError>` where `AppError = { kind, message }` with `kind ∈ { Network | Auth | NotFound | BadInput | Storage | Crypto | PairExpired | Keychain }`.

### Events (Rust → React)

```
"account-added"      { user_id, device_id, label }
"account-removed"    { user_id }
"active-changed"     { user_id }
"connection-state"   { user_id, state, last_error? }
"entry-added"        { user_id, entry: EntryView }
"entry-deleted"      { user_id, entry_id }
"history-changed"    { user_id }
"pending-count"      { user_id, count }
"capture-skipped"    { reason, source_app? }
"decryption-error"   { user_id, entry_id }
"pair-shortcode"     { code, expires_at }
"pair-claimed"       { user_id }
"pair-expired"       {}
```

### DTOs

```ts
type EntryView = {
  id: number; user_id: string;
  preview: string;            // first 80 chars, single-line, control-stripped
  created_at: number; device_id: string; device_label?: string;
};
type Account = {
  user_id: string; device_id: string; label: string;
  server_url: string; status: ConnectionState; pending: number;
};
type Settings = {
  capture_enabled: boolean; deny_list: string[];
  autostart: boolean; hotkey?: string;
};
type ConnectionState = "Disconnected" | "Connecting" | "Online" | "AuthFailed";
```

`list_history` returns previews only — popover-fast even for the full 100-entry window. `copy_to_clipboard` reads plaintext directly from `entries_cache` in Rust and writes to the system clipboard without ever crossing the IPC boundary. `get_entry_full` exists for the rare UI need (e.g. a future preview pane) and returns `Err(NotFound)` if `entries_cache.plaintext IS NULL` (i.e. that entry failed to decrypt earlier).

## Error handling

| Failure | Behavior |
|---|---|
| Server unreachable / 5xx | Sync → `Connecting` with backoff. Pending accumulates. Tray amber. Footer "offline · N pending". |
| TLS cert error | Retry continues. `connection-state.last_error = "tls"`. Footer hint "Check server certificate". No trust override. |
| `POST /entries` 401 | Sync → `AuthFailed`. Uploader stops. Tray red. Banner with `Re-pair` button. Local cache + key retained until user picks `Forget account`. |
| `POST /entries` 413 | Drop from queue, emit `capture-skipped { reason: "too_large" }`. Defense-in-depth — should already be filtered. |
| Other 4xx | Drop from queue, log, debounced toast (30s) "Server rejected an entry — see logs". |
| SSE drop | Reconnect 1→30s backoff. On reopen, `GET /entries?since=last_seen_id&limit=500` paged backfill before resuming live stream. |
| Decryption failure on fetched entry | Skip, mark `entries_cache.plaintext = NULL`, increment session counter, emit `decryption-error`. Footer once-per-session "1 undecryptable entry". No auto-retry. |
| Pair `pair_id` 404/410 | Modal: "Pair code expired or already used. Generate a new one." |
| Pair claim 403 | Modal: "Wrong code. {2|1} attempts left." After 3rd: "Code burned. Restart pairing on the other device." |
| Keychain unavailable | Bubble `KeychainError`. Modal: "Cannot access Keychain. Unlock and retry." Sync task does not start. |
| SQLite locked | Retry once after 100ms, else bubble `Storage` error. |
| Clipboard write failure | Toast "Couldn't put on clipboard". No state change. |
| Decode fail on paste code | Modal validation: red border + "That doesn't look like a valid pair code." |
| Crash during pending flush | At-least-once: pending row remains, retried on next launch. Server may receive duplicates → distinct ids per spec, history shows two entries. Acceptable. |
| User force-quits during pairing | Pair slot expires server-side at TTL. Inviter's pair-watch task drops on app exit. |

### Logging

`tracing` crate, JSON layer, file `~/Library/Logs/sharepaste/desktop-YYYY-MM-DD.log`, 7-day retention. **Never log clipboard plaintext, `user_key`, `device_token`, or `pairing_secret`.** Capture-skipped logs include reason and source-app id but no bytes.

## Build, distribution, OS integration

### Build

- Tauri 2 (`tauri@^2`, `@tauri-apps/cli@^2`).
- `tauri.conf.json`: `productName: sharepaste`, `identifier: com.sharepaste.desktop`, `bundle.macOS.minimumSystemVersion: 12.0`, `bundle.macOS.LSUIElement: true`.
- Rust toolchain pinned via `rust-toolchain.toml` (stable).
- Frontend bundler: Vite. Tailwind via PostCSS.
- Dev: `cargo tauri dev` boots Vite at `1420`, server runs separately at user-supplied URL.
- HTTP scheme policy: `http://localhost*` accepted without warning; any other `http://` URL triggers an in-modal warning ribbon ("Unencrypted — only use on trusted networks").
- Release: `cargo tauri build` → `.app` + `.dmg` in `src-tauri/target/release/bundle/`.

### Signing & distribution (MVP)

- **Unsigned `.app`**, distributed as `.dmg` via manual GitHub release upload.
- README documents Gatekeeper workaround: `xattr -d com.apple.quarantine /Applications/sharepaste.app`.
- Auto-update disabled. Apple Developer ID + notarization deferred.

### OS integration

- **Accessory app** (`LSUIElement = true`). No Dock icon, no app menu bar. Status item only.
- **Tray**: `tauri::tray::TrayIconBuilder`. 16pt template image (auto-light/dark). Click toggles popover. Badge tint = aggregated connection state across active accounts (green/amber/red).
- **Popover**: undecorated, fixed 360×480, anchored to tray geometry on open. `decorations: false`, `always_on_top: true`, `focus_on_show: true`. Hides on blur unless a child modal is open.
- **Modals** (Settings, Pair, Accounts): regular decorated windows, `resizable: false`, opened on demand, killed on dismiss.
- **Login item**: opt-in via `tauri-plugin-autostart`. Default off.
- **Global hotkey**: opt-in via `tauri-plugin-global-shortcut`. Default unbound. User chooses (e.g. `Cmd+Shift+V`) → invokes `show_popover`.
- **Permissions**: clipboard read/write needs no prompt on macOS. Accessibility prompt is requested only when the user enables a global hotkey (Apple requires it for system-wide key capture); the rest of the app works without it. First launch shows a friendly explanation modal covering the data flow.

### Local data layout

```
~/Library/Application Support/sharepaste/
  state.sqlite, state.sqlite-wal, state.sqlite-shm
~/Library/Logs/sharepaste/
  desktop-YYYY-MM-DD.log
~/Library/Caches/sharepaste/
  qr/                        (transient QR PNGs, cleared on dismiss)
Keychain (login, service="sharepaste"):
  account="<user_id>:key"    user_key (32 bytes hex)
  account="<user_id>:token"  device_token (raw)
```

## Testing

### Rust core (unit, no Tauri runtime)

- **`crypto`**: known-answer vectors generated from libsodium for cross-implementation compatibility. Round-trip with random nonces. AAD mismatch fails. Tampered ciphertext fails.
- **`storage`**: temp sqlite, migrations, retention prune (insert 105 → 5 evicted; old `created_at` → time-evicted), pending FIFO + retry counter.
- **`capture::filter`**: table-driven cases for size, deny-list, transient detection (mock NSPasteboard via trait, suite runs anywhere), self-write dedup window.
- **`pairing::shortcode`**: base32 round-trip; rejects malformed; tolerates whitespace + case.
- **`sync` state machine**: synthetic transitions over a mocked transport trait. Asserts backoff schedule, AuthFailed terminal-ness.
- **`uploader`**: mocked transport, FIFO + at-least-once retry semantics, attempt counter increments.

### Rust integration (against real server)

Tests live under `clients/desktop/src-tauri/tests/`. Each spins a `sharepaste serve` process on a tempfile DB on a free port, hits the full flow end to end:

- Flow 1: claim invite → upload → fetch → SSE live event.
- Flow 2: pair_start on instance A, pair_with_code on instance B, both upload, both see each other's entries.
- Auth revocation: `DELETE /devices/:id` → instance B reaches `AuthFailed` within reconnect window.
- Multi-account on one app instance: switch active, history hydrates from cache + backfill, capture routes to correct account.

No mocks. Fresh tempfile DB and process per test.

### React / UI

- **Component tests** (Vitest + Testing Library): popover render given fake store state, search filters list, footer renders states, pair wizard tab switching, keyboard shortcuts (↑/↓/Enter to copy).
- **Store unit tests**: zustand reducers for `history` and `accounts` slices.
- `commands.ts` and `events.ts` are wrapped behind a tiny interface so a test mock replaces the Tauri runtime.

### Manual smoke checklist (in `clients/desktop/README.md`)

1. `cargo tauri dev`, ensure tray icon appears, accessory mode (no Dock).
2. Claim invite against local server, copy text in any app, see entry appear in popover.
3. Pair second instance via short code (run with override data dir), entries cross-flow.
4. Revoke device from server CLI, observe red banner + `AuthFailed` state.
5. Toggle capture-enabled; copy concealed item via 1Password test; verify skipped.
6. Force-quit during upload; restart; verify pending flushes.

## Threat model deltas vs. server spec

The server spec already covers most of the system threat model. Client-side additions:

| Adversary capability | Outcome |
|---|---|
| Reads `~/Library/Application Support/sharepaste/state.sqlite` on disabled-FileVault Mac | Sees ciphertext + cached plaintext (cache is for fast popover). Mitigation: rely on FileVault, document in README. Optional v2: encrypt cached plaintext with `user_key`. |
| Reads Keychain on unlocked Mac | Sees `user_key` + `device_token`. Mitigation: macOS user session + login keychain protection. Same risk as any password-managing app. |
| Process injection / arbitrary code in the app | Full compromise. Mitigation: standard macOS hardening + (future) signed + hardened runtime. |
| Malicious clipboard contents | Encrypted, stored, displayed as plaintext in popover. Risk: rendering control characters or super-long payloads. Mitigation: 64 KB cap, control-char strip in `preview`. |

## Open questions for v2

- Encrypt cached plaintext in `entries_cache` with `user_key` (currently relies on FileVault).
- Apple Developer ID signing + notarization + hardened runtime + Tauri updater.
- Auto-paste + global hotkey overlay (Raycast-style).
- Per-entry "do not sync this one" toggle on the source device before upload.
- Snippet pinning / favorites at the cache level (not synced).
