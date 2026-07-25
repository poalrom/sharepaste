# Desktop Client Simplification & Test Consolidation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

> **STATUS: EXECUTED 2026-07-25** on branch `desktop-simplification`, six commits. All 40 tasks across Phases 1-6 and T1-T4 are complete and verified. Two defects that only became visible *after* Task 4.1 re-enabled the `dead_code` lint were fixed on top of the plan and are recorded in the Execution Record at the end of this document.

**Goal:** Fix the ACL regression that leaves the main window unable to receive events, close three gaps against the intended clipboard→popover→main-window flow, delete the dead command/event surface those bugs hid, get the desktop suite running in CI, and cut 133 defined tests to ~108 without losing a covered contract.

**Scope:** `clients/desktop/**`, `.github/workflows/desktop-build.yml`, root `Makefile`. No server changes — no wire contract changes are proposed.

**Baseline measured 2026-07-25:**

| | Value |
|---|---|
| `src-tauri/src` | 5,111 lines / 35 files (incl. inline `#[cfg(test)]` modules) |
| `ui/src` (excl. tests) | 1,078 lines / 22 files |
| `ui/src/__tests__` | 630 lines / 8 files / 41 tests |
| `cargo check --all-targets` | clean, **0 warnings** |
| `cargo test --lib` | **87 passed, 0 failed, 2 ignored** |
| `cargo test` (all targets) | **FAILS** — see S2 |
| `npx vitest run` | **41 passed / 8 files** |
| Desktop tests in CI | **none** |

---

## Findings

Severity-ordered. Every line reference verified against the tree as of this date.

### S1 — The main window can invoke commands but cannot receive any event: no capability covers the label `main`

> **Corrected 2026-07-25** after a screenshot showed the Accounts tab rendering live account data. An earlier draft of this finding claimed the whole window was dead. That was wrong — the mechanism below is verified against the *pinned* Tauri 2.11.0 source in the local registry, not the dev branch.

`capabilities/default.json:5` grants its permissions to `"windows": ["popover", "modal-*"]`.

The app creates exactly two windows:

| Label | Created at | Covered by a capability? |
|---|---|---|
| `popover` | `lib.rs:154` | yes |
| `main` | `lib.rs:426` | **no** |

No window whose label begins with `modal-` is created anywhere — `modal-*` matches nothing.

**This is a regression from the window-streamlining work.** `docs/superpowers/specs/2026-05-05-ui-window-streamlining-design.md:20` describes the previous topology: "`open_modal` Tauri cmd to spawn one window per `kind`. Each window gets a unique label `modal-<kind>`." Line 84 replaces them with "Single label `\"main\"`". Line 88 instructs that "the `modal-*` window labels are removed once nothing references them." The Rust and TS were migrated; `capabilities/default.json` never was. It still names the windows that no longer exist and does not name the one that does.

**Exact runtime consequence, verified against `tauri-2.11.0` as pinned in `Cargo.lock`:**

The ACL guard is `webview/mod.rs:1820`:

```rust
// we only check ACL on plugin commands or if the app defined its ACL manifest
if (plugin_command.is_some() || has_app_acl_manifest)
```

`has_app_acl_manifest` is **not** "capabilities exist". It is `acl.contains_key(APP_ACL_KEY)` where `APP_ACL_KEY = "__app-acl__"` (`tauri-utils-2.9.0/src/acl/mod.rs:50,348-350`) — true only when the app ships its own `permissions/` directory. This app has none, and `gen/schemas/acl-manifests.json` contains only plugin keys (`autostart`, `core`, `core:*`, `global-shortcut`). So `has_app_acl_manifest == false` and the guard reduces to `plugin_command.is_some()`:

| Call from the `main` window | ACL-checked? | Result |
|---|---|---|
| `invoke("list_accounts")` and every other `#[tauri::command]` | no — not `plugin:`-prefixed | **works** |
| `listen(...)` → `plugin:event|listen` (`tauri-2.11.0/src/event/plugin.rs`) | yes | **rejected:** `Command plugin:event\|listen not allowed by ACL` (`webview/mod.rs:1847`) |

A window matching no capability resolves to no permissions at all (`ipc/authority.rs:439-465`), so every plugin command from `main` is denied. `app.emit_to("main", …)` (`lib.rs:422`) still succeeds on the Rust side — `emit_filter` performs no ACL check (`event/listener.rs`) — it just reaches no listener.

**So the window renders and its commands work; it is deaf.** Concretely broken, all user-reachable:

| Symptom | Listener that never registers |
|---|---|
| Pairing a second device never advances past the code screen — no "paired" confirmation, no expiry notice | `onPairClaimed`, `onPairExpired` (`PairingSection.tsx:35,40`) |
| Tray → **Settings…** while the window is already open does not switch tabs (`lib.rs:420-423` focuses and emits) | `onMainNavigate` (`Main.tsx:30`) |
| Account list, `ACTIVE` badge and `status:`/`pending:` line are correct at load but never update live | `onAccountAdded`, `onAccountRemoved`, `onActiveChanged`, `onConnectionState` (`AccountsSection.tsx:32-48`) |

The failure is invisible because both subscription blocks are bare `(async () => { … })()` IIFEs with no `catch` (`AccountsSection.tsx:29-49`, `PairingSection.tsx:28-41`). The first rejected `await` aborts the IIFE, so the remaining listeners are never even attempted and nothing surfaces to the user.

The popover is unaffected — it is covered by the capability, which is why clipboard history works and this went unnoticed.

**Why no test catches this:** all six component test files call `injectForTests` (`ui/src/ipc/tauri.ts:17`), which replaces `_invoke`/`_listen` with `vi.fn()` mocks — `AccountsSection.test.tsx:38`, `Main.test.tsx:16`, `PairingSection.test.tsx:21`, `Popover.test.tsx:38,67,86`, `SettingsSection.test.tsx:21`, `openSection.test.ts:13`. Every test's `listen` resolves happily. The suite substitutes the exact layer that is broken. That is the structural lesson here, and it survives the correction above.

### S2 — No desktop test runs in CI, and `cargo test` is red on a clean machine

`.github/workflows/desktop-build.yml` builds and uploads artifacts; it never runs `cargo test` or `npm test`. `server-ci.yml:48-50` runs the server suite, so the gap is desktop-specific. The root `Makefile` has no test target.

The three integration tests are not runnable as written. `tests/common/mod.rs` hard-codes a server URL (`:5`), a Docker container name (`:11`), and a container-internal DB path (`:10`), then **panics** rather than skips when the server is absent (`:40`), and shells out to `docker exec` (`:57-59`). Verified:

```
$ cargo test --test flow1_invite
thread 'invite_then_post_and_list' panicked at tests\common\mod.rs:40:10:
server not reachable at 127.0.0.1:8443; start sharepaste serve first
test result: FAILED. 0 passed; 1 failed
```

`cargo test --lib` is green (87/0/2). It is the default `cargo test` that fails, which is what any contributor or CI job will run.

### S3 — A third of the Tauri command surface is unreachable from the UI

18 commands are registered (`lib.rs:52-71`). Six have no caller in `ui/src`:

| Rust command | TS wrapper | Status |
|---|---|---|
| `revoke_device` (`commands.rs:345`) | `revokeDevice` (`commands.ts:13`) | both sides dead |
| `search_history` (`commands.rs:406`) | `searchHistory` (`commands.ts:17`) | both sides dead — `HistoryList.tsx:13-14` filters client-side instead |
| `delete_entry` (`commands.rs:486`) | `deleteEntry` (`commands.ts:20`) | both sides dead — no delete affordance exists in the UI |
| `clear_history` (`commands.rs:507`) | `clearHistory` (`commands.ts:21`) | both sides dead |
| `get_status` (`commands.rs:584`) | `getStatus` (`commands.ts:24`) | both sides dead **and redundant** — `list_accounts` already returns `status` and `pending` (`AccountSummary`, `commands.rs:27-28`) |
| `get_entry_full` (`commands.rs:436`) | *none* | no wrapper was ever written |

Also dead: the `openMainWindow` wrapper (`commands.ts:26`) — `openSection` (`commands.ts:29-32`) inlines both invokes rather than calling it.

`get_status` additionally returns a hardcoded `last_error: None` (`commands.rs:600`) while its own struct declares the field — the command could not report an error even if it were called.

### S4 — Three events are half-wired; two cause silent UX bugs, one is documented but unimplemented

| Event | Rust emit | TS subscription | Reality |
|---|---|---|---|
| `capture-skipped` | **never emitted** — only the const (`events.rs:11`) and struct (`events.rs:54`) exist; the skip path just logs and `continue`s (`lib.rs:822-825`) | `onCaptureSkipped` defined (`events.ts:13`), never called | fully dead **and** `README.md:95` instructs the tester to "confirm a `capture-skipped` toast" — a documented feature that does not exist |
| `history-changed` | emitted at `lib.rs:602` and `commands.rs:517` | `onHistoryChanged` defined (`events.ts:11`), never called | **bug:** the popover list does not refresh after a backfill or `clear_history` |
| `decryption-error` | emitted at `lib.rs:673` | `onDecryptionError` defined (`events.ts:14`), never called | **bug:** decryption failures are silent to the user |

### S5 — `spawn_sync` is a 258-line function

`lib.rs:511-768`. It performs task-slot registration, membership loading, the whole SSE lifecycle (backfill, reconnect, event dispatch — 158 lines, `:557-714`), and the uploader wiring. It also declares a trait implementation inside a spawned closure: `struct ServerUpload` + `impl UploadTransport for ServerUpload` at `lib.rs:725-731`, nested two levels inside an `async move` block. Its siblings already live in `core/sync/`; this function does not.

### S6 — 270 lines of popover geometry live in `lib.rs`

`lib.rs:143-411` — nine functions plus the `PopoverPlacement` enum, in the crate root next to app setup and tray wiring.

`PopoverPlacement` (`lib.rs:147-151`) exists to support one caller: `apply_hotkey` passes `Fallback` on Windows only (`lib.rs:899`), everything else passes `PreferTray`. `select_popover_tray_rect` (`lib.rs:237-247`) is a ten-line function whose entire `Fallback` branch is `None` (`:245`) — it exists so that returning `None` can be unit-tested (`lib.rs:988-1002`). Passing `None` for the rect at the one Windows call site expresses the same thing.

### S7 — `qr.rs` contains two copies of the same hex encoder, a needless sha2 wrapper, and no QR code

`core/pairing/qr.rs`:

- `hex_lower` (`:24-30`, inside `mod sha2_local`) and `hex_lower_static` (`:110-117`) are the same function written twice in one file.
- Both are redundant: `data-encoding` is already a dependency and already used in this file (`:120,125`) and in `shortcode.rs:2`. `data_encoding::HEXLOWER.encode()` replaces both.
- `mod sha2_local` (`:11-22`) wraps `sha2::Sha256` in a unit struct with one associated function, for one call site (`:55`).
- The module is named `qr.rs` but contains no QR code — the shortcode is base32 text (`shortcode.rs:2`). It holds pairing-payload crypto and base64 helpers.

### S8 — `InMemoryKeychain` is production code used only by tests

`core/keychain.rs:45-62`. Every reference is inside a `#[cfg(test)]` block: `keychain.rs:76,86`, `account/mod.rs:110,116`, `pairing/invite.rs:80,86`. Zero production callers.

### S9 — The entry DTO is defined three times

`commands::EntryViewDto` (`commands.rs:33-40`) and `events::EntryView` (`events.rs:37-44`) declare the same six fields with the same types and serialize identically; `ui/src/types.ts:3-10` mirrors them. Two Rust structs where one suffices.

### S10 — Everything is `pub`, which disables rustc's dead-code lint

`lib.rs:1-7` declares all seven modules `pub`, and nearly every item within them is `pub`. rustc exempts items reachable from a library's public API from `dead_code`. That is why `cargo check --all-targets` reports **zero warnings** while six commands (S3), one event (S4), and `InMemoryKeychain` (S8) sit unused.

Only `src-tauri/tests/*` consumes the public surface, and only four paths: `core::crypto`, `core::http`, `core::pairing::qr`, `errors`. Everything else can be `pub(crate)`, which turns the compiler into the dead-code detector this review had to be.

### S11 — `verify-release-packaging.mjs` asserts source text and is never invoked

52 lines (`scripts/verify-release-packaging.mjs`). Its four checks are: `main.rs` contains a literal `#![cfg_attr(...)]` string, `ui/public/favicon.ico` exists, and each HTML file contains a literal `<link rel="icon">`. These assert file contents, not behaviour — a build that produced a broken binary with the right source text would pass.

It is wired to `npm run test:packaging` (`package.json:9`) and invoked by nothing: no CI step, no Makefile target, no npm chain.

### S12 — Config, Makefile and README drift

- `tauri.macos.conf.json` is five lines setting `macOSPrivateApi: true`, applied only via the CI matrix arg (`desktop-build.yml:27`). `Cargo.toml:50` already enables the `macos-private-api` feature unconditionally for macOS targets. A `--config` file that CI can silently stop passing with no error is a fragile way to carry one boolean.
- Every `Makefile` target is gated on `check-macos`, yet CI builds Windows and `core/capture/windows.rs` exists. Windows contributors have no `make` path at all; this is undocumented.
- `README.md:59-61` states Windows output "depends on the selected Tauri bundle target" and that `cargo check` / `tauri dev` are "the supported compile/launch checks", but CI builds `--bundles nsis` (`desktop-build.yml:29`).
- `README.md:95` documents the non-existent `capture-skipped` toast (S4).

### S13 — Three gaps against the intended three-surface flow

The product shape is: (1) clipboard captured automatically with no user action, (2) a hotkey-summoned popover holding synced history, (3) a main window for settings and account management. Ranked here at S2-level severity despite the number — added in a second pass after the architecture was confirmed.

**S13a — "Launch at login" is a checkbox that does nothing.** `update_settings` persists `autostart` (`commands.rs:551-552`) and `SettingsSection.tsx:38-46` renders a checkbox bound to it, but no code ever calls the autostart plugin. `tauri_plugin_autostart` is only `init`'d (`lib.rs:37-40`); `ManagerExt::autolaunch().enable()/disable()` is never invoked anywhere in `src-tauri/src`. Contrast the sibling settings, which are all genuinely wired: `hotkey` re-registers via `apply_hotkey` (`commands.rs:568-571`), `capture_enabled` and `deny_list` are read by the capture filter (`lib.rs:813-814`). `autostart` is the one setting with a control, persistence, a passing round-trip test (`settings.rs:149,158`) — and no effect. It directly undermines flow (1): the app cannot start itself, so nothing is captured until the user launches it manually.

**S13b — No hotkey ships by default, so flow (2) does not exist out of the box.** `Settings::default()` sets `hotkey: None` (`settings.rs:21`), and `register_initial_hotkey` no-ops on `None` (`lib.rs:877`). Until the user discovers the Settings text field and types a valid accelerator, the popover is reachable only by clicking the tray icon. The hotkey is the intended primary entry point to the history; it should have a default (e.g. `CmdOrCtrl+Shift+V`), with `apply_hotkey` already handling the unbind case for users who clear it.

**S13c — The hotkey field re-registers a global shortcut on every keystroke.** `SettingsSection.tsx:58-63` calls `update({ hotkey: … })` from `onChange`. Typing `Ctrl+Shift+V` fires ~13 `update_settings` calls; each one runs `apply_hotkey`, which does `unregister_all()` then re-registers (`lib.rs:888`). Intermediate values (`C`, `Ct`, `Ctr`, …) fail registration and are swallowed as warnings (`commands.rs:569-570`). The final value does land, so this is not a correctness bug — but the shortcut is transiently unbound or bound to junk throughout, and every keystroke writes to SQLite. Commit on blur or Enter instead.

### S14 — The popover accumulates duplicate entries and its keyboard selection can leave the viewport

Observed in a running instance, then traced to source. Like S13, this ranks above its number.

**S14a — Nothing deduplicates by content, anywhere.** A live popover shows `/etc/docker/containers/duckdns` three times and one cron line twice. Three layers could have collapsed these; none does:

| Layer | Dedupe behaviour |
|---|---|
| `capture/filter.rs:56-59` | Skips only if the text equals the app's **own** last clipboard write inside `SELF_WRITE_WINDOW` — i.e. it prevents echo when the user copies *from* the popover. Re-copying the same text from another app is captured every time |
| `store/history.ts:20-23` `dedupePrepend` | Dedupes by `id`, and ids are always distinct for distinct captures — so for repeated copies it never collapses anything |
| `storage/entries_cache.rs` | No content uniqueness constraint |

Every capture is encrypted, queued, uploaded, and stored. With the popover capped at 100 rows (`history.ts:22`) and the cache capped by `MAX_PER_USER`, duplicates evict genuinely distinct history — the exact failure mode a clipboard manager exists to prevent.

The fix belongs in `capture/filter.rs`, **not** the UI: collapsing in the store would still burn a server row, a cache slot, and an upload. Add a "same as the previous capture" check alongside the existing self-write check — the filter already carries `(Instant, String)` state for `last_self_write`, so the shape is established. Decide explicitly whether a repeat should be dropped or should bump the existing entry to the top; dropping is simpler and matches most managers.

**S14b — Arrow-key selection never scrolls into view.** `HistoryList.tsx:22-27` moves `selectedIndex` on ArrowUp/ArrowDown, and `EntryRow` renders the highlight (`EntryRow.tsx:11`), but no `scrollIntoView` call exists anywhere in `ui/src/views` (grepped). The list is `overflow-auto` (`HistoryList.tsx:48`) and holds up to 100 rows against a 480px window showing ~13, so arrowing past the fold moves an invisible selection and Enter copies an entry the user cannot see. This is the primary interaction of flow (2).
  *Handled correctly and worth not breaking:* `setSearch` resets `selectedIndex` to 0 (`store/ui.ts:18`), so narrowing the filter can never strand the selection out of bounds.

**S14c — Enter is ambiguous when a footer button holds focus.** `HistoryList`'s keydown handler is bound to `window` with no check on `e.target` (`HistoryList.tsx:28-38,40`). `Search.tsx:25` autofocuses the input on open, but focus can move to the `Accounts`/`Settings` buttons (`Footer.tsx:16-17`) — a live instance shows the `Accounts` button focused. Enter in that state should fire both the button's `openSection` **and** the list's copy-and-hide. [INFERENCE — read from the handlers, not reproduced in a running build.] Scope the handler to the list, or ignore events whose target is a `button`.

---

## Plan

### Phase 1 — Make the main window hear events, and guard it

The window works; it receives nothing. One line fixes it, and one test keeps it fixed.

- [x] **Task 1.1: Cover the `main` window (S1).**
  `capabilities/default.json`: change `"windows"` to `["main", "popover"]`. Delete `"modal-*"`.
  Acceptance: open the tray menu → **Pair device… → I want to pair another device**, claim the code from a second instance, and confirm the window advances to the "paired" state. Before the fix it sits on the code screen forever, with `Command plugin:event|listen not allowed by ACL` in the webview console. Second check: with the window already open, tray → **Settings…** switches the tab.

- [x] **Task 1.2: Add a regression guard that does not mock the ACL (S1).**
  New Rust test in `lib.rs` (or a small `capabilities.rs`): parse `capabilities/default.json` at test time via `include_str!`, and assert the `windows` array covers every label the app builds. Keep a single `const WINDOW_LABELS: [&str; 2] = ["main", "popover"]` used by both `build_popover_window`/`open_main_window_impl` and the test, so adding a window without a capability fails the build.
  Acceptance: the test fails if `"main"` is removed from the capability file.

- [x] **Task 1.3: Run the desktop suite in CI (S2).**
  `.github/workflows/desktop-build.yml`: add a `test` job (macOS + Windows matrix) running `cargo test --lib --manifest-path clients/desktop/src-tauri/Cargo.toml` and `npm --prefix clients/desktop/ui ci && npm --prefix clients/desktop/ui test`. Make `build` depend on it.
  Acceptance: both matrix legs pass; a deliberately broken assertion fails the workflow.

- [x] **Task 1.4: Stop `cargo test` failing on a clean machine (S2).**
  `tests/common/mod.rs`: `start()` returns `Option<TestServer>`; on an unreachable health check, print a skip notice and return `None` instead of `expect`-panicking (`:40`). Each integration test returns early on `None`. Read the server URL from `SHAREPASTE_TEST_SERVER` with the current value as default.
  Acceptance: `cargo test` (all targets, no server running) is green and reports the three tests as skipped.

- [x] **Task 1.5: Add an opt-in CI job for the live-server tests (S2).**
  Separate job: `docker compose up -d --build`, wait for `/healthz`, then `cargo test --test flow1_invite --test flow2_pairing`. Gate on `workflow_dispatch` plus pushes touching `clients/desktop/src-tauri/src/core/http/**` or `core/pairing/**`.
  Acceptance: the job passes with the compose stack up.

### Phase 2 — Make the three surfaces behave as designed

Behaviour fixes only. Every item here is a feature that is present in the code but broken, inert, or unreachable.

**Auto-capture (surface 1)**

- [x] **Task 2.1: Make "Launch at login" actually work (S13a).**
  `update_settings` must call the autostart plugin, not just persist the boolean: on `autostart` change, `app.autolaunch().enable()` / `.disable()` via `tauri_plugin_autostart::ManagerExt`. Reconcile at startup too, so the OS state matches the stored setting after a reinstall.
  Acceptance: tick the box, then inspect the OS autostart entry — Registry `Run` key on Windows, `LaunchAgents` plist on macOS. Untick, confirm it is removed. The existing `settings.rs` round-trip tests do **not** cover this; they only prove the boolean persists, which is exactly how this shipped broken.

- [x] **Task 2.2: Drop consecutive duplicate captures (S14a).**
  `capture/filter.rs`: extend `CaptureContext` with the previously captured plaintext and return a new `SkipReason::Duplicate` when the incoming text matches. Store it beside `last_self_write` in `AppState` (`state.rs:25`) and set it in `spawn_clipboard_capture` after a successful enqueue (`lib.rs:842-858`). It must run *before* encrypt+enqueue so a repeat costs no server row, no cache slot, and no upload.
  Decide and document one rule: drop the repeat (recommended, simplest) or bump the existing entry to the top. Do not implement both.
  Acceptance: new cases in the Task T3 table-driven test — same text twice in a row yields `Capture` then `Skip(Duplicate)`; the same text with a different capture in between yields `Capture` twice.

**Popover (surface 2)**

- [x] **Task 2.3: Ship a default hotkey (S13b).**
  `Settings::default()` (`settings.rs:21`) → `hotkey: Some("CmdOrCtrl+Shift+V".into())`. Check it against OS-reserved combinations on both platforms; `apply_hotkey` already tolerates an unregistrable value by logging and continuing (`lib.rs:878-880`), and clearing the field still unbinds.
  Acceptance: fresh profile (`SHAREPASTE_DATA_DIR=/tmp/sp-fresh`), press the hotkey, popover appears — without visiting Settings first.

- [x] **Task 2.4: Keep the keyboard selection visible (S14b).**
  `HistoryList.tsx`: give the selected `EntryRow` a ref and call `scrollIntoView({ block: "nearest" })` from an effect keyed on `selectedIndex`.
  Acceptance: with >20 entries, holding ArrowDown keeps the highlighted row on screen to the last entry.

- [x] **Task 2.5: Disambiguate Enter (S14c).**
  `HistoryList.tsx:20-42`: ignore keydown events whose `e.target` is a `button`, or scope the listener to the list container instead of `window`.
  Acceptance: focus the footer **Accounts** button, press Enter — the main window opens and no entry is copied.

- [x] **Task 2.6: Give entries a delete affordance (S3).**
  `delete_entry` is a complete, tested backend feature with no UI — `EntryRow.tsx:20-22` renders a bare `<div>` and nothing else. Add a per-row delete control; the wrapper already exists (`commands.ts:20`).
  This is a privacy requirement, not tidiness: a live popover shows a Shadowsocks URL with embedded credentials and there is no way to remove a single entry short of purging everything from the server CLI.
  Acceptance: deleting a row removes it from the popover and the server.

- [x] **Task 2.7: Fix the two silent event bugs (S4).**
  Subscribe `onHistoryChanged` in `Popover.tsx` (re-run `cmd.listHistory` for the active user) and surface `onDecryptionError` somewhere visible. The events are already emitted; only the listeners are missing.
  Acceptance: `clear_history` empties the popover list without reopening it.

**Main window (surface 3)**

- [x] **Task 2.8: Add "Clear history" to Settings (S3).**
  `clear_history` has no affordance either; `SettingsSection.tsx` is its home. Wrapper exists (`commands.ts:21`). Confirm destructively (it deletes server-side for every device).
  Acceptance: the action empties both the popover and the server. Combined with Task 2.6, no command remains registered without a UI caller.

- [x] **Task 2.9: Stop the hotkey field thrashing the global shortcut (S13c).**
  `SettingsSection.tsx:58-63`: move `update({ hotkey })` from `onChange` to `onBlur` plus Enter, holding the in-progress string in local state.
  Acceptance: typing a full accelerator produces exactly one `update_settings` call.

### Phase 3 — Delete the surface nothing reaches

Run this *after* Phase 2, so the delete/wire decisions are already settled.

- [x] **Task 3.1: Delete the redundant status path (S3).**
  Remove `get_status` (`commands.rs:583-602`), `StatusResp` (`commands.rs:576-581`), the `generate_handler!` entry (`lib.rs:68`), and `cmd.getStatus` (`commands.ts:24-25`). `list_accounts` already carries `status` and `pending`.

- [x] **Task 3.2: Delete the unused window wrapper (S3).**
  Remove `cmd.openMainWindow` (`commands.ts:26-27`). `openSection` stays and keeps inlining both invokes.

- [x] **Task 3.3: Delete `revoke_device` and `search_history` (S3).**
  `search_history` duplicates client-side filtering that already works over the 100-row cache (`HistoryList.tsx:13-16`); server-side search only matters past that cap. Delete both commands, both wrappers, and the `revoke_device` command layer. Drop `entries_cache::search` and its test with it.
  Note `revoke_device` is exercised by `tests/auth_revoke.rs` against `ServerClient` directly, not through the command — the HTTP method stays either way.

- [x] **Task 3.4: Delete `get_entry_full` (S3).**
  The only command with no TS side at all. `copy_to_clipboard` already covers "user wants the plaintext" without handing it to the webview, which is the safer design. Remove `get_entry_full` (`commands.rs:435-443`) and its handler entry (`lib.rs:62`); keep `entries_cache::get_full`, which `copy_to_clipboard` uses (`commands.rs:475`).

- [x] **Task 3.5: Delete `capture-skipped` (S4).**
  Remove `CAPTURE_SKIPPED` (`events.rs:11`), `CaptureSkipped` (`events.rs:53-54`), `onCaptureSkipped` (`events.ts:13`), and the `README.md:95` smoke step promising the toast.
  Note Task 2.2 adds a *new* skip reason; if a skip toast is genuinely wanted, spec it then — do not keep an unemitted constant lying next to a `tracing::debug!` in the meantime.

### Phase 4 — Let the compiler find dead code (do this after Phase 3, not before)

- [x] **Task 4.1: Narrow the public surface (S10).**
  Keep `pub` only on what `src-tauri/tests/*` imports: `core::crypto::{encrypt, decrypt, random_user_key}`, `core::http::ServerClient`, `core::pairing::qr::{base64_encode, base64_decode}`, `errors::AppError`. Everything else → `pub(crate)`. Keep `pub` on `#[tauri::command]` functions and `launch`.
  Acceptance: `cargo check --all-targets` still clean; then deliberately orphan a function and confirm rustc now reports `never used`. Record any genuine dead code the lint surfaces and delete it.

- [x] **Task 4.2: Gate `InMemoryKeychain` to tests (S8).**
  `#[cfg(test)]` on the struct and its `impl` (`keychain.rs:45-62`). If `tests/*` needs it, use `#[cfg(any(test, feature = "test-support"))]` with a dev-only feature rather than shipping it.

### Phase 5 — Break up the two oversized modules

Pure moves. No behaviour change; the tests that exist must pass untouched.

- [x] **Task 5.1: Extract popover geometry (S6).**
  New `src-tauri/src/popover.rs` taking `lib.rs:143-411` — the constants, `build_popover_window`, `toggle_popover*`, positioning, monitor helpers — plus the three geometry tests (`lib.rs:964-1003`).
  Simultaneously drop `PopoverPlacement` and `select_popover_tray_rect`: give `toggle_popover(app, tray_rect: Option<Rect>, use_cached: bool)` the one behavioural bit the enum carried, and pass `false` at the Windows hotkey call site (`lib.rs:899`). Delete `fallback_placement_ignores_cached_tray_rect` (`lib.rs:988-1002`) with the function it was written for.
  Acceptance: `positions_popover_above_bottom_tray_and_inside_work_area` and `falls_back_to_bottom_right_when_taskbar_reduces_bottom_work_area` pass unchanged.

- [x] **Task 5.2: Move `spawn_sync` into `core/sync/` (S5).**
  New `core/sync/session.rs`. Split the 258-line body into: `run_session` (slot registration + membership + task spawn), `run_sse_loop` (`lib.rs:557-714`), and `run_uploader` (`lib.rs:716-767`). Hoist `struct ServerUpload` + its `UploadTransport` impl (`lib.rs:725-731`) to module scope — a trait impl does not belong inside a closure.
  `lib.rs` keeps only `set_conn_state` and `spawn_sync_for_existing_accounts`, or those move too.
  Acceptance: `cargo test --lib` unchanged; manual smoke — pair, copy text, confirm it syncs and the pending count drops.

- [x] **Task 5.3: Clean up `qr.rs` (S7).**
  Delete `mod sha2_local` (`:11-22`) and `hex_lower_static` (`:110-117`); use `sha2::{Digest, Sha256}` directly and `data_encoding::HEXLOWER` for both hex sites. Rename the file to `core/pairing/payload.rs` and update `core/pairing/mod.rs` plus the three `tests/*.rs` imports.
  Acceptance: `cargo test --lib` green; `flow2_pairing` still passes against a live server.

- [x] **Task 5.4: Collapse the duplicate entry DTO (S9).**
  Delete `commands::EntryViewDto` (`commands.rs:33-40`); have `list_history`/`search_history` return `events::EntryView`. Both already serialize identically, so `ui/src/types.ts` is untouched.

### Phase 6 — Build and docs

- [x] **Task 6.1: Delete `verify-release-packaging.mjs` (S11).**
  Remove the script and the `test:packaging` entry (`package.json:9`). The Windows-subsystem attribute it checks is better protected by the CI build actually producing an `.exe`; the favicon checks protect nothing.

- [x] **Task 6.2: Fold `tauri.macos.conf.json` into the main config (S12).**
  Move `macOSPrivateApi: true` into `tauri.conf.json`'s `app` block, delete the file, and drop `--config src-tauri/tauri.macos.conf.json` from the macOS matrix arg (`desktop-build.yml:27`).
  Acceptance: macOS CI leg still builds `app,dmg`.

- [x] **Task 6.3: Correct the README (S12, S4).**
  Fix `README.md:59-61` to state what CI actually produces (NSIS installer on Windows). Delete the `capture-skipped` toast step (`:95`). Add one line noting the `Makefile` is a macOS convenience wrapper and that Windows/Linux use `npm run` directly.

- [x] **Task 6.4: Document the Makefile's platform scope (S12).**
  One comment at the top of the `Makefile` stating macOS-only and pointing Windows users at `npm --prefix clients/desktop run build`. No new targets.

---

## Test Consolidation

Current: **133 defined** — 89 Rust inline (87 run, 2 ignored), 3 Rust integration (0 runnable), 41 UI. Target: **~108, all runnable, all in CI**.

The headline problem is not count, it is S1: 41 green UI tests while the window they cover is entirely non-functional, because every one of them mocks the IPC layer. Task 1.2 is the most valuable test in this plan.

### Task T1: Kill the mock boilerplate

`injectForTests` + `vi.fn()` scaffolding is rebuilt in six files (`AccountsSection.test.tsx:8-41`, `Main.test.tsx:8-19`, `PairingSection.test.tsx:10-23`, `Popover.test.tsx:10-41` plus per-test re-injection at `:66-67,85-86`, `SettingsSection.test.tsx:6-22`, `openSection.test.ts:6-13`). `Popover.test.tsx` spends ~60 lines on mock setup for ~38 lines of assertion.

- [x] Add `ui/src/__tests__/helpers.ts` exporting `mockIpc({ invoke?, listen? })` that installs the mocks, returns the spies, and registers `afterEach` cleanup. Replace all six ad-hoc blocks.
- [x] Remove the per-test re-injection in `Popover.test.tsx:66-67,85-86` — the shared `beforeEach` covers it.

### Task T2: Delete duplicated and tautological tests

| Delete | Reason | Contract preserved by |
|---|---|---|
| `commands.rs:690-700` `open_main_window_args_rejects_unknown_section` | **Tautology.** Calls no production code — it asserts that its own five fixture strings are absent from a hardcoded copy of the valid list. It cannot fail unless the test itself is edited | Extract `fn is_valid_section(&str) -> bool` from `open_main_window_impl:414` and test *that* with the same cases — one real test replacing one fake one |
| `lib.rs:988-1002` `fallback_placement_ignores_cached_tray_rect` | Tests `select_popover_tray_rect`, which exists only to be testable and returns `None` unconditionally on that branch | Deleted with the function in Task 4.1 |
| `capture/windows.rs:83-100` (2 tests) | Both `#[ignore]`d "does not panic" FFI smoke calls with no assertions. They are the 2 ignored tests in the baseline — they have never run | Nothing needed; the README Windows smoke checklist already covers this manually |
| `capture/macos.rs` ignored smoke tests | Same pattern | README macOS smoke checklist |
| `HistoryList.test.tsx` newest-first ordering | Ordering is the store's job, already asserted in `store.test.ts` history add/dedup | `store.test.ts` |
| `openSection.test.ts` (whole file, 1 test) | Asserts that a four-line helper issues two invokes in order — pure plumbing over mocks | Nothing needed. If `openSection` breaks, both `Footer.tsx` buttons break visibly |

Keep, against the initial audit's recommendation:

- **`decryptor.rs` `ingest_aad_mismatch_marks_undecryptable`** — not a duplicate of `crypto.rs` `aad_mismatch_fails`. Crypto asserts decryption *fails*; decryptor asserts the row is *marked undecryptable* and persisted. Different contracts, different layers.
- **`HistoryList.test.tsx` search filtering** — filtering is client-side in `HistoryList.tsx:13-14`, not in `entries_cache`. Nothing else covers it.

### Task T3: Consolidate over-granular tests

- [x] **`capture/filter.rs` 9 → 2.** `filter.rs:74-160` walks one decision tree with nine near-identical bodies. Replace with a table-driven test: one `cases: &[(CaptureContext, SniffResult, FilterDecision)]` array covering all nine, plus one focused test for the self-write time window (the only case with non-trivial timing). Same coverage, ~60 fewer lines.
- [x] **`storage/settings.rs` 8 → 5.** Three tests cover deny-list defaulting (`load_returns_default_when_unset`, deny-list defaults, `load_upgrades_existing_persisted_deny_list_with_windows_password_managers`). Collapse to one parameterized test over (stored value → expected list). Keep round-trip, `last_active_user_id` round-trip, missing-field, and case-insensitive dedup as-is.
- [x] **`Main.test.tsx` 6 → 4.** Merge the two URL-routing tests (valid section, fallback) into one parameterized case. Keep tab click, no-pairing-tab, and the navigate-event test.
- [x] **`AccountsSection.test.tsx` 7 → 5.** Drop the two tests that re-assert store hydrate/remove already owned by `store.test.ts:*`; keep badge rendering, the confirm strip, and the two invoke assertions (`set_active_account`, `forget_account`) which are the component's actual contract.
- [x] **`PairingSection.test.tsx` 11 → 6.** This is the largest UI file (182 lines) and its state machine (`chooser → invite → code → show-code → paired`) is genuinely the component's job — keep that. Drop the cases whose only assertion is that a mocked invoke resolved with the value the mock was configured to return. Keep: chooser rendering, show-code-disabled-without-account, code validation, shortcode display on `pair-shortcode`, `paired` on `pair-claimed`, and error rendering.
- [x] **`Popover.test.tsx` 3 → 2.** Merge the two navigate-on-empty-state cases (choose-account, empty-pair) — same assertion, different button.

### Task T4: Retire the un-runnable integration duplication

After Task 1.4 makes them skip cleanly:

- [x] **Delete `tests/auth_revoke.rs`.** Its contract — a revoked device token 401s on subsequent calls — is owned server-side by `server/tests/integration/auth.test.ts` and `devices.test.ts`, which run in `server-ci.yml` against real Fastify and real SQLite. The client adds nothing: `AppError::Auth` mapping from a 401 is already unit-tested at `core/http/client.rs` (`map_status`).
- [x] **Keep `flow1_invite.rs` and `flow2_pairing.rs`.** These exercise client-side crypto against a real server — the invite round trip and the pairing-payload encrypt/decrypt — which no server test covers. Both run in the Task 1.5 job.

### Resulting suite

| Suite | Before | After |
|---|---|---|
| Rust inline — `capture/filter.rs` | 9 | 2 |
| Rust inline — `capture/windows.rs` | 2 (both ignored) | 0 |
| Rust inline — `storage/settings.rs` | 8 | 5 |
| Rust inline — `lib.rs` / `popover.rs` | 3 | 2 (+1 capability guard, Task 1.2) |
| Rust inline — `commands.rs` | 3 | 2 (+1 real `is_valid_section` test) |
| Rust inline — all other modules | 64 | 64 |
| **Rust inline total** | **89** | **77** |
| Rust integration | 3 (0 runnable) | 2 (both runnable, opt-in CI) |
| UI — `openSection.test.ts` | 1 | 0 (deleted) |
| UI — `HistoryList.test.tsx` | 3 | 2 |
| UI — `Popover.test.tsx` | 3 | 2 |
| UI — `PairingSection.test.tsx` | 11 | 6 |
| UI — `AccountsSection.test.tsx` | 7 | 5 |
| UI — `Main.test.tsx` | 6 | 4 |
| UI — `SettingsSection.test.tsx` | 2 | 2 |
| UI — `store.test.ts` | 8 | 8 |
| **UI total** | **41** | **29** |
| **Total** | **133** | **108** |

Net: 1 UI test file deleted, 1 shared helper added, ~25 fewer tests, ~200 fewer lines of test code — and, for the first time, all of them run in CI.

> Rust counts are for the **Windows** target, matching the measured baseline. On macOS the totals shift by +3/−2: `capture/macos.rs` contributes 3 tests instead of `capture/windows.rs`'s 2, and its `#[ignore]`d smoke tests are deleted by the same T2 row.

### Coverage gaps worth closing

Short list, high value only:

1. **Capability coverage for every window label** — Task 1.2. Would have caught S1, which no existing test could see. Cheap, and it fails loudly the next time a window is added or renamed.
2. **A test that does not mock the IPC bridge.** Every UI test stubs `invoke`/`listen`. One end-to-end check — even a `tauri dev` smoke step in the Task 1.5 job asserting the main window loads its account list — would close the class of bug S1 belongs to.
3. **SSE reconnect and backoff after a dropped stream** (`lib.rs:692-712`). `state.rs` tests the backoff arithmetic; nothing tests that a dropped SSE stream actually reconnects and resumes from `last_seen_id`.

---

## Verification

Run after each phase, not once at the end:

```bash
cd clients/desktop/src-tauri && cargo test --lib && cargo check --all-targets
cd clients/desktop/ui && npm test
```

Phase 1 additionally requires a manual check, because the whole finding is that no automated test can see it. Note that the window renders and its commands work either way — you are testing whether it *hears*:

1. `cd clients/desktop && npm run dev`
2. Tray → **Pair device… → I want to pair another device**. Claim the code from a second instance (`SHAREPASTE_DATA_DIR=/tmp/sp-b npm run dev`). The window must advance to the paired state — before the fix it stays on the code screen, with `Command plugin:event|listen not allowed by ACL` in the webview console.
3. With the main window already open, tray → **Settings…** must switch the tab (`main://navigate`).
4. Pair or forget an account from a second instance; the Accounts list, `ACTIVE` badge and `status:` line must update without reopening the window.

Phase 2 requires checking the two settings that were silently inert: tick **Launch at login** and confirm the OS autostart entry appears (Registry `Run` key on Windows, `LaunchAgents` plist on macOS); then on a fresh profile confirm the default hotkey opens the popover before Settings is ever visited.

Phase 5 is a pure refactor: `cargo test --lib` must pass with the moved tests unmodified, plus one end-to-end smoke — pair, copy text in another app, confirm the entry appears in the popover and the pending count returns to zero.

---

## Execution Record — 2026-07-25

Branch `desktop-simplification`, six implementation commits, executed by five waves of subagents.

| Commit | Covers |
|---|---|
| `23aabc2` | Tasks 1.1, 1.2 — the ACL fix and its guard |
| `e34f273` | Tasks 1.3–1.5, 2.1–2.9, 6.1–6.4, T3 (Rust), T4 |
| `d3f5720` | Tasks 3.1–3.5, 4.2, 5.3, T1, T2, T3 (UI) |
| `356d20e` | Tasks 5.1, 5.4 |
| `fb974dd` | Task 5.2 |
| `c7ecbed` | Two defects found by Task 4.1 (below) |

### Measured outcome

| | Baseline | After |
|---|---|---|
| `cargo test` (all targets, clean machine) | **FAILS** — integration tests panic | **passes**, integration skips with a notice |
| `cargo check --all-targets` | 0 warnings *(lint disabled by blanket `pub`)* | 0 warnings *(lint live)* |
| Rust `pub` items | 374 | **64** (+283 `pub(crate)`) — 82.9% narrower |
| `lib.rs` | 1,184 lines | **538** |
| Rust lib tests | 89 defined / 87 run / 2 never run | **81, all run** |
| Rust integration tests | 3 defined / **0 runnable** | **2, both pass live** |
| UI tests | 41 | **44** |
| Desktop tests in CI | **none** | every one, macOS + Windows |

Total defined tests 133 → 127, but the comparison that matters is 130/133 runnable → **127/127**. The plan projected ~108; the extra 19 are genuine new coverage the plan did not budget for — 15 UI tests for the Phase 2 features (delete affordance, scroll-into-view, Enter disambiguation, the two event subscriptions, hotkey commit-on-blur, clear-history confirm), 3 capability-guard tests instead of 1, and a wire-format golden test added to de-risk Task 5.3. No planned deletion was skipped.

### Two defects found by Task 4.1, fixed in `c7ecbed`

Phase 4 predicted the `dead_code` lint would surface real problems once the crate stopped declaring everything public. It surfaced six warnings; four were dead code and deleted, two were bugs:

1. **Stale plaintext survived a decryption failure.** `entries_cache::mark_undecryptable` was written, unit-tested, and never called. `upsert_and_prune` COALESCEs a NULL incoming plaintext onto the stored one — right for a ciphertext-only backfill, wrong for a decryption failure, which is exactly when `decryptor::ingest` passes NULL. An entry that decrypted once and stopped kept its old plaintext, and `copy_to_clipboard` handed it back while the same ingest emitted `decryption-error` to the UI. The two disagreed. Fixed, with a test that fails without the fix.
2. **Pending-upload eviction was silent.** `EnqueueResult.dropped_oldest` counts un-uploaded entries discarded at the `MAX_PER_USER` cap. The capture loop discarded the value, so a user copying heavily while offline lost queued entries with nothing in the log. Now logged.

Both are instances of the pattern this review kept hitting: a feature fully built, persisted and tested, but never wired to anything.

### Corrections to this plan found during execution

- **Task 2.5** offered "scope the listener to the list container" as an equal alternative. It is not viable — the popover opens with focus on the search input, so a container-scoped listener never sees the arrow keys. The `e.target` filter is the only workable branch.
- **Task 2.8** did not account for `clear_history` being user-scoped while `SettingsSection` had no path to the active account; the section now hydrates the accounts store like `PairingSection` does.
- **Task 3.3** said to keep `ServerClient::revoke_device` because `tests/auth_revoke.rs` exercised it. T4 deletes that test, leaving zero callers, so the HTTP method went too.
- **Task 4.1**'s "keep `pub`" list was incomplete: `flow2_pairing.rs` also imports five items from `pairing::payload`, plus `pairing::shortcode::decode` and `pairing::invite::hex::decode_user_key`. `errors::AppError` stays public for a different reason than stated — no test imports it, but every public fallible signature returns it.
- **Task 5.3** referred to `core::pairing::qr`; the module is renamed to `payload` by that same task.
- **T2** said `capture/macos.rs` had 2 ignored smoke tests; it had 3.
- The integration harness invoked `/app/dist/src/index.js`, which does not exist in the image, and passed `--db /var/lib/sharepaste/db.sqlite`, which the server never reads. Both fixed; the flows now pass against a live compose stack.

### Still open (out of scope, worth tracking)

- The image has no `sharepaste` on `PATH`: `server/package.json` declares a `bin` entry but the Dockerfile never links it, and `docker exec` bypasses the `ENTRYPOINT` that would otherwise supply it. The root `README.md` documents `docker exec sharepaste sharepaste user create alice`, which fails. Server-side fix.
- `README.md:20` and `server/README.md:50` document `./db/db.sqlite`; the server uses `DB_PATH=/var/lib/sharepaste/sharepaste.sqlite`. Both databases exist in the volume. Tracked as task 2.5 of the server-simplification plan.
- SSE reconnect after a dropped stream is still untested (coverage gap 3 of this document).
