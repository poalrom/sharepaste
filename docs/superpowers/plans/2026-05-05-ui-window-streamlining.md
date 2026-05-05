# UI Window Streamlining Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Collapse the desktop client to one popover plus one on-demand main window with tab navigation; replace the per-modal `WebviewWindow` machinery with a single `open_main_window(section)` command driving a single `main` window.

**Architecture:** A new `main.html` entrypoint hosts `Main.tsx`, which renders a tab bar (Accounts / Settings / Pairing) above the section content. The Tauri `open_main_window(section)` command creates the window on first call (label `main`) and, on subsequent calls, focuses the existing window and emits a `main://navigate` event so the UI flips tabs. The popover gets a small `openSection(section)` helper that calls `openMainWindow` and `hidePopover` together. The tray right-click menu gains entries for each section. Cleanup removes `open_modal`, `modal.html`, and the unused `index.html`.

**Tech Stack:** Tauri v2 (Rust + WebviewWindow), Vite multi-input + React 18, Zustand store, Vitest + React Testing Library.

**Spec:** `docs/superpowers/specs/2026-05-05-ui-window-streamlining-design.md`

---

## File Structure

**New files**
- `clients/desktop/ui/main.html` — main window HTML entrypoint (`data-route="main"`).
- `clients/desktop/ui/src/views/Main.tsx` — main window shell: tab bar + section content + `main://navigate` listener.
- `clients/desktop/ui/src/views/sections/AccountsSection.tsx` — moved from `modals/AccountsModal.tsx`, modal chrome stripped.
- `clients/desktop/ui/src/views/sections/SettingsSection.tsx` — moved from `modals/SettingsModal.tsx`, modal chrome stripped.
- `clients/desktop/ui/src/views/sections/PairingSection.tsx` — moved from `modals/PairingModal.tsx`, modal chrome stripped, `onClose` removed.
- `clients/desktop/ui/src/__tests__/Main.test.tsx` — Main shell tests.
- `clients/desktop/ui/src/__tests__/openSection.test.ts` — helper tests.

**Modified files**
- `clients/desktop/src-tauri/src/commands.rs` — replace `open_modal` cmd with `open_main_window` cmd. Section-validation unit test.
- `clients/desktop/src-tauri/src/lib.rs` — register new cmd in `invoke_handler`, replace tray menu items, replace `open_modal` helper with `open_main_window_impl`, add macOS activation-policy switching.
- `clients/desktop/src-tauri/src/events.rs` — add `MAIN_NAVIGATE` event name constant.
- `clients/desktop/ui/vite.config.ts` — replace inputs with `{ popover, main }`.
- `clients/desktop/ui/src/App.tsx` — route `main` → `<Main />`.
- `clients/desktop/ui/src/store/ui.ts` — add `mainSection` field + setter; remove `modal`/`setModal`.
- `clients/desktop/ui/src/ipc/commands.ts` — replace `openModal` with `openMainWindow`; add `openSection` helper.
- `clients/desktop/ui/src/ipc/events.ts` — add `onMainNavigate`.
- `clients/desktop/ui/src/views/Popover.tsx` — drop inline pairing branch, rewire entries to `openSection`.
- `clients/desktop/ui/src/views/Footer.tsx` — rewire to `openSection`.
- `clients/desktop/ui/src/__tests__/Popover.test.tsx` — update assertions for new IPC.

**Deleted files**
- `clients/desktop/ui/modal.html`
- `clients/desktop/ui/index.html`
- `clients/desktop/ui/src/modals/AccountsModal.tsx`
- `clients/desktop/ui/src/modals/SettingsModal.tsx`
- `clients/desktop/ui/src/modals/PairingModal.tsx`
- `clients/desktop/ui/src/__tests__/AccountsModal.test.tsx` — replaced by `AccountsSection.test.tsx` (rename, see Task 6)
- `clients/desktop/ui/src/__tests__/SettingsModal.test.tsx` — replaced by `SettingsSection.test.tsx`
- `clients/desktop/ui/src/__tests__/PairingModal.test.tsx` — replaced by `PairingSection.test.tsx`

---

## Task 1: Add `MAIN_NAVIGATE` event constant

**Files:**
- Modify: `clients/desktop/src-tauri/src/events.rs`

- [ ] **Step 1: Read existing event constants**

Run: `grep -n "pub const " clients/desktop/src-tauri/src/events.rs`

You will see lines like `pub const ACTIVE_CHANGED: &str = "active-changed";`. Append a new constant in the same style.

- [ ] **Step 2: Add the constant**

At the bottom of the constants section in `clients/desktop/src-tauri/src/events.rs`, add:

```rust
pub const MAIN_NAVIGATE: &str = "main://navigate";
```

- [ ] **Step 3: Verify compile**

Run: `cd clients/desktop/src-tauri && cargo check`
Expected: clean compile (the constant has no consumers yet, but no warnings either since unused-pub-constants are not a warning).

- [ ] **Step 4: Commit**

```bash
git add clients/desktop/src-tauri/src/events.rs
git commit -m "feat(desktop): add MAIN_NAVIGATE event constant"
```

---

## Task 2: Add `open_main_window` Rust command (alongside `open_modal`)

**Files:**
- Modify: `clients/desktop/src-tauri/src/commands.rs`
- Modify: `clients/desktop/src-tauri/src/lib.rs:52-71` (invoke_handler list)

- [ ] **Step 1: Write the failing unit test for section validation**

In `clients/desktop/src-tauri/src/commands.rs`, append inside the existing `#[cfg(test)] mod tests { ... }` block:

```rust
#[test]
fn open_main_window_args_rejects_unknown_section() {
    let valid_sections = ["accounts", "settings", "pairing"];
    let test_cases = ["", "  ", "history", "Accounts", "ACCOUNTS"];
    for s in test_cases {
        assert!(
            !valid_sections.contains(&s),
            "test fixture must not collide with valid sections: {s}",
        );
    }
}
```

(This test exists only to lock in the section list. The cmd itself is exercised through manual smoke and the UI tests.)

- [ ] **Step 2: Run the test to verify it passes**

Run: `cd clients/desktop/src-tauri && cargo test open_main_window_args_rejects_unknown_section`
Expected: PASS.

- [ ] **Step 3: Add `open_main_window` cmd + supporting types**

In `clients/desktop/src-tauri/src/commands.rs`, just after the existing `OpenModalArgs` / `open_modal` block, add:

```rust
#[derive(Deserialize)]
pub struct OpenMainWindowArgs {
    pub section: String,
}

#[tauri::command]
pub async fn open_main_window(
    app: AppHandle,
    args: OpenMainWindowArgs,
) -> Result<(), AppError> {
    crate::open_main_window_impl(&app, &args.section).map_err(|e| AppError::BadInput(e.to_string()))
}
```

(`open_main_window_impl` is added in the next task; this references it ahead of time and intentionally fails to compile until then.)

- [ ] **Step 4: Stop here — don't compile**

The crate will not compile yet because `open_main_window_impl` doesn't exist. That's intentional. Move on to Task 3 before running `cargo check`. Do not commit yet.

---

## Task 3: Add `open_main_window_impl` helper + register cmd

**Files:**
- Modify: `clients/desktop/src-tauri/src/lib.rs:409-423` (replace `open_modal` helper region — but keep the old `open_modal` helper for now; we add `open_main_window_impl` next to it)
- Modify: `clients/desktop/src-tauri/src/lib.rs:52-71` (invoke_handler)

- [ ] **Step 1: Add `open_main_window_impl`**

In `clients/desktop/src-tauri/src/lib.rs`, just below the existing `open_modal` helper (around line 423), add:

```rust
fn open_main_window_impl(app: &tauri::AppHandle, section: &str) -> tauri::Result<()> {
    let valid = matches!(section, "accounts" | "settings" | "pairing");
    if !valid {
        return Err(tauri::Error::Io(std::io::Error::other(format!(
            "unknown section: {section}"
        ))));
    }
    if let Some(win) = app.get_webview_window("main") {
        win.set_focus()?;
        let _ = app.emit_to("main", crate::events::MAIN_NAVIGATE, section);
        return Ok(());
    }
    let url = format!("main.html?section={section}");
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App(url.into()))
        .title("sharepaste")
        .inner_size(720.0, 560.0)
        .resizable(true)
        .build()?;
    Ok(())
}
```

- [ ] **Step 2: Register `open_main_window` cmd in the invoke handler**

In `clients/desktop/src-tauri/src/lib.rs`, modify the `invoke_handler` block. After the line `commands::open_modal,` add:

```rust
            commands::open_main_window,
```

The block now reads:

```rust
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
            commands::open_modal,
            commands::open_main_window,
            commands::hide_popover,
        ])
```

- [ ] **Step 3: Verify compile**

Run: `cd clients/desktop/src-tauri && cargo check`
Expected: clean compile.

- [ ] **Step 4: Run the section-validation test**

Run: `cd clients/desktop/src-tauri && cargo test open_main_window`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add clients/desktop/src-tauri/src/commands.rs clients/desktop/src-tauri/src/lib.rs
git commit -m "feat(desktop): add open_main_window command and impl"
```

---

## Task 4: Update Vite multi-input and add `main.html`

**Files:**
- Create: `clients/desktop/ui/main.html`
- Modify: `clients/desktop/ui/vite.config.ts`

Note: `index.html` and `modal.html` stay in the input map for now — they are deleted in the cleanup task. We add `main` alongside them so the build keeps working through the migration.

- [ ] **Step 1: Create `main.html`**

Create `clients/desktop/ui/main.html` with:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>sharepaste</title>
  </head>
  <body data-route="main">
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 2: Update Vite input**

Modify `clients/desktop/ui/vite.config.ts`. Replace the `rollupOptions.input` block:

```ts
    rollupOptions: {
      input: {
        popover: "popover.html",
        modal: "modal.html",
        index: "index.html",
        main: "main.html",
      },
    },
```

- [ ] **Step 3: Run the UI build to verify**

Run: `cd clients/desktop/ui && npm run build`
Expected: build succeeds. `clients/desktop/ui/dist/main.html` is produced.

- [ ] **Step 4: Commit**

```bash
git add clients/desktop/ui/main.html clients/desktop/ui/vite.config.ts
git commit -m "feat(desktop-ui): add main.html entrypoint"
```

---

## Task 5: Add `mainSection` to UI store, drop `modal` field

**Files:**
- Modify: `clients/desktop/ui/src/store/ui.ts`
- Modify: `clients/desktop/ui/src/__tests__/store.test.ts`
- Modify: `clients/desktop/ui/src/__tests__/Popover.test.tsx`

- [ ] **Step 1: Write failing test for `mainSection`**

In `clients/desktop/ui/src/__tests__/store.test.ts`, append:

```ts
describe("useUiStore mainSection", () => {
  it("defaults to 'accounts'", () => {
    expect(useUiStore.getState().mainSection).toBe("accounts");
  });

  it("setMainSection updates the field", () => {
    useUiStore.getState().setMainSection("pairing");
    expect(useUiStore.getState().mainSection).toBe("pairing");
  });
});
```

(If `useUiStore` is not already imported in this file, add `import { useUiStore } from "../store/ui";`.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cd clients/desktop/ui && npx vitest run src/__tests__/store.test.ts`
Expected: FAIL — `mainSection`/`setMainSection` undefined.

- [ ] **Step 3: Update `store/ui.ts`**

Replace the contents of `clients/desktop/ui/src/store/ui.ts` with:

```ts
import { create } from "zustand";

export type MainSection = "accounts" | "settings" | "pairing";

export type UiState = {
  search: string;
  selectedIndex: number;
  mainSection: MainSection;
  setSearch: (s: string) => void;
  setSelectedIndex: (i: number) => void;
  setMainSection: (m: MainSection) => void;
};

export const useUiStore = create<UiState>((set) => ({
  search: "",
  selectedIndex: 0,
  mainSection: "accounts",
  setSearch: (search) => set({ search, selectedIndex: 0 }),
  setSelectedIndex: (selectedIndex) => set({ selectedIndex }),
  setMainSection: (mainSection) => set({ mainSection }),
}));
```

- [ ] **Step 4: Update `Popover.test.tsx` setState init**

In `clients/desktop/ui/src/__tests__/Popover.test.tsx`, change:

```tsx
useUiStore.setState({ modal: null, search: "", selectedIndex: 0 });
```

to:

```tsx
useUiStore.setState({ search: "", selectedIndex: 0, mainSection: "accounts" });
```

- [ ] **Step 5: Run all UI tests; expect compile errors elsewhere**

Run: `cd clients/desktop/ui && npm test`
Expected: store test passes; **TypeScript errors will appear** in `Popover.tsx`, `Popover.test.tsx`, `PairingModal.tsx`, etc., that reference `modal`/`setModal`. That's expected — those references are removed in later tasks. **Do not commit yet.** Move directly to the next task.

---

## Task 6: Move section components into `views/sections/` (placeholder modal references)

We move and rename the three modal components but keep them temporarily inert — they still reference `modal`/`setModal` and `cmd.openModal` indirectly. We strip those in subsequent tasks. This task lets the test suite stop complaining about missing files when later tests import the new paths.

**Files:**
- Move: `clients/desktop/ui/src/modals/AccountsModal.tsx` → `clients/desktop/ui/src/views/sections/AccountsSection.tsx`
- Move: `clients/desktop/ui/src/modals/SettingsModal.tsx` → `clients/desktop/ui/src/views/sections/SettingsSection.tsx`
- Move: `clients/desktop/ui/src/modals/PairingModal.tsx` → `clients/desktop/ui/src/views/sections/PairingSection.tsx`
- Rename: `__tests__/AccountsModal.test.tsx` → `__tests__/AccountsSection.test.tsx`
- Rename: `__tests__/SettingsModal.test.tsx` → `__tests__/SettingsSection.test.tsx`
- Rename: `__tests__/PairingModal.test.tsx` → `__tests__/PairingSection.test.tsx`

- [ ] **Step 1: Make the directory**

Run: `mkdir -p clients/desktop/ui/src/views/sections`

- [ ] **Step 2: Move files using git**

Run:

```bash
git mv clients/desktop/ui/src/modals/AccountsModal.tsx clients/desktop/ui/src/views/sections/AccountsSection.tsx
git mv clients/desktop/ui/src/modals/SettingsModal.tsx clients/desktop/ui/src/views/sections/SettingsSection.tsx
git mv clients/desktop/ui/src/modals/PairingModal.tsx clients/desktop/ui/src/views/sections/PairingSection.tsx
git mv clients/desktop/ui/src/__tests__/AccountsModal.test.tsx clients/desktop/ui/src/__tests__/AccountsSection.test.tsx
git mv clients/desktop/ui/src/__tests__/SettingsModal.test.tsx clients/desktop/ui/src/__tests__/SettingsSection.test.tsx
git mv clients/desktop/ui/src/__tests__/PairingModal.test.tsx clients/desktop/ui/src/__tests__/PairingSection.test.tsx
rmdir clients/desktop/ui/src/modals
```

- [ ] **Step 3: Rename default exports + update test imports**

In each section file, rename the default-exported component:
- `AccountsSection.tsx`: `export default function AccountsModal()` → `export default function AccountsSection()`
- `SettingsSection.tsx`: `export default function SettingsModal()` → `export default function SettingsSection()`
- `PairingSection.tsx`: `export default function PairingModal({ onClose }: { onClose?: () => void } = {})` → keep the prop signature for now; we strip `onClose` in Task 8.

In each test file, update the import path and the imported name:
- `AccountsSection.test.tsx`: `import AccountsModal from "../modals/AccountsModal";` → `import AccountsSection from "../views/sections/AccountsSection";` and replace `<AccountsModal />` with `<AccountsSection />` everywhere.
- Same pattern for `SettingsSection.test.tsx` and `PairingSection.test.tsx`.

- [ ] **Step 4: Update `App.tsx` imports**

In `clients/desktop/ui/src/App.tsx`, replace:

```tsx
import PairingModal from "./modals/PairingModal";
import SettingsModal from "./modals/SettingsModal";
import AccountsModal from "./modals/AccountsModal";
```

with:

```tsx
import PairingSection from "./views/sections/PairingSection";
import SettingsSection from "./views/sections/SettingsSection";
import AccountsSection from "./views/sections/AccountsSection";
```

And update the JSX inside `App.tsx`:

```tsx
if (kind === "pairing")  return <PairingSection />;
if (kind === "settings") return <SettingsSection />;
if (kind === "accounts") return <AccountsSection />;
```

- [ ] **Step 5: Update `Popover.tsx` import**

In `clients/desktop/ui/src/views/Popover.tsx`, change:

```tsx
import PairingModal from "../modals/PairingModal";
```

to:

```tsx
import PairingSection from "../views/sections/PairingSection";
```

(The inline pairing branch still exists; we drop it in Task 9. For now, replace the JSX `<PairingModal …/>` with `<PairingSection …/>` so the file compiles.)

- [ ] **Step 6: Run typecheck + tests**

Run: `cd clients/desktop/ui && npx tsc --noEmit`
Expected: still errors about `useUiStore.modal` from Task 5 — that's fine.

Run: `cd clients/desktop/ui && npm test -- --run AccountsSection.test.tsx SettingsSection.test.tsx PairingSection.test.tsx`
Expected: section tests pass (they only renamed imports).

- [ ] **Step 7: Commit**

```bash
git add -A clients/desktop/ui/src
git commit -m "refactor(desktop-ui): move modal components to views/sections"
```

---

## Task 7: Add `openMainWindow` IPC + `openSection` helper, drop `openModal`

**Files:**
- Modify: `clients/desktop/ui/src/ipc/commands.ts`
- Modify: `clients/desktop/ui/src/ipc/events.ts`
- Create: `clients/desktop/ui/src/__tests__/openSection.test.ts`

- [ ] **Step 1: Write failing test for `openSection`**

Create `clients/desktop/ui/src/__tests__/openSection.test.ts`:

```ts
import { describe, expect, it, vi } from "vitest";
import { injectForTests, type Invoker, type Listener } from "../ipc/tauri";
import { cmd } from "../ipc/commands";

describe("cmd.openSection", () => {
  it("invokes open_main_window then hide_popover", async () => {
    const calls: Array<{ cmd: string; args: unknown }> = [];
    const invoke: Invoker = async (cmd, args) => {
      calls.push({ cmd, args: args ?? null });
      return undefined as never;
    };
    const listen: Listener = async () => () => {};
    injectForTests(invoke, listen);

    await cmd.openSection("pairing");

    expect(calls).toEqual([
      { cmd: "open_main_window", args: { args: { section: "pairing" } } },
      { cmd: "hide_popover", args: null },
    ]);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd clients/desktop/ui && npx vitest run src/__tests__/openSection.test.ts`
Expected: FAIL — `cmd.openSection` is not a function.

- [ ] **Step 3: Update `ipc/commands.ts`**

Edit `clients/desktop/ui/src/ipc/commands.ts`. Replace the `openModal` line and append `openMainWindow` + `openSection`:

```ts
  openMainWindow:      (args: { section: "accounts" | "settings" | "pairing" }) =>
                         tauri.invoke<void>("open_main_window", { args }),
  hidePopover:         () => tauri.invoke<void>("hide_popover"),
  openSection:         async (section: "accounts" | "settings" | "pairing") => {
                         await tauri.invoke<void>("open_main_window", { args: { section } });
                         await tauri.invoke<void>("hide_popover");
                       },
```

Remove the old `openModal` line entirely. The full updated `cmd` object now ends with `openMainWindow`, `hidePopover`, `openSection`.

- [ ] **Step 4: Add `onMainNavigate` to events**

Edit `clients/desktop/ui/src/ipc/events.ts`. Append (before the closing `};`):

```ts
  onMainNavigate:    (cb: (section: "accounts" | "settings" | "pairing") => void) => tauri.listen("main://navigate", cb),
```

- [ ] **Step 5: Run the new test**

Run: `cd clients/desktop/ui && npx vitest run src/__tests__/openSection.test.ts`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add clients/desktop/ui/src/ipc/commands.ts clients/desktop/ui/src/ipc/events.ts clients/desktop/ui/src/__tests__/openSection.test.ts
git commit -m "feat(desktop-ui): add openMainWindow IPC and openSection helper"
```

---

## Task 8: Strip modal chrome from section components

**Files:**
- Modify: `clients/desktop/ui/src/views/sections/PairingSection.tsx`
- Modify: `clients/desktop/ui/src/views/sections/AccountsSection.tsx`
- Modify: `clients/desktop/ui/src/__tests__/PairingSection.test.tsx`

- [ ] **Step 1: PairingSection — drop `onClose`, drop `window.close()` fallback**

Edit `clients/desktop/ui/src/views/sections/PairingSection.tsx`. Change the component signature and the `close` handler.

Before:

```tsx
export default function PairingSection({ onClose }: { onClose?: () => void } = {}) {
  const close = onClose ?? (() => window.close());
```

After:

```tsx
export default function PairingSection() {
  const setMainSection = useUiStore((s) => s.setMainSection);
  const close = () => setMainSection("accounts");
```

Add this import near the top:

```tsx
import { useUiStore } from "../../store";
```

Inside `useEffect` for the `close` dep array, leave `[close]` removed and replace with `[setMainSection]`:

```tsx
  }, [setMainSection]);
```

(`close` is recreated each render, but Effect only needs to re-subscribe when `setMainSection` identity changes, which is never in zustand.)

- [ ] **Step 2: AccountsSection — replace empty-state IPC**

Edit `clients/desktop/ui/src/views/sections/AccountsSection.tsx`. Find:

```tsx
onClick={() => cmd.openModal("pairing").catch((e) => setError(String(e)))}
```

Replace with:

```tsx
onClick={() => useUiStore.getState().setMainSection("pairing")}
```

Add this import near the top if not already present:

```tsx
import { useUiStore } from "../../store";
```

Remove the `setError` call from that path (the section just switches tab; no IPC error to surface). Drop the unused `cmd` import only if no other call site remains in this file (re-check; it is still used by `setActiveAccount` and `forgetAccount`, so keep the import).

- [ ] **Step 3: Update PairingSection test to drop `onClose`**

Edit `clients/desktop/ui/src/__tests__/PairingSection.test.tsx`. Find any usage like `<PairingSection onClose={...} />` and replace with `<PairingSection />`. Remove the corresponding `onClose` mocks. If a test asserted on `onClose` being called, replace the assertion with: after the closing trigger, expect `useUiStore.getState().mainSection` to be `"accounts"`. Reset `useUiStore.setState({ mainSection: "pairing" })` in `beforeEach` for that test.

(If `useUiStore` is not imported in this test file, add `import { useUiStore } from "../store/ui";`.)

- [ ] **Step 4: Run section tests**

Run: `cd clients/desktop/ui && npx vitest run src/__tests__/PairingSection.test.tsx src/__tests__/AccountsSection.test.tsx src/__tests__/SettingsSection.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add clients/desktop/ui/src/views/sections clients/desktop/ui/src/__tests__/PairingSection.test.tsx clients/desktop/ui/src/__tests__/AccountsSection.test.tsx
git commit -m "refactor(desktop-ui): strip modal chrome from sections"
```

---

## Task 9: Add `Main.tsx` shell with tabs and navigate listener

**Files:**
- Create: `clients/desktop/ui/src/views/Main.tsx`
- Create: `clients/desktop/ui/src/__tests__/Main.test.tsx`

- [ ] **Step 1: Write failing tests**

Create `clients/desktop/ui/src/__tests__/Main.test.tsx`:

```tsx
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { injectForTests, type Invoker, type Listener } from "../ipc/tauri";
import { useUiStore } from "../store/ui";
import Main from "../views/Main";

let invoke: ReturnType<typeof vi.fn<Invoker>>;
let navigateCb: ((section: string) => void) | undefined;

beforeEach(() => {
  invoke = vi.fn(async () => undefined as never) as ReturnType<typeof vi.fn<Invoker>>;
  const listen: Listener = async <P,>(event: string, cb: (payload: P) => void) => {
    if (event === "main://navigate") navigateCb = cb as (s: string) => void;
    return () => {};
  };
  injectForTests(invoke as never, listen);
  useUiStore.setState({ search: "", selectedIndex: 0, mainSection: "accounts" });
  navigateCb = undefined;
});

afterEach(() => {
  window.history.replaceState({}, "", "/");
});

describe("Main shell", () => {
  it("uses ?section= from URL on mount", () => {
    window.history.replaceState({}, "", "/main.html?section=settings");
    render(<Main />);
    expect(useUiStore.getState().mainSection).toBe("settings");
    expect(screen.getByTestId("tab-settings")).toHaveAttribute("aria-selected", "true");
  });

  it("falls back to 'accounts' when ?section= is missing or unknown", () => {
    window.history.replaceState({}, "", "/main.html");
    render(<Main />);
    expect(useUiStore.getState().mainSection).toBe("accounts");
  });

  it("clicking a tab updates the active section", () => {
    render(<Main />);
    fireEvent.click(screen.getByTestId("tab-pairing"));
    expect(useUiStore.getState().mainSection).toBe("pairing");
  });

  it("main://navigate event flips the active section", async () => {
    render(<Main />);
    // wait one microtask for the listener registration
    await Promise.resolve();
    expect(navigateCb).toBeDefined();
    navigateCb!("settings");
    expect(useUiStore.getState().mainSection).toBe("settings");
  });
});
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cd clients/desktop/ui && npx vitest run src/__tests__/Main.test.tsx`
Expected: FAIL — `Main` not found.

- [ ] **Step 3: Implement `Main.tsx`**

Create `clients/desktop/ui/src/views/Main.tsx`:

```tsx
import { useEffect } from "react";
import { useUiStore, type MainSection } from "../store/ui";
import { events } from "../ipc/events";
import AccountsSection from "./sections/AccountsSection";
import SettingsSection from "./sections/SettingsSection";
import PairingSection from "./sections/PairingSection";

const SECTIONS: MainSection[] = ["accounts", "settings", "pairing"];
const LABELS: Record<MainSection, string> = {
  accounts: "Accounts",
  settings: "Settings",
  pairing: "Pairing",
};

export default function Main() {
  const active = useUiStore((s) => s.mainSection);
  const setActive = useUiStore((s) => s.setMainSection);

  useEffect(() => {
    const fromUrl = new URLSearchParams(window.location.search).get("section");
    if (fromUrl && (SECTIONS as string[]).includes(fromUrl)) {
      setActive(fromUrl as MainSection);
    }
  }, [setActive]);

  useEffect(() => {
    let unsub: (() => void) | undefined;
    let cancelled = false;
    (async () => {
      const off = await events.onMainNavigate((section) => {
        if ((SECTIONS as string[]).includes(section)) setActive(section as MainSection);
      });
      if (cancelled) off();
      else unsub = off;
    })();
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, [setActive]);

  return (
    <div className="flex h-full flex-col">
      <nav role="tablist" className="flex border-b border-zinc-700">
        {SECTIONS.map((s) => (
          <button
            key={s}
            data-testid={`tab-${s}`}
            role="tab"
            aria-selected={active === s}
            className={
              "px-4 py-2 text-sm " +
              (active === s
                ? "border-b-2 border-blue-500 text-blue-300"
                : "text-zinc-300 hover:text-zinc-100")
            }
            onClick={() => setActive(s)}
          >
            {LABELS[s]}
          </button>
        ))}
      </nav>
      <div className="flex-1 overflow-auto">
        {active === "accounts" && <AccountsSection />}
        {active === "settings" && <SettingsSection />}
        {active === "pairing" && <PairingSection />}
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run Main tests**

Run: `cd clients/desktop/ui && npx vitest run src/__tests__/Main.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add clients/desktop/ui/src/views/Main.tsx clients/desktop/ui/src/__tests__/Main.test.tsx
git commit -m "feat(desktop-ui): add Main shell with tabs and navigate listener"
```

---

## Task 10: Route `main` in `App.tsx`, drop modal route

**Files:**
- Modify: `clients/desktop/ui/src/App.tsx`

- [ ] **Step 1: Replace App.tsx**

Replace `clients/desktop/ui/src/App.tsx` contents with:

```tsx
import Popover from "./views/Popover";
import Main from "./views/Main";

export default function App() {
  const route = document.body.dataset.route ?? "popover";
  if (route === "main") return <Main />;
  return <Popover />;
}
```

(Also removes the `kind` URL parsing and the per-modal renders, which are no longer used.)

- [ ] **Step 2: Run typecheck**

Run: `cd clients/desktop/ui && npx tsc --noEmit`
Expected: clean (any remaining errors must be addressed before commit).

- [ ] **Step 3: Commit**

```bash
git add clients/desktop/ui/src/App.tsx
git commit -m "feat(desktop-ui): route main entrypoint to Main view"
```

---

## Task 11: Rewire `Popover.tsx` to `openSection`, drop inline pairing branch

**Files:**
- Modify: `clients/desktop/ui/src/views/Popover.tsx`
- Modify: `clients/desktop/ui/src/__tests__/Popover.test.tsx`

- [ ] **Step 1: Update Popover test to assert new IPC**

Edit `clients/desktop/ui/src/__tests__/Popover.test.tsx`. Replace the `"choose-account"` test body with:

```tsx
  it("renders the choose-account placeholder when accounts exist but none is active", async () => {
    const inactiveAccounts: Account[] = accounts.map((a) => ({ ...a, is_active: false, status: "Disconnected" }));
    invoke = vi.fn(async (cmd) => {
      if (cmd === "list_accounts") return inactiveAccounts;
      if (cmd === "list_history") return [];
      return undefined;
    }) as ReturnType<typeof vi.fn<Invoker>>;
    const listen = vi.fn(async () => () => {}) as ReturnType<typeof vi.fn<Listener>>;
    injectForTests(invoke as never, listen as never);

    const { findByTestId } = render(<Popover />);
    const button = await findByTestId("choose-account");
    fireEvent.click(button);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("open_main_window", {
        args: { section: "accounts" },
      });
      expect(invoke).toHaveBeenCalledWith("hide_popover", undefined);
    });
  });
```

- [ ] **Step 2: Add a test for empty-state "Pair a device"**

Append inside the `describe("Popover", () => { … })` block:

```tsx
  it("empty-state Pair button opens main window on pairing section", async () => {
    invoke = vi.fn(async (cmd) => {
      if (cmd === "list_accounts") return [];
      return undefined;
    }) as ReturnType<typeof vi.fn<Invoker>>;
    const listen = vi.fn(async () => () => {}) as ReturnType<typeof vi.fn<Listener>>;
    injectForTests(invoke as never, listen as never);

    const { findByText } = render(<Popover />);
    const button = await findByText("Pair a device");
    fireEvent.click(button);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("open_main_window", {
        args: { section: "pairing" },
      });
      expect(invoke).toHaveBeenCalledWith("hide_popover", undefined);
    });
  });
```

- [ ] **Step 3: Run Popover tests; expect failure**

Run: `cd clients/desktop/ui && npx vitest run src/__tests__/Popover.test.tsx`
Expected: FAIL — Popover still calls `open_modal` and the inline pairing branch.

- [ ] **Step 4: Update `Popover.tsx`**

Edit `clients/desktop/ui/src/views/Popover.tsx`. Apply these changes:

1. Remove the `PairingSection` import (the popover never renders it inline anymore).
2. Remove `modal` and `setModal` derivations from `useUiStore`. Replace the existing block:

   ```tsx
   const modal = useUiStore((s) => s.modal);
   const setModal = useUiStore((s) => s.setModal);
   ```

   with nothing (delete those two lines).

3. Simplify the Esc key handler:

   ```tsx
   useEffect(() => {
     const handler = (e: KeyboardEvent) => {
       if (e.key !== "Escape") return;
       e.preventDefault();
       cmd.hidePopover().catch((err) => console.error("hide failed", err));
     };
     window.addEventListener("keydown", handler);
     return () => window.removeEventListener("keydown", handler);
   }, []);
   ```

4. Delete the `if (modal === "pairing") { … }` block entirely.

5. Empty-state "Pair a device" button — replace the click handler:

   Before:
   ```tsx
   onClick={() => setModal("pairing")}
   ```

   After:
   ```tsx
   onClick={() => cmd.openSection("pairing").catch((err) => console.error("open pairing failed", err))}
   ```

6. "Choose account" button — replace:

   Before:
   ```tsx
   onClick={() => cmd.openModal("accounts").catch((err) => console.error("open accounts failed", err))}
   ```

   After:
   ```tsx
   onClick={() => cmd.openSection("accounts").catch((err) => console.error("open accounts failed", err))}
   ```

- [ ] **Step 5: Run Popover tests**

Run: `cd clients/desktop/ui && npx vitest run src/__tests__/Popover.test.tsx`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add clients/desktop/ui/src/views/Popover.tsx clients/desktop/ui/src/__tests__/Popover.test.tsx
git commit -m "feat(desktop-ui): popover entries open main window via openSection"
```

---

## Task 12: Rewire `Footer.tsx` to `openSection`

**Files:**
- Modify: `clients/desktop/ui/src/views/Footer.tsx`

- [ ] **Step 1: Update Footer.tsx**

Edit `clients/desktop/ui/src/views/Footer.tsx`. Replace the two button click handlers:

Before:

```tsx
<button onClick={() => cmd.openModal("accounts")} className="hover:underline">Accounts</button>
<button onClick={() => cmd.openModal("settings")} className="hover:underline">Settings</button>
```

After:

```tsx
<button onClick={() => cmd.openSection("accounts").catch(() => {})} className="hover:underline">Accounts</button>
<button onClick={() => cmd.openSection("settings").catch(() => {})} className="hover:underline">Settings</button>
```

- [ ] **Step 2: Typecheck + run all UI tests**

Run: `cd clients/desktop/ui && npx tsc --noEmit && npm test`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add clients/desktop/ui/src/views/Footer.tsx
git commit -m "feat(desktop-ui): footer routes through openSection"
```

---

## Task 13: Rewire tray menu to `open_main_window_impl`

**Files:**
- Modify: `clients/desktop/src-tauri/src/lib.rs:84-110`

- [ ] **Step 1: Replace tray menu items**

In `clients/desktop/src-tauri/src/lib.rs`, replace this block:

```rust
    let menu = MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id("pair", "Pair device…").build(app)?)
        .item(&MenuItemBuilder::with_id("settings", "Settings…").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("quit", "Quit").build(app)?)
        .build()?;
```

with:

```rust
    let menu = MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id("open_accounts", "Accounts…").build(app)?)
        .item(&MenuItemBuilder::with_id("open_pairing", "Pair device…").build(app)?)
        .item(&MenuItemBuilder::with_id("open_settings", "Settings…").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("quit", "Quit").build(app)?)
        .build()?;
```

- [ ] **Step 2: Replace the menu event handler**

Replace:

```rust
    tray.on_menu_event(|app, event| match event.id.as_ref() {
        "pair" => {
            let _ = open_modal(app, "pairing");
        }
        "settings" => {
            let _ = open_modal(app, "settings");
        }
        "quit" => {
            app.exit(0);
        }
        _ => {}
    });
```

with:

```rust
    tray.on_menu_event(|app, event| match event.id.as_ref() {
        "open_accounts" => {
            let _ = open_main_window_impl(app, "accounts");
        }
        "open_pairing" => {
            let _ = open_main_window_impl(app, "pairing");
        }
        "open_settings" => {
            let _ = open_main_window_impl(app, "settings");
        }
        "quit" => {
            app.exit(0);
        }
        _ => {}
    });
```

- [ ] **Step 3: Verify compile**

Run: `cd clients/desktop/src-tauri && cargo check`
Expected: clean compile.

- [ ] **Step 4: Commit**

```bash
git add clients/desktop/src-tauri/src/lib.rs
git commit -m "feat(desktop): tray menu opens main window per section"
```

---

## Task 14: macOS dock activation policy

When the main window opens, the app should appear in the dock so users can `Cmd-Tab` to it. When closed, revert to the tray-only state.

**Files:**
- Modify: `clients/desktop/src-tauri/src/lib.rs` — `open_main_window_impl` and a window-event handler

- [ ] **Step 1: Set policy when creating the window**

In `clients/desktop/src-tauri/src/lib.rs`, modify `open_main_window_impl`. Replace the existing creation-and-return:

```rust
    let url = format!("main.html?section={section}");
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App(url.into()))
        .title("sharepaste")
        .inner_size(720.0, 560.0)
        .resizable(true)
        .build()?;
    Ok(())
```

with:

```rust
    let url = format!("main.html?section={section}");
    let win = WebviewWindowBuilder::new(app, "main", WebviewUrl::App(url.into()))
        .title("sharepaste")
        .inner_size(720.0, 560.0)
        .resizable(true)
        .build()?;

    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    }

    let app_handle = app.clone();
    win.on_window_event(move |ev| {
        if let WindowEvent::Destroyed = ev {
            #[cfg(target_os = "macos")]
            {
                let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }
            // Touch app_handle on non-macOS to silence unused warning.
            let _ = &app_handle;
        }
    });
    Ok(())
```

- [ ] **Step 2: Verify compile on macOS**

Run: `cd clients/desktop/src-tauri && cargo check --target aarch64-apple-darwin || cargo check`
Expected: clean compile.

- [ ] **Step 3: Manual smoke**

Run: `cd clients/desktop && npm --prefix ui run dev` in one terminal, then `cd clients/desktop/src-tauri && cargo tauri dev` in another (or use the project's existing dev workflow).

Verify:
1. App starts; no dock icon.
2. Right-click tray → Settings → main window opens, dock icon appears.
3. Close main window → dock icon disappears.
4. Re-open from popover (Footer Accounts) → dock icon reappears.

- [ ] **Step 4: Commit**

```bash
git add clients/desktop/src-tauri/src/lib.rs
git commit -m "feat(desktop): toggle macOS activation policy with main window"
```

---

## Task 15: Cleanup — delete `open_modal`, `modal.html`, `index.html`

**Files:**
- Modify: `clients/desktop/src-tauri/src/commands.rs` (remove `OpenModalArgs`, `open_modal` cmd)
- Modify: `clients/desktop/src-tauri/src/lib.rs` (remove `open_modal` helper, remove `commands::open_modal` from invoke handler)
- Delete: `clients/desktop/ui/modal.html`, `clients/desktop/ui/index.html`
- Modify: `clients/desktop/ui/vite.config.ts`

- [ ] **Step 1: Remove the Rust `open_modal` cmd**

In `clients/desktop/src-tauri/src/commands.rs`, delete the `OpenModalArgs` struct and the `pub async fn open_modal` block (the whole `#[tauri::command]` function).

- [ ] **Step 2: Remove the Rust `open_modal` helper + handler entry**

In `clients/desktop/src-tauri/src/lib.rs`:
1. Delete the helper:

   ```rust
   fn open_modal(app: &tauri::AppHandle, kind: &str) -> tauri::Result<()> { … }
   ```

2. In `invoke_handler`, remove `commands::open_modal,` line.

- [ ] **Step 3: Remove obsolete HTML and Vite inputs**

```bash
git rm clients/desktop/ui/modal.html clients/desktop/ui/index.html
```

In `clients/desktop/ui/vite.config.ts`, change the inputs to:

```ts
    rollupOptions: {
      input: {
        popover: "popover.html",
        main: "main.html",
      },
    },
```

- [ ] **Step 4: Verify everything builds**

Run: `cd clients/desktop/src-tauri && cargo check`
Expected: clean.

Run: `cd clients/desktop/ui && npm run build`
Expected: clean. `dist/main.html` and `dist/popover.html` produced; no `modal.html` or `index.html`.

Run: `cd clients/desktop/ui && npm test`
Expected: all tests PASS.

- [ ] **Step 5: Commit**

```bash
git add -A clients/desktop
git commit -m "chore(desktop): remove open_modal and obsolete html entrypoints"
```

---

## Task 16: Final manual smoke

Cover all the end-to-end paths.

- [ ] **Step 1: Build and launch**

Run: `cd clients/desktop && npm --prefix ui run build && cargo --manifest-path src-tauri/Cargo.toml run`
(Or use the existing dev command if there is one — `cargo tauri dev`.)

- [ ] **Step 2: Smoke checklist**

Verify each, in order:
1. App starts; only tray icon visible.
2. Left-click tray → popover appears.
3. Empty-state "Pair a device" (if no accounts) → main window opens on Pairing tab; popover hides.
4. Pair an account, close main window.
5. Left-click tray, Footer "Accounts" → main window opens on Accounts tab; popover hides.
6. While main window open, right-click tray → "Settings…" → tab flips to Settings; no second window.
7. Close main, right-click tray → "Pair device…" → fresh window on Pairing tab.
8. macOS: dock icon visible only while main window is open.

- [ ] **Step 3: No commit needed**

This task is verification only. If any step fails, file a follow-up task; do not commit broken behavior.

---

## Self-review checklist (executed during plan write — informational)

- Spec coverage: every spec section maps to a task — Window topology (Tasks 4, 9), Tauri command surface (Tasks 1–3, 13), UI structure (Tasks 4, 6, 9, 10), Popover changes (Tasks 7, 11), Tray menu (Task 13), Edge cases (Tasks 9, 14), Testing (Tasks 5, 7, 8, 9, 11), Migration order (overall task numbering).
- No placeholders: every code-changing step contains the actual code.
- Type consistency: `MainSection`, `mainSection`, `setMainSection`, `openMainWindow`, `openSection`, `open_main_window_impl`, `MAIN_NAVIGATE` are referenced consistently across tasks.
