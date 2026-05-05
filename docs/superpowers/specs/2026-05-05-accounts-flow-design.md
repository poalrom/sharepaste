# Accounts flow: persistent active account and inline disconnect

## Goal

Streamline the account management flow in the desktop client so that:

1. The list in the Accounts modal shows an "Active" badge for the active account and a "Use" button for inactive ones.
2. Disconnecting an account uses an inline confirmation strip (no native browser `confirm()` dialog) triggered by a trash-bin icon next to the account row.
3. The last active account is restored automatically when the app starts.
4. When no account is active, the popover surfaces a clear placeholder pointing to the accounts modal.

## Non-goals

- No changes to pairing, sync, or history rendering beyond what is needed to keep them consistent with the new active-account lifecycle.
- No new account-level settings, multi-account simultaneous sync, or remote device management features.
- No tray-menu redesign.

## Background

Today the Accounts modal (`clients/desktop/ui/src/modals/AccountsModal.tsx`) keeps a local `active` state that starts `undefined` and only updates after a successful `set_active_account` call. Disconnecting goes through `window.confirm()`, which is jarring and inconsistent with the rest of the UI. On startup, `spawn_sync_for_existing_accounts` (`clients/desktop/src-tauri/src/lib.rs:425`) picks `accounts.first()` rather than the user's last-used account. The popover store (`clients/desktop/ui/src/store/accounts.ts`) infers the active account from connection status, which is a leaky proxy.

## Architecture

### Source of truth for the active account

`Settings.last_active_user_id: Option<String>` becomes the persisted source of truth, stored alongside the rest of the settings JSON in the existing `settings` sqlite table. The in-memory `AccountRegistry.active` (`clients/desktop/src-tauri/src/core/account/mod.rs:21`) acts as a cache; every mutation flows through a single helper that writes both.

### Lifecycle

1. **Startup.** `spawn_sync_for_existing_accounts` reads `Settings.last_active_user_id`. If the value is present and matches an account, that account becomes active. Otherwise the first account by `created_at` is activated and persisted. If there are no accounts, the active id stays `None`.
2. **`set_active_account` command.** Persists the new id, updates the registry cache, emits `ACTIVE_CHANGED`, and spawns the sync task as it does today.
3. **`forget_account` command.** Cancels any sync task for the forgotten user, runs the existing forget cleanup, and — if the forgotten account was active — promotes the oldest remaining account by `created_at`. If no accounts remain, the persisted active id is cleared. Events are emitted in this order: `ACTIVE_CHANGED` first (with the new active id or `None`), then `ACCOUNT_REMOVED`. This avoids an intermediate flicker where the UI sees an account-remove without a corresponding active update.
4. **`list_accounts` response.** Each `AccountSummary` includes a new `is_active: bool` field derived from the registry's current active id, replacing the status-based heuristic in the frontend store.

### Concurrency and ordering

The persistence helper accepts a borrowed `&Connection` so callers control the lock lifetime. Two flavors:

- `set_active_persisted_with(&self, conn: &Connection, user_id: Option<String>) -> Result<(), AppError>` — used by `forget`, which already holds the conn lock from the surrounding cleanup.
- `set_active_persisted(&self, user_id: Option<String>) -> Result<(), AppError>` — convenience wrapper used by `set_active_account` and startup, which acquire the lock internally.

Both write the settings row first and only update the in-memory `active` after the disk write succeeds, so the cache cannot diverge from disk on a successful return.

## Backend changes

### `core/storage/settings.rs`

- Add `pub last_active_user_id: Option<String>` to `Settings`.
- Mark the field with `#[serde(default)]` so existing rows without the field deserialize cleanly.
- `Default::default()` returns `None` for the new field.

### `core/account/mod.rs`

- New helper `set_active_persisted(&self, user_id: Option<String>) -> Result<(), AppError>`. Loads settings, writes `last_active_user_id`, saves, then updates `self.active`. Returns `AppError::Storage` on failure without touching the in-memory cache.
- New helper `load_persisted_active(&self) -> Result<Option<String>, AppError>` for startup, validating that the user id still appears in the account list.
- Modify `forget(...)` to return `Result<Option<String>, AppError>` where the `Option<String>` is the new active id (or `None` when no accounts remain or the forgotten user wasn't active). Internally:
  1. Delete keychain entries (existing behavior).
  2. Take the conn lock; delete the entries cache and the account row (existing behavior).
  3. If the forgotten user was active, query `accounts::list(&conn)` for remaining accounts, pick the first by `created_at` (or `None` if empty), and call `set_active_persisted_with(&conn, ...)`. If that call returns `Err`, set the in-memory `active` to `None` (best-effort, no further disk writes) and propagate the error.
  4. Otherwise leave the in-memory `active` untouched.
  5. Return the chosen new active id (only meaningful when the forgotten user was previously active).

Sync-task cancellation lives in the `forget_account` command, not in the registry, because the registry does not own the `sync_tasks` map.

### `commands.rs`

- `AccountSummary` gains `pub is_active: bool`.
- `list_accounts` computes `is_active = active.as_deref() == Some(&a.user_id)`.
- `set_active_account` calls `set_active_persisted` instead of bare `set_active`. The rest of the handler (event emit, sync spawn) is unchanged.
- `forget_account`:
  - Capture `was_active = state.registry.active_user_id().as_deref() == Some(&args.user_id)` before mutating state.
  - Cancel any sync slot for `args.user_id`: `state.sync_tasks.lock().remove(&args.user_id).map(|s| s.cancel.cancel())`.
  - Call `state.registry.forget(&args.user_id).await`. On `Ok(new_active)`:
    - If `was_active`, emit `ACTIVE_CHANGED { user_id: new_active.clone() }`.
    - Emit `ACCOUNT_REMOVED { user_id: args.user_id.clone() }`.
    - If `new_active.is_some()`, call `activate_and_sync` for that user. Note: `set_active_persisted_with` already updated the in-memory cache and disk, so `activate_and_sync` only needs to spawn sync (or refactor to a `spawn_sync_only` helper to avoid a redundant `set_active`).
  - On `Err(e)`: emit `ACTIVE_CHANGED { user_id: None }` (only if `was_active`), emit `ACCOUNT_REMOVED { user_id: args.user_id.clone() }`, then return the error.

### `lib.rs::spawn_sync_for_existing_accounts`

- Replace the `accounts.first()` heuristic with:
  1. `registry.load_persisted_active().await` — if `Some` and matches a current account, use it.
  2. Otherwise pick the first account by `created_at` and call `set_active_persisted` so the next startup honors it.
  3. If no accounts, leave active as `None` and skip sync spawn.
- Continue to emit `ACTIVE_CHANGED` and call `spawn_sync` as today.

## Frontend changes

### `ui/src/types.ts`

- `Account` gains `is_active: boolean`.

### `ui/src/store/accounts.ts`

- `hydrate(rows)` sets `active = rows.find((a) => a.is_active)?.user_id`. Drop the status-based fallback.
- `setActive(uid)` updates both `active` and the `is_active` flag on the cached `accounts` array so any view reading the array stays consistent without a refetch.
- `remove(uid)` keeps current behavior but stops guessing the next active id; backend events drive the new active.

### `ui/src/modals/AccountsModal.tsx`

- Drop local `active` state and the inline `confirm()` call.
- Read `accounts` from `useAccountsStore`. On mount, `cmd.listAccounts().then(hydrate)`.
- Subscribe to `events.onAccountAdded`, `events.onAccountRemoved`, `events.onActiveChanged` and refresh by re-hydrating from `cmd.listAccounts()` (simpler than incremental merges given the small list size).
- Component-local state `confirmingUserId: string | undefined`, with at most one row in confirmation mode at a time.
- Row layout per account:
  - Top row: label, meta (`user_id @ server_url`, status, pending count), right-aligned controls.
  - Right controls: an "Active" badge (non-interactive pill) when `a.is_active`, otherwise a "Use" button calling `cmd.setActiveAccount`. After the badge/button, a trash-bin icon button that sets `confirmingUserId = a.user_id`.
  - When `confirmingUserId === a.user_id`, render a confirmation strip below the row containing the prompt "Erase local history and key for {label}?", a Cancel button (clears `confirmingUserId`), and a destructive Forget button (calls `cmd.forgetAccount`, then clears `confirmingUserId`).
- Empty state (`accounts.length === 0`): heading "No accounts" and a body message with a button that calls `cmd.openModal("pairing")`.

The trash-bin icon uses an inline SVG (no new dependency) styled via Tailwind. The "Active" badge uses an existing Tailwind utility set such as `rounded bg-emerald-700 px-2 py-1 text-xs uppercase tracking-wide` (final classes selected during implementation review).

### `ui/src/views/Popover.tsx`

- Subscribe to `events.onAccountRemoved` → update the store via `useAccountsStore.getState().remove(user_id)`.
- Subscribe to `events.onActiveChanged` → call `useAccountsStore.getState().setActive(user_id ?? undefined)` and re-hydrate history for the new active id (or clear it when `user_id` is null).
- Add a placeholder branch:
  - `accounts.length === 0` keeps the existing "No accounts paired yet" placeholder with a "Pair a device" button.
  - `accounts.length > 0 && active === undefined` shows "No active account" with a "Choose account" button calling `cmd.openModal("accounts")`.

## Error handling

- **Settings write failure during activation.** `set_active_persisted` returns `AppError::Storage` and leaves the in-memory cache untouched. The originating Tauri command propagates the error; the modal renders it in its existing `error` slot.
- **Forget failure mid-flight.** Keychain or account-row delete failures bubble up as today; the row stays and no events fire. If the post-forget activation (the `set_active_persisted_with` call inside `forget`) fails after the row delete succeeded, the registry sets the in-memory `active` to `None` and returns `Err`. The `forget_account` command catches that error and, before propagating, still emits `ACCOUNT_REMOVED` (the row is gone) and `ACTIVE_CHANGED { user_id: None }` so the UI doesn't show a stale forgotten account or stale active id. The error is surfaced to the modal's error slot. The popover falls into the no-active-account placeholder and the user picks an account manually.
- **Startup load failure.** `load_persisted_active` errors are logged at warn level; the app falls back to the oldest account by `created_at`.
- **Stale persisted id.** A persisted `last_active_user_id` that no longer exists in the account list is ignored at startup; the fallback path runs.
- **Sync task on forgotten user.** The forget command cancels the sync slot before invoking `registry.forget` so the running task does not observe a half-deleted state. If the slot is missing (no active sync), the cancel is a no-op.

## Testing

### Rust unit tests (`core/account/mod.rs`)

- `set_active_persisted_writes_settings`: set active → reload settings → assert `last_active_user_id == Some(user_id)`.
- `set_active_persisted_none_clears_settings`: set `None` → reload → assert `None`.
- `forget_active_promotes_oldest_remaining`: insert three accounts with distinct `created_at`, activate the second, forget it, assert the oldest is now active and the settings row reflects it.
- `forget_only_account_clears_active`: single active account → forget → assert in-memory `None` and persisted `None`.
- `forget_inactive_keeps_active`: two accounts, activate the first, forget the second, assert the first is still active and the settings row is unchanged.
- The existing `forget_clears_keychain_and_db_and_active` test keeps passing.

### Rust storage tests (`core/storage/settings.rs`)

- Insert a legacy JSON value missing `last_active_user_id` → `load(...)` returns `Settings` with `last_active_user_id == None` (proves `#[serde(default)]`).

### Rust command tests (`commands.rs`)

- `list_accounts_marks_active`: two accounts seeded, registry activated for one → response has `is_active = true` for that user_id, `false` otherwise.

### Frontend tests (`ui/src/__tests__/`)

- `AccountsModal.test.tsx` (new):
  - Renders an "Active" badge for the active account and a "Use" button for the others, driven by `is_active` in the IPC mock.
  - Clicking the trash icon expands the confirmation strip; clicking Cancel collapses it.
  - Clicking Forget invokes `forget_account` and collapses the strip on resolution.
  - Clicking Use invokes `set_active_account`.
  - Empty list renders the pair-device button and clicking it invokes `open_modal` with `pairing`.
- `Popover.test.tsx`: add a case where `accounts.length > 0 && active === undefined` renders the "Choose account" placeholder; existing fixtures are updated to include `is_active`.

### Manual smoke test

1. Pair two devices, switch between them via Use, restart the app, and confirm the last-used account is auto-active.
2. Forget a non-active account; the list updates and the active account is unchanged.
3. Forget the active account; another account is promoted and sync resumes.
4. Forget the only remaining account; the popover shows the pair-device placeholder.
