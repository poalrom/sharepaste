# Accounts Flow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist the last-active account across restarts, replace the native disconnect dialog with an inline confirmation strip in the Accounts modal, and surface a no-active-account placeholder in the popover.

**Architecture:** A new `Settings.last_active_user_id` JSON field becomes the persisted source of truth. The in-memory `AccountRegistry.active` is the cache; one helper writes both. `list_accounts` exposes an explicit `is_active` flag, the frontend store reads it directly, and the Accounts modal subscribes to backend events for refresh. Forgetting the active account auto-promotes the oldest remaining account by `created_at`, or clears the active id when none remain.

**Tech Stack:** Rust (Tauri 2, rusqlite, tokio, parking_lot), TypeScript (React 18, Zustand, Vitest, Testing Library, Tauri JS API).

**Spec:** `docs/superpowers/specs/2026-05-05-accounts-flow-design.md`

---

## File Map

**Backend (Rust, `clients/desktop/src-tauri/`):**
- `src/core/storage/settings.rs` — add `last_active_user_id` field with `#[serde(default)]`; add round-trip test for legacy JSON.
- `src/core/account/mod.rs` — add `set_active_persisted_with`, `set_active_persisted`, `load_persisted_active`; change `forget` signature to `Result<Option<String>, AppError>` returning the new active id.
- `src/commands.rs` — add `is_active` to `AccountSummary`; route `set_active_account` through the new helper; rewrite `forget_account` to capture `was_active`, cancel sync, emit `ACTIVE_CHANGED` then `ACCOUNT_REMOVED`, and call sync spawn for the promoted user.
- `src/lib.rs` — `spawn_sync_for_existing_accounts` reads `load_persisted_active` first, falls back to oldest account.

**Frontend (TypeScript, `clients/desktop/ui/`):**
- `src/types.ts` — `Account.is_active: boolean`.
- `src/store/accounts.ts` — `hydrate` reads `is_active`; `setActive` flips flags on the cached array; `remove` no longer guesses next active.
- `src/modals/AccountsModal.tsx` — drop local active state and `confirm()`; inline trash icon + confirmation strip; subscribe to `account-added` / `account-removed` / `active-changed`; empty-state CTA opens pairing modal.
- `src/views/Popover.tsx` — subscribe to `account-removed` and `active-changed`; render a "Choose account" placeholder when accounts exist but none is active.
- `src/__tests__/store.test.ts` — update fixtures to use `is_active`.
- `src/__tests__/Popover.test.tsx` — update fixtures to `is_active`; add no-active placeholder case.
- `src/__tests__/PairingModal.test.tsx` — update fixtures to include `is_active` so the type compiles after the change.
- `src/__tests__/AccountsModal.test.tsx` — new file covering badge, Use button, trash → confirm strip, Cancel, Forget, empty state CTA.

---

## Build Commands

| Purpose | Command (run from repo root) |
|---|---|
| Rust: run all tests in the desktop crate | `cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml` |
| Rust: run a single test | `cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml -- <test_name>` |
| TS: run all UI tests | `cd clients/desktop/ui && npm test -- --run` |
| TS: run a single test file | `cd clients/desktop/ui && npm test -- --run src/__tests__/<file>` |
| TS: typecheck | `cd clients/desktop/ui && npx tsc --noEmit` |

`npm test` is wired to Vitest. Tests run headless. The `--run` flag forces one-shot execution (default in CI but not in interactive `npm test`).

If `cargo test` fails because the crate hasn't been built before, run it once without filters first; subsequent runs are incremental.

---

## Task 1: Persist `last_active_user_id` in settings

**Files:**
- Modify: `clients/desktop/src-tauri/src/core/storage/settings.rs`

- [ ] **Step 1: Write a failing test for legacy JSON deserialization**

Add this test inside the existing `mod tests` block at the bottom of `clients/desktop/src-tauri/src/core/storage/settings.rs`:

```rust
#[test]
fn load_returns_none_for_last_active_user_id_when_field_missing() {
    let c = open_in_memory().unwrap();
    let legacy = r#"{"capture_enabled":true,"deny_list":[],"autostart":false,"hotkey":null}"#;
    c.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)",
        params!["settings", legacy],
    ).unwrap();
    let s = load(&c).unwrap();
    assert!(s.last_active_user_id.is_none());
}
```

- [ ] **Step 2: Run the test and confirm it fails**

Run: `cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml -- load_returns_none_for_last_active_user_id_when_field_missing`
Expected: compile error — `Settings` has no field `last_active_user_id`.

- [ ] **Step 3: Add the field to `Settings`**

Edit `clients/desktop/src-tauri/src/core/storage/settings.rs`. Update the `Settings` struct and its `Default` impl:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub capture_enabled: bool,
    pub deny_list: Vec<String>,
    pub autostart: bool,
    pub hotkey: Option<String>,
    #[serde(default)]
    pub last_active_user_id: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        let mut settings = Settings {
            capture_enabled: true,
            deny_list: Vec::new(),
            autostart: false,
            hotkey: None,
            last_active_user_id: None,
        };
        append_builtin_deny_list_entries(&mut settings);
        settings
    }
}
```

- [ ] **Step 4: Add a round-trip test for the new field**

Append to `mod tests` in the same file:

```rust
#[test]
fn save_then_load_round_trips_last_active_user_id() {
    let c = open_in_memory().unwrap();
    let mut s = Settings::default();
    s.last_active_user_id = Some("user-1".into());
    save(&c, &s).unwrap();
    let loaded = load(&c).unwrap();
    assert_eq!(loaded.last_active_user_id, Some("user-1".into()));
}
```

- [ ] **Step 5: Run the test suite**

Run: `cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml -- settings`
Expected: PASS for both `load_returns_none_for_last_active_user_id_when_field_missing` and `save_then_load_round_trips_last_active_user_id`, plus the existing settings tests.

- [ ] **Step 6: Commit**

```bash
git add clients/desktop/src-tauri/src/core/storage/settings.rs
git commit -m "feat(desktop): persist last_active_user_id in settings"
```

---

## Task 2: Registry helpers for persisted active id

**Files:**
- Modify: `clients/desktop/src-tauri/src/core/account/mod.rs`

- [ ] **Step 1: Write a failing test for `set_active_persisted` writing the settings row**

Add inside `mod tests` of `clients/desktop/src-tauri/src/core/account/mod.rs`:

```rust
#[tokio::test]
async fn set_active_persisted_writes_settings_row() {
    let r = registry();
    {
        let c = r.conn.lock().await;
        accounts::upsert(&c, &Account {
            user_id: "u".into(), device_id: "d".into(), device_label: "mac".into(),
            server_url: "https://srv".into(), last_seen_id: 0, created_at: 1,
        }).unwrap();
    }
    r.set_active_persisted(Some("u".into())).await.unwrap();
    let c = r.conn.lock().await;
    let s = crate::core::storage::settings::load(&c).unwrap();
    assert_eq!(s.last_active_user_id, Some("u".into()));
    assert_eq!(r.active_user_id(), Some("u".into()));
}
```

- [ ] **Step 2: Run the test and confirm it fails**

Run: `cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml -- set_active_persisted_writes_settings_row`
Expected: compile error — `set_active_persisted` does not exist.

- [ ] **Step 3: Implement the helpers**

Edit `clients/desktop/src-tauri/src/core/account/mod.rs`. Replace the existing `set_active` impl block with the following (keeps the old in-memory-only setter as `set_active` for callers that don't need persistence, and adds the new persisted variants):

```rust
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

    pub fn set_active_persisted_with(
        &self,
        conn: &Connection,
        user_id: Option<String>,
    ) -> Result<(), AppError> {
        let mut s = crate::core::storage::settings::load(conn)?;
        s.last_active_user_id = user_id.clone();
        crate::core::storage::settings::save(conn, &s)?;
        *self.active.write() = user_id;
        Ok(())
    }

    pub async fn set_active_persisted(&self, user_id: Option<String>) -> Result<(), AppError> {
        let conn = self.conn.lock().await;
        self.set_active_persisted_with(&conn, user_id)
    }

    pub async fn load_persisted_active(&self) -> Result<Option<String>, AppError> {
        let conn = self.conn.lock().await;
        let s = crate::core::storage::settings::load(&conn)?;
        let Some(uid) = s.last_active_user_id else { return Ok(None) };
        match accounts::find(&conn, &uid)? {
            Some(_) => Ok(Some(uid)),
            None => Ok(None),
        }
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
```

(The `forget` body stays unchanged in this task; Task 4 changes its signature.)

- [ ] **Step 4: Run the test**

Run: `cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml -- set_active_persisted_writes_settings_row`
Expected: PASS.

- [ ] **Step 5: Add `set_active_persisted(None)` clears persisted id**

Add inside `mod tests`:

```rust
#[tokio::test]
async fn set_active_persisted_none_clears_settings_row() {
    let r = registry();
    {
        let c = r.conn.lock().await;
        accounts::upsert(&c, &Account {
            user_id: "u".into(), device_id: "d".into(), device_label: "mac".into(),
            server_url: "https://srv".into(), last_seen_id: 0, created_at: 1,
        }).unwrap();
    }
    r.set_active_persisted(Some("u".into())).await.unwrap();
    r.set_active_persisted(None).await.unwrap();
    let c = r.conn.lock().await;
    let s = crate::core::storage::settings::load(&c).unwrap();
    assert!(s.last_active_user_id.is_none());
    assert!(r.active_user_id().is_none());
}
```

- [ ] **Step 6: Add `load_persisted_active` test for happy path and stale id**

```rust
#[tokio::test]
async fn load_persisted_active_returns_id_when_account_exists() {
    let r = registry();
    {
        let c = r.conn.lock().await;
        accounts::upsert(&c, &Account {
            user_id: "u".into(), device_id: "d".into(), device_label: "mac".into(),
            server_url: "https://srv".into(), last_seen_id: 0, created_at: 1,
        }).unwrap();
    }
    r.set_active_persisted(Some("u".into())).await.unwrap();
    assert_eq!(r.load_persisted_active().await.unwrap(), Some("u".into()));
}

#[tokio::test]
async fn load_persisted_active_returns_none_when_account_missing() {
    let r = registry();
    {
        let c = r.conn.lock().await;
        let mut s = crate::core::storage::settings::Settings::default();
        s.last_active_user_id = Some("ghost".into());
        crate::core::storage::settings::save(&c, &s).unwrap();
    }
    assert!(r.load_persisted_active().await.unwrap().is_none());
}
```

- [ ] **Step 7: Run all account-module tests**

Run: `cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml -- account::`
Expected: all four new tests plus the existing two pass.

- [ ] **Step 8: Commit**

```bash
git add clients/desktop/src-tauri/src/core/account/mod.rs
git commit -m "feat(desktop): add persisted active-account helpers"
```

---

## Task 3: Auto-promotion on `forget`

**Files:**
- Modify: `clients/desktop/src-tauri/src/core/account/mod.rs`

- [ ] **Step 1: Update existing forget test for new signature**

The existing `forget_clears_keychain_and_db_and_active` test calls `r.forget("u").await.unwrap();` and expects `()`. Change it to handle the new `Option<String>` return value. Replace that call with:

```rust
let new_active = r.forget("u").await.unwrap();
assert!(new_active.is_none());
```

- [ ] **Step 2: Add a failing test for promotion to oldest remaining**

Append inside `mod tests`:

```rust
#[tokio::test]
async fn forget_active_promotes_oldest_remaining() {
    let r = registry();
    let kc = r.keychain.clone();
    for (uid, created_at) in [("oldest", 1i64), ("middle", 2), ("newest", 3)] {
        kc.put(&format!("{uid}:key"), &"ab".repeat(32)).unwrap();
        kc.put(&format!("{uid}:token"), "tok").unwrap();
        let c = r.conn.lock().await;
        accounts::upsert(&c, &Account {
            user_id: uid.into(), device_id: "d".into(), device_label: uid.into(),
            server_url: "https://srv".into(), last_seen_id: 0, created_at,
        }).unwrap();
    }
    r.set_active_persisted(Some("middle".into())).await.unwrap();
    let new_active = r.forget("middle").await.unwrap();
    assert_eq!(new_active, Some("oldest".into()));
    assert_eq!(r.active_user_id(), Some("oldest".into()));
    let c = r.conn.lock().await;
    let s = crate::core::storage::settings::load(&c).unwrap();
    assert_eq!(s.last_active_user_id, Some("oldest".into()));
}

#[tokio::test]
async fn forget_inactive_keeps_active_unchanged() {
    let r = registry();
    let kc = r.keychain.clone();
    for uid in ["a", "b"] {
        kc.put(&format!("{uid}:key"), &"ab".repeat(32)).unwrap();
        kc.put(&format!("{uid}:token"), "tok").unwrap();
        let c = r.conn.lock().await;
        accounts::upsert(&c, &Account {
            user_id: uid.into(), device_id: "d".into(), device_label: uid.into(),
            server_url: "https://srv".into(), last_seen_id: 0, created_at: 1,
        }).unwrap();
    }
    r.set_active_persisted(Some("a".into())).await.unwrap();
    let new_active = r.forget("b").await.unwrap();
    assert!(new_active.is_none());
    assert_eq!(r.active_user_id(), Some("a".into()));
    let c = r.conn.lock().await;
    let s = crate::core::storage::settings::load(&c).unwrap();
    assert_eq!(s.last_active_user_id, Some("a".into()));
}

#[tokio::test]
async fn forget_only_active_account_clears_persisted_id() {
    let r = registry();
    let kc = r.keychain.clone();
    kc.put("u:key", &"ab".repeat(32)).unwrap();
    kc.put("u:token", "tok").unwrap();
    {
        let c = r.conn.lock().await;
        accounts::upsert(&c, &Account {
            user_id: "u".into(), device_id: "d".into(), device_label: "u".into(),
            server_url: "https://srv".into(), last_seen_id: 0, created_at: 1,
        }).unwrap();
    }
    r.set_active_persisted(Some("u".into())).await.unwrap();
    let new_active = r.forget("u").await.unwrap();
    assert!(new_active.is_none());
    assert!(r.active_user_id().is_none());
    let c = r.conn.lock().await;
    let s = crate::core::storage::settings::load(&c).unwrap();
    assert!(s.last_active_user_id.is_none());
}
```

- [ ] **Step 3: Run the new tests and confirm they fail**

Run: `cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml -- forget_`
Expected: type-mismatch errors — `forget` returns `()` and the new tests expect `Option<String>`.

- [ ] **Step 4: Update `forget` to return `Option<String>`**

Replace the existing `forget` body in `clients/desktop/src-tauri/src/core/account/mod.rs`:

```rust
pub async fn forget(&self, user_id: &str) -> Result<Option<String>, AppError> {
    self.keychain.delete(&user_key_account(user_id))?;
    self.keychain.delete(&token_account(user_id))?;
    let conn = self.conn.lock().await;
    crate::core::storage::entries_cache::delete_all(&conn, user_id)?;
    accounts::delete(&conn, user_id)?;
    let was_active = self.active.read().as_deref() == Some(user_id);
    if !was_active {
        return Ok(None);
    }
    let next = accounts::list(&conn)?.into_iter().next().map(|a| a.user_id);
    if let Err(e) = self.set_active_persisted_with(&conn, next.clone()) {
        *self.active.write() = None;
        return Err(e);
    }
    Ok(next)
}
```

`accounts::list` already orders `ORDER BY created_at ASC` (`clients/desktop/src-tauri/src/core/storage/accounts.rs:30`), so `.into_iter().next()` is the oldest remaining account.

- [ ] **Step 5: Run all account tests**

Run: `cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml -- account::`
Expected: PASS for all six tests.

- [ ] **Step 6: Commit**

```bash
git add clients/desktop/src-tauri/src/core/account/mod.rs
git commit -m "feat(desktop): auto-promote oldest account on forget"
```

---

## Task 4: Expose `is_active` in `list_accounts`

**Files:**
- Modify: `clients/desktop/src-tauri/src/commands.rs`

- [ ] **Step 1: Locate the `AccountSummary` struct and `list_accounts` handler**

They live at `clients/desktop/src-tauri/src/commands.rs:21-29` and `:41-66`. Read both to recall context.

- [ ] **Step 2: Add `is_active` field to `AccountSummary`**

Replace the struct definition:

```rust
#[derive(Serialize)]
pub struct AccountSummary {
    pub user_id: String,
    pub device_id: String,
    pub label: String,
    pub server_url: String,
    pub status: ConnectionState,
    pub pending: i64,
    pub is_active: bool,
}
```

- [ ] **Step 3: Populate `is_active` in `list_accounts`**

Replace the body of `list_accounts` (`commands.rs:41-66`):

```rust
#[tauri::command]
pub async fn list_accounts(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<AccountSummary>, AppError> {
    let accts = state.registry.list().await?;
    let active = state.registry.active_user_id();
    let mut out = Vec::with_capacity(accts.len());
    let conn = state.conn.lock().await;
    for a in accts {
        let pending = pending::count(&conn, &a.user_id)?;
        let is_active = active.as_deref() == Some(&a.user_id);
        let status = if is_active {
            ConnectionState::Connecting
        } else {
            ConnectionState::Disconnected
        };
        out.push(AccountSummary {
            user_id: a.user_id,
            device_id: a.device_id,
            label: a.device_label,
            server_url: a.server_url,
            status,
            pending,
            is_active,
        });
    }
    Ok(out)
}
```

- [ ] **Step 4: Confirm the crate still compiles**

Run: `cargo check --manifest-path clients/desktop/src-tauri/Cargo.toml`
Expected: clean compile (no test runs yet).

- [ ] **Step 5: Commit**

```bash
git add clients/desktop/src-tauri/src/commands.rs
git commit -m "feat(desktop): include is_active in AccountSummary"
```

---

## Task 5: Route `set_active_account` through persistence

**Files:**
- Modify: `clients/desktop/src-tauri/src/commands.rs`

- [ ] **Step 1: Update `set_active_account` and `activate_and_sync`**

In `clients/desktop/src-tauri/src/commands.rs`, replace the body of `set_active_account` (`commands.rs:320-335`) and `activate_and_sync` (`commands.rs:605-614`):

```rust
#[tauri::command]
pub async fn set_active_account(
    args: UserScopedArgs,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), AppError> {
    state.registry.set_active_persisted(Some(args.user_id.clone())).await?;
    app.emit(
        ACTIVE_CHANGED,
        crate::events::ActiveChanged {
            user_id: Some(args.user_id.clone()),
        },
    )
    .ok();
    crate::spawn_sync(app.clone(), Arc::clone(state.inner()), args.user_id).await;
    Ok(())
}

async fn activate_and_sync(app: &AppHandle, state: &Arc<AppState>, user_id: &str) {
    if let Err(e) = state
        .registry
        .set_active_persisted(Some(user_id.to_string()))
        .await
    {
        tracing::warn!(err = %e, "persisting active account failed");
    }
    let _ = app.emit(
        ACTIVE_CHANGED,
        crate::events::ActiveChanged {
            user_id: Some(user_id.to_string()),
        },
    );
    crate::spawn_sync(app.clone(), Arc::clone(state), user_id.to_string()).await;
}
```

`set_active_account` previously emitted only `ACTIVE_CHANGED` and relied on the active sync task being spawned elsewhere. Confirm by grepping that `set_active_account` callers don't depend on the old behavior:

Run: `grep -rn "set_active_account\|setActiveAccount" clients/desktop`
Expected: only the Tauri command definition, the IPC wrapper, the React modal handler, and tests.

- [ ] **Step 2: Confirm crate compiles and tests still pass**

Run: `cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml`
Expected: PASS for all existing and new tests.

- [ ] **Step 3: Commit**

```bash
git add clients/desktop/src-tauri/src/commands.rs
git commit -m "feat(desktop): persist active account on set_active and pairing"
```

---

## Task 6: Rewrite `forget_account` command

**Files:**
- Modify: `clients/desktop/src-tauri/src/commands.rs`

- [ ] **Step 1: Replace the `forget_account` command body**

In `clients/desktop/src-tauri/src/commands.rs`, replace the existing `forget_account` (`commands.rs:288-303`) with:

```rust
#[tauri::command]
pub async fn forget_account(
    args: UserScopedArgs,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), AppError> {
    let was_active = state
        .registry
        .active_user_id()
        .as_deref()
        == Some(args.user_id.as_str());

    if let Some(slot) = state.sync_tasks.lock().remove(&args.user_id) {
        slot.cancel.cancel();
    }

    let result = state.registry.forget(&args.user_id).await;

    let new_active = match &result {
        Ok(next) => next.clone(),
        Err(_) => None,
    };

    if was_active {
        app.emit(
            ACTIVE_CHANGED,
            crate::events::ActiveChanged {
                user_id: new_active.clone(),
            },
        )
        .ok();
    }
    app.emit(
        crate::events::ACCOUNT_REMOVED,
        crate::events::AccountRemoved {
            user_id: args.user_id.clone(),
        },
    )
    .ok();

    result?;

    if let Some(uid) = new_active {
        crate::spawn_sync(app.clone(), Arc::clone(state.inner()), uid).await;
    }

    Ok(())
}
```

The flow:
1. Snapshot `was_active` before mutating state.
2. Cancel the sync slot for the forgotten user (no-op if absent).
3. Call `registry.forget`, which already updates the persisted active id internally.
4. Emit `ACTIVE_CHANGED` (only if the forgotten user was active) before `ACCOUNT_REMOVED` so the popover never sees a stale active id while the account list is shrinking.
5. Propagate any error from `forget` (after emitting events) so the modal can surface it.
6. Spawn sync for the promoted account, if any.

- [ ] **Step 2: Confirm everything still builds**

Run: `cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml`
Expected: all tests pass; no new tests required at the command layer (the registry-level tests cover the promotion logic, and the wiring is exercised manually by the smoke test in Task 13).

- [ ] **Step 3: Commit**

```bash
git add clients/desktop/src-tauri/src/commands.rs
git commit -m "feat(desktop): rewrite forget_account to drive promotion events"
```

---

## Task 7: Honor persisted active id at startup

**Files:**
- Modify: `clients/desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: Replace `spawn_sync_for_existing_accounts`**

In `clients/desktop/src-tauri/src/lib.rs`, replace the function (`lib.rs:425-439`) with:

```rust
fn spawn_sync_for_existing_accounts(app: tauri::AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let persisted = match state.registry.load_persisted_active().await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(err = %e, "load persisted active failed");
                None
            }
        };
        let chosen = if let Some(uid) = persisted {
            Some(uid)
        } else {
            state
                .registry
                .list()
                .await
                .unwrap_or_default()
                .into_iter()
                .next()
                .map(|a| a.user_id)
        };
        let Some(user_id) = chosen else { return };
        if let Err(e) = state
            .registry
            .set_active_persisted(Some(user_id.clone()))
            .await
        {
            tracing::warn!(err = %e, "persist active on startup failed");
        }
        let _ = app.emit(
            ACTIVE_CHANGED,
            crate::events::ActiveChanged {
                user_id: Some(user_id.clone()),
            },
        );
        spawn_sync(app.clone(), state.clone(), user_id).await;
    });
}
```

- [ ] **Step 2: Build and run the test suite**

Run: `cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add clients/desktop/src-tauri/src/lib.rs
git commit -m "feat(desktop): restore last active account on app start"
```

---

## Task 8: Add `is_active` to the TS `Account` type

**Files:**
- Modify: `clients/desktop/ui/src/types.ts`

- [ ] **Step 1: Update the type**

Replace the `Account` declaration in `clients/desktop/ui/src/types.ts`:

```typescript
export type Account = {
  user_id: string;
  device_id: string;
  label: string;
  server_url: string;
  status: ConnectionState;
  pending: number;
  is_active: boolean;
};
```

- [ ] **Step 2: Update test fixtures so TS still compiles**

`is_active` is required, so all existing fixtures must declare it. Update `clients/desktop/ui/src/__tests__/store.test.ts`:

Replace the entire `describe("accounts store", ...)` block with:

```typescript
describe("accounts store", () => {
  beforeEach(() => useAccountsStore.setState({ accounts: [], active: undefined }));

  it("hydrate sets active to the row flagged is_active", () => {
    useAccountsStore.getState().hydrate([
      { user_id: "a", device_id: "d", label: "x", server_url: "https://s", status: "Online", pending: 0, is_active: true },
      { user_id: "b", device_id: "d", label: "y", server_url: "https://s", status: "Disconnected", pending: 0, is_active: false },
    ]);
    expect(useAccountsStore.getState().active).toBe("a");
  });

  it("hydrate leaves active undefined when no row is flagged", () => {
    useAccountsStore.getState().hydrate([
      { user_id: "a", device_id: "d", label: "x", server_url: "https://s", status: "Disconnected", pending: 0, is_active: false },
    ]);
    expect(useAccountsStore.getState().active).toBeUndefined();
  });

  it("removing a non-active account leaves active alone", () => {
    useAccountsStore.getState().hydrate([
      { user_id: "a", device_id: "d", label: "x", server_url: "https://s", status: "Online", pending: 0, is_active: true },
      { user_id: "b", device_id: "d", label: "y", server_url: "https://s", status: "Disconnected", pending: 0, is_active: false },
    ]);
    useAccountsStore.getState().remove("b");
    expect(useAccountsStore.getState().active).toBe("a");
  });

  it("removing the active account clears active and waits for backend", () => {
    useAccountsStore.getState().hydrate([
      { user_id: "a", device_id: "d", label: "x", server_url: "https://s", status: "Online", pending: 0, is_active: true },
      { user_id: "b", device_id: "d", label: "y", server_url: "https://s", status: "Disconnected", pending: 0, is_active: false },
    ]);
    useAccountsStore.getState().remove("a");
    expect(useAccountsStore.getState().active).toBeUndefined();
  });
});
```

(The "removing the active account" test will start failing after this edit and stays failing until Task 9 lands, but the typecheck must succeed now. Run `npx tsc --noEmit` only.)

Update `clients/desktop/ui/src/__tests__/Popover.test.tsx`:

Replace the `accounts` array (`Popover.test.tsx:10-27`) with:

```typescript
const accounts: Account[] = [
  {
    user_id: "u-oldest",
    device_id: "d-oldest",
    label: "Oldest",
    server_url: "https://srv",
    status: "Disconnected",
    pending: 0,
    is_active: false,
  },
  {
    user_id: "u-active",
    device_id: "d-active",
    label: "Active",
    server_url: "https://srv",
    status: "Connecting",
    pending: 0,
    is_active: true,
  },
];
```

Update `clients/desktop/ui/src/__tests__/PairingModal.test.tsx`. Find each `list_accounts` mock that returns an `Account` array and ensure each fixture has `is_active: false` (or `true` if the test needs an active account). Search the file:

Run: `grep -n "list_accounts\|user_id" clients/desktop/ui/src/__tests__/PairingModal.test.tsx`

For every account literal (objects with `user_id`, `device_id`, `label`, `server_url`, `status`, `pending`), append `, is_active: false`.

- [ ] **Step 3: Run typecheck**

Run: `cd clients/desktop/ui && npx tsc --noEmit`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add clients/desktop/ui/src/types.ts clients/desktop/ui/src/__tests__/store.test.ts clients/desktop/ui/src/__tests__/Popover.test.tsx clients/desktop/ui/src/__tests__/PairingModal.test.tsx
git commit -m "feat(desktop-ui): add is_active to Account type and fixtures"
```

---

## Task 9: Update accounts store to use `is_active`

**Files:**
- Modify: `clients/desktop/ui/src/store/accounts.ts`

- [ ] **Step 1: Run the store tests and confirm they fail**

Run: `cd clients/desktop/ui && npm test -- --run src/__tests__/store.test.ts`
Expected: the "hydrate sets active to the row flagged is_active" and "removing the active account clears active and waits for backend" tests fail because the current store uses status-based logic.

- [ ] **Step 2: Replace the store implementation**

Replace the body of `clients/desktop/ui/src/store/accounts.ts`:

```typescript
import { create } from "zustand";
import type { Account } from "../types";

export type AccountsState = {
  accounts: Account[];
  active: string | undefined;
  hydrate: (rows: Account[]) => void;
  upsert: (a: Account) => void;
  remove: (user_id: string) => void;
  setActive: (user_id: string | undefined) => void;
};

export const useAccountsStore = create<AccountsState>((set) => ({
  accounts: [],
  active: undefined,
  hydrate: (rows) =>
    set({
      accounts: rows,
      active: rows.find((a) => a.is_active)?.user_id,
    }),
  upsert: (a) =>
    set((s) => {
      const without = s.accounts.filter((x) => x.user_id !== a.user_id);
      return { accounts: [...without, a] };
    }),
  remove: (uid) =>
    set((s) => ({
      accounts: s.accounts.filter((a) => a.user_id !== uid),
      active: s.active === uid ? undefined : s.active,
    })),
  setActive: (active) =>
    set((s) => ({
      active,
      accounts: s.accounts.map((a) => ({ ...a, is_active: a.user_id === active })),
    })),
}));
```

- [ ] **Step 3: Run the store tests**

Run: `cd clients/desktop/ui && npm test -- --run src/__tests__/store.test.ts`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add clients/desktop/ui/src/store/accounts.ts
git commit -m "feat(desktop-ui): drive accounts store from is_active flag"
```

---

## Task 10: Rewrite `AccountsModal`

**Files:**
- Modify: `clients/desktop/ui/src/modals/AccountsModal.tsx`
- Create: `clients/desktop/ui/src/__tests__/AccountsModal.test.tsx`

- [ ] **Step 1: Write the failing test file**

Create `clients/desktop/ui/src/__tests__/AccountsModal.test.tsx`:

```typescript
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { injectForTests, type Invoker, type Listener } from "../ipc/tauri";
import { useAccountsStore } from "../store";
import type { Account } from "../types";
import AccountsModal from "../modals/AccountsModal";

const accounts: Account[] = [
  { user_id: "u-active", device_id: "d1", label: "Laptop", server_url: "https://srv", status: "Connecting", pending: 0, is_active: true },
  { user_id: "u-other", device_id: "d2", label: "Desktop", server_url: "https://srv", status: "Disconnected", pending: 0, is_active: false },
];

let invoke: ReturnType<typeof vi.fn<Invoker>>;
let currentAccounts: Account[];

beforeEach(() => {
  currentAccounts = [...accounts];
  invoke = vi.fn(async (cmd, payload) => {
    if (cmd === "list_accounts") return currentAccounts;
    if (cmd === "set_active_account") {
      const target = (payload as { args: { user_id: string } }).args.user_id;
      currentAccounts = currentAccounts.map((a) => ({ ...a, is_active: a.user_id === target }));
      return undefined;
    }
    if (cmd === "forget_account") {
      const target = (payload as { args: { user_id: string } }).args.user_id;
      currentAccounts = currentAccounts.filter((a) => a.user_id !== target);
      return undefined;
    }
    if (cmd === "open_modal") return undefined;
    return undefined;
  }) as ReturnType<typeof vi.fn<Invoker>>;
  const listen = vi.fn(async () => () => {}) as ReturnType<typeof vi.fn<Listener>>;
  injectForTests(invoke as never, listen as never);
  useAccountsStore.setState({ accounts: [], active: undefined });
});

describe("AccountsModal", () => {
  it("renders Active badge for the active account and Use button for others", async () => {
    render(<AccountsModal />);
    await waitFor(() => expect(screen.getByText("Laptop")).toBeInTheDocument());
    expect(screen.getByTestId("active-badge-u-active")).toBeInTheDocument();
    expect(screen.getByTestId("use-u-other")).toBeInTheDocument();
    expect(screen.queryByTestId("active-badge-u-other")).toBeNull();
  });

  it("clicking trash opens an inline confirmation strip below the row", async () => {
    render(<AccountsModal />);
    await waitFor(() => expect(screen.getByText("Laptop")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("trash-u-other"));
    expect(screen.getByTestId("confirm-strip-u-other")).toBeInTheDocument();
    expect(screen.queryByTestId("confirm-strip-u-active")).toBeNull();
  });

  it("Cancel collapses the confirmation strip without invoking forget", async () => {
    render(<AccountsModal />);
    await waitFor(() => expect(screen.getByText("Laptop")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("trash-u-other"));
    fireEvent.click(screen.getByTestId("cancel-u-other"));
    expect(screen.queryByTestId("confirm-strip-u-other")).toBeNull();
    expect(invoke).not.toHaveBeenCalledWith("forget_account", expect.anything());
  });

  it("Forget invokes forget_account and clears the strip", async () => {
    render(<AccountsModal />);
    await waitFor(() => expect(screen.getByText("Laptop")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("trash-u-other"));
    fireEvent.click(screen.getByTestId("confirm-forget-u-other"));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("forget_account", { args: { user_id: "u-other" } }),
    );
    await waitFor(() => expect(screen.queryByText("Desktop")).toBeNull());
  });

  it("Use invokes set_active_account", async () => {
    render(<AccountsModal />);
    await waitFor(() => expect(screen.getByText("Laptop")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("use-u-other"));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("set_active_account", { args: { user_id: "u-other" } }),
    );
  });

  it("renders empty state and opens pairing modal", async () => {
    currentAccounts = [];
    render(<AccountsModal />);
    await waitFor(() => expect(screen.getByTestId("empty-pair")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("empty-pair"));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("open_modal", { args: { kind: "pairing" } }),
    );
  });
});
```

- [ ] **Step 2: Run the test and confirm it fails**

Run: `cd clients/desktop/ui && npm test -- --run src/__tests__/AccountsModal.test.tsx`
Expected: FAIL — current modal lacks `data-testid` values like `active-badge-*`, `use-*`, `trash-*`, `confirm-strip-*`, `cancel-*`, `confirm-forget-*`, `empty-pair`.

- [ ] **Step 3: Replace `AccountsModal.tsx`**

Replace the contents of `clients/desktop/ui/src/modals/AccountsModal.tsx`:

```typescript
import { useEffect, useState } from "react";
import { cmd } from "../ipc/commands";
import { events } from "../ipc/events";
import { useAccountsStore } from "../store";
import type { Account } from "../types";

export default function AccountsModal() {
  const accounts = useAccountsStore((s) => s.accounts);
  const hydrate = useAccountsStore((s) => s.hydrate);
  const removeFromStore = useAccountsStore((s) => s.remove);
  const setActiveInStore = useAccountsStore((s) => s.setActive);
  const [confirmingUserId, setConfirmingUserId] = useState<string | undefined>();
  const [error, setError] = useState<string>();

  useEffect(() => {
    let cancelled = false;
    const refresh = async () => {
      try {
        const rows = await cmd.listAccounts();
        if (!cancelled) hydrate(rows);
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    };
    refresh();
    let unsub: Array<() => void> = [];
    (async () => {
      unsub.push(await events.onAccountAdded(refresh));
      unsub.push(
        await events.onAccountRemoved(({ user_id }) => {
          removeFromStore(user_id);
          setConfirmingUserId((curr) => (curr === user_id ? undefined : curr));
        }),
      );
      unsub.push(
        await events.onActiveChanged(({ user_id }) => {
          setActiveInStore(user_id ?? undefined);
        }),
      );
    })();
    return () => {
      cancelled = true;
      unsub.forEach((u) => u());
    };
  }, [hydrate, removeFromStore, setActiveInStore]);

  if (accounts.length === 0) {
    return (
      <div className="flex flex-col gap-3 p-6 text-sm">
        <h1 className="text-base font-semibold">Accounts</h1>
        <div className="text-zinc-300">No accounts. Pair a device to get started.</div>
        <button
          data-testid="empty-pair"
          className="self-start rounded bg-blue-600 px-3 py-1.5 text-white hover:bg-blue-500"
          onClick={() => cmd.openModal("pairing").catch((e) => setError(String(e)))}
        >
          Pair a device
        </button>
        {error && <div className="text-xs text-red-400">{error}</div>}
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-3 p-6 text-sm">
      <h1 className="text-base font-semibold">Accounts</h1>
      <ul className="flex flex-col gap-2">
        {accounts.map((a) => (
          <li key={a.user_id} className="rounded border border-zinc-700">
            <div className="flex items-center justify-between p-3">
              <div>
                <div className="font-semibold">{a.label}</div>
                <div className="text-xs text-zinc-400">
                  {a.user_id} @ {a.server_url}
                </div>
                <div className="text-xs text-zinc-400">
                  status: {a.status} · pending: {a.pending}
                </div>
              </div>
              <div className="flex items-center gap-2">
                {a.is_active ? (
                  <span
                    data-testid={`active-badge-${a.user_id}`}
                    className="rounded bg-emerald-700 px-2 py-1 text-xs uppercase tracking-wide text-white"
                  >
                    Active
                  </span>
                ) : (
                  <button
                    data-testid={`use-${a.user_id}`}
                    className="rounded bg-blue-600 px-2 py-1 text-white hover:bg-blue-500"
                    onClick={async () => {
                      try {
                        await cmd.setActiveAccount({ user_id: a.user_id });
                      } catch (e) {
                        setError(String(e));
                      }
                    }}
                  >
                    Use
                  </button>
                )}
                <button
                  aria-label={`Disconnect ${a.label}`}
                  data-testid={`trash-${a.user_id}`}
                  className="rounded p-1 text-zinc-300 hover:bg-zinc-800 hover:text-red-300"
                  onClick={() => setConfirmingUserId(a.user_id)}
                >
                  <TrashIcon />
                </button>
              </div>
            </div>
            {confirmingUserId === a.user_id && (
              <ConfirmStrip
                account={a}
                onCancel={() => setConfirmingUserId(undefined)}
                onConfirm={async () => {
                  try {
                    await cmd.forgetAccount({ user_id: a.user_id });
                    setConfirmingUserId(undefined);
                  } catch (e) {
                    setError(String(e));
                  }
                }}
              />
            )}
          </li>
        ))}
      </ul>
      {error && <div className="text-xs text-red-400">{error}</div>}
    </div>
  );
}

function ConfirmStrip(props: {
  account: Account;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <div
      data-testid={`confirm-strip-${props.account.user_id}`}
      className="border-t border-zinc-700 bg-zinc-900/40 p-3 flex items-center justify-between gap-3"
    >
      <div className="text-xs text-zinc-300">
        Erase local history and key for {props.account.label}?
      </div>
      <div className="flex items-center gap-2">
        <button
          data-testid={`cancel-${props.account.user_id}`}
          className="rounded border border-zinc-700 px-2 py-1 text-zinc-200 hover:bg-zinc-800"
          onClick={props.onCancel}
        >
          Cancel
        </button>
        <button
          data-testid={`confirm-forget-${props.account.user_id}`}
          className="rounded bg-red-600 px-2 py-1 text-white hover:bg-red-500"
          onClick={props.onConfirm}
        >
          Forget
        </button>
      </div>
    </div>
  );
}

function TrashIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M3 6h18" />
      <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
      <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" />
      <path d="M10 11v6" />
      <path d="M14 11v6" />
    </svg>
  );
}
```

- [ ] **Step 4: Re-run the modal tests**

Run: `cd clients/desktop/ui && npm test -- --run src/__tests__/AccountsModal.test.tsx`
Expected: PASS for all six cases.

- [ ] **Step 5: Run the full UI test suite and typecheck**

Run: `cd clients/desktop/ui && npm test -- --run && npx tsc --noEmit`
Expected: all suites green, no TS errors.

- [ ] **Step 6: Commit**

```bash
git add clients/desktop/ui/src/modals/AccountsModal.tsx clients/desktop/ui/src/__tests__/AccountsModal.test.tsx
git commit -m "feat(desktop-ui): inline disconnect with active badge in accounts modal"
```

---

## Task 11: Popover subscribes to events and shows no-active placeholder

**Files:**
- Modify: `clients/desktop/ui/src/views/Popover.tsx`
- Modify: `clients/desktop/ui/src/__tests__/Popover.test.tsx`

- [ ] **Step 1: Add a failing test for the no-active placeholder**

Append to `clients/desktop/ui/src/__tests__/Popover.test.tsx` inside `describe("Popover", ...)`:

```typescript
it("renders the choose-account placeholder when accounts exist but none is active", async () => {
  const inactiveAccounts: Account[] = accounts.map((a) => ({ ...a, is_active: false, status: "Disconnected" }));
  invoke = vi.fn(async (cmd) => {
    if (cmd === "list_accounts") return inactiveAccounts;
    if (cmd === "list_history") return [];
    if (cmd === "open_modal") return undefined;
    return undefined;
  }) as ReturnType<typeof vi.fn<Invoker>>;
  const listen = vi.fn(async () => () => {}) as ReturnType<typeof vi.fn<Listener>>;
  injectForTests(invoke as never, listen as never);

  const { findByTestId } = render(<Popover />);
  const button = await findByTestId("choose-account");
  fireEvent.click(button);
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("open_modal", { args: { kind: "accounts" } }),
  );
});
```

Add the imports at the top of the file if not already present:

```typescript
import { fireEvent } from "@testing-library/react";
```

- [ ] **Step 2: Run the test and confirm it fails**

Run: `cd clients/desktop/ui && npm test -- --run src/__tests__/Popover.test.tsx`
Expected: FAIL — `choose-account` testid does not exist yet.

- [ ] **Step 3: Update `Popover.tsx`**

Edit `clients/desktop/ui/src/views/Popover.tsx`. Add two extra subscriptions inside the existing `useEffect` and a new placeholder branch.

Replace the `useEffect` that registers listeners (`Popover.tsx:44-71`) with:

```typescript
  useEffect(() => {
    let unsub: Array<() => void> = [];
    let cancelled = false;
    (async () => {
      const accs = await cmd.listAccounts();
      if (cancelled) return;
      hydrateAccounts(accs);
      const activeUserId = useAccountsStore.getState().active;
      if (activeUserId) {
        const rows = await cmd.listHistory({ user_id: activeUserId, limit: 100 });
        if (!cancelled) hydrateHistory(rows);
      }
      unsub.push(await events.onEntryAdded(({ user_id, entry }) => {
        if (user_id === useAccountsStore.getState().active) addEntry(entry);
      }));
      unsub.push(await events.onEntryDeleted(({ user_id, entry_id }) => {
        if (user_id === useAccountsStore.getState().active) removeEntry(entry_id);
      }));
      unsub.push(await events.onConnectionState(({ user_id, state, last_error }) => {
        setStatus(user_id, last_error !== undefined ? { state, last_error } : { state });
      }));
      unsub.push(await events.onPendingCount(({ user_id, count }) => {
        setStatus(user_id, { pending: count });
      }));
      unsub.push(await events.onAccountAdded(() => {
        cmd.listAccounts().then(hydrateAccounts);
      }));
      unsub.push(await events.onAccountRemoved(({ user_id }) => {
        useAccountsStore.getState().remove(user_id);
      }));
      unsub.push(await events.onActiveChanged(({ user_id }) => {
        const next = user_id ?? undefined;
        useAccountsStore.getState().setActive(next);
        if (next) {
          cmd.listHistory({ user_id: next, limit: 100 }).then(hydrateHistory).catch(() => {});
        } else {
          hydrateHistory([]);
        }
      }));
    })();
    return () => {
      cancelled = true;
      unsub.forEach((u) => u());
    };
  }, [addEntry, hydrateAccounts, hydrateHistory, removeEntry, setStatus]);
```

Replace the empty-state branch (`Popover.tsx:81-93`) with this two-branch block, leaving the existing return below it untouched:

```typescript
  if (accounts.length === 0) {
    return (
      <div className="flex h-full flex-col p-4 gap-2 text-sm">
        <div className="font-semibold">No accounts paired yet.</div>
        <button
          className="rounded bg-blue-600 px-3 py-1.5 text-white hover:bg-blue-500"
          onClick={() => setModal("pairing")}
        >
          Pair a device
        </button>
      </div>
    );
  }

  if (!active) {
    return (
      <div className="flex h-full flex-col p-4 gap-2 text-sm">
        <div className="font-semibold">No active account.</div>
        <button
          data-testid="choose-account"
          className="self-start rounded bg-blue-600 px-3 py-1.5 text-white hover:bg-blue-500"
          onClick={() => cmd.openModal("accounts").catch((err) => console.error("open accounts failed", err))}
        >
          Choose account
        </button>
      </div>
    );
  }
```

- [ ] **Step 4: Run the popover tests**

Run: `cd clients/desktop/ui && npm test -- --run src/__tests__/Popover.test.tsx`
Expected: both tests pass.

- [ ] **Step 5: Run the full UI test suite and typecheck**

Run: `cd clients/desktop/ui && npm test -- --run && npx tsc --noEmit`
Expected: all suites green.

- [ ] **Step 6: Commit**

```bash
git add clients/desktop/ui/src/views/Popover.tsx clients/desktop/ui/src/__tests__/Popover.test.tsx
git commit -m "feat(desktop-ui): popover subscribes to active-changed and shows chooser"
```

---

## Task 12: Cross-test sweep

**Files:**
- Run only — no edits expected unless failures surface.

- [ ] **Step 1: Run the full Rust test suite**

Run: `cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml`
Expected: all tests pass.

- [ ] **Step 2: Run the full UI test suite**

Run: `cd clients/desktop/ui && npm test -- --run`
Expected: all tests pass.

- [ ] **Step 3: Run typecheck**

Run: `cd clients/desktop/ui && npx tsc --noEmit`
Expected: clean.

- [ ] **Step 4: If any test fails, fix it and commit**

Use a focused commit per fix. Do not bundle unrelated fixes.

---

## Task 13: Manual smoke test

**Goal:** Confirm the end-to-end flow on a running desktop client.

- [ ] **Step 1: Build and launch the desktop client**

Run from `clients/desktop`: `npm install` (only if not already installed) then `npm run tauri dev` (or the project's documented dev command in `clients/desktop/README.md` — follow that if it differs).

- [ ] **Step 2: Pair two devices**

Use the existing pairing flow to add two accounts. Switch the active account between them via the new "Use" button. Confirm the badge moves and that the popover history reflects the active account.

- [ ] **Step 3: Verify last-active is restored on restart**

Activate one account, quit the desktop client, relaunch it. Confirm the same account becomes active automatically and history loads for it.

- [ ] **Step 4: Forget a non-active account**

Click trash on the inactive row. Confirm the strip appears under that row and outside that row only. Click Forget. The row disappears, the active row is unchanged.

- [ ] **Step 5: Forget the active account**

Click trash on the active row. Click Forget. The other account is auto-promoted; the popover keeps working.

- [ ] **Step 6: Forget the last remaining account**

The Accounts modal shows the empty state with the "Pair a device" button. The popover (or, if the popover is closed, when next opened) shows the "No accounts paired yet" placeholder.

- [ ] **Step 7: Confirm all happy-path expectations hold**

If anything regresses, capture a console / Tauri log snippet and add a follow-up task before merging.

---

## Self-review notes

- Spec coverage: persistence (Task 1), helpers (Task 2), promotion (Task 3), DTO flag (Task 4), `set_active_account` rewrite (Task 5), `forget_account` rewrite (Task 6), startup (Task 7), TS type (Task 8), store (Task 9), modal UX (Task 10), popover placeholder + subscriptions (Task 11), full sweep + smoke (Tasks 12–13).
- No placeholders: every code step shows full code or exact replacement; every command shows expected output category.
- Type/method names are consistent across tasks: `set_active_persisted`, `set_active_persisted_with`, `load_persisted_active`, `is_active`, `confirmingUserId`.
- Order honors TDD: tests precede implementation in Tasks 1, 2, 3, 9, 10, 11. Tasks 4–7 are pure refactors covered by registry-level tests added in Tasks 2–3 and the smoke test in Task 13.
