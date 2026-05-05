# UI Window Streamlining — Design

Date: 2026-05-05
Status: Draft

## Goal

Collapse the desktop client's window topology to **one popover + one main window**. Today the app spawns a separate `WebviewWindow` per modal kind (`pairing`, `settings`, `accounts`), so the user can end up with multiple top-level windows. After this change, the only popover is the tray popover; everything else lives inside a single, on-demand main window with tab navigation.

## Non-goals

- No redesign of the popover itself (look, position, blur behavior).
- No redesign of pairing/accounts/settings flows beyond stripping their modal chrome and re-hosting them as tabs.
- No new sections beyond the three that exist today.
- No router library introduction.

## Current state (summary)

- `clients/desktop/ui/popover.html` (`data-route="popover"`) — popover entrypoint, built at app start.
- `clients/desktop/ui/modal.html` (`data-route="modal"`) — used by `open_modal` Tauri cmd to spawn one window per `kind`. Each window gets a unique label `modal-<kind>`.
- `clients/desktop/ui/index.html` — duplicate of `popover.html`, unused.
- `App.tsx` switches on `data-route`/`?kind=` to render `Popover`, `PairingModal`, `SettingsModal`, or `AccountsModal`.
- `Popover.tsx` also has an inline pairing branch via `useUiStore.modal === "pairing"`.

## Target state

### Window topology

- **Popover** (label `popover`, `popover.html`) — built at app start, toggled by tray left-click, hides on blur. **Unchanged**.
- **Main** (label `main`, `main.html`) — created on demand, destroyed on close. Single instance: re-opening focuses the existing window and switches the active tab via an event. Default size `720×560`, decorations on, resizable.

### Entry points to the main window

| Trigger | Section opened |
|---|---|
| Tray right-click → "Accounts" | `accounts` |
| Tray right-click → "Pair…" | `pairing` |
| Tray right-click → "Settings" | `settings` |
| Popover empty-state "Pair a device" | `pairing` |
| Popover "Choose account" (no active account) | `accounts` |
| Popover footer "Settings" / "Accounts" | matching section |

Every entry specifies a section explicitly. There is no "open main window with default section" path.

### Popover behavior on main-window open

The popover hides **explicitly** when it triggers a main-window open (does not rely on focus blur alone). UI helper `openSection(section)` calls `cmd.openMainWindow({ section })` and `cmd.hidePopover()` together so the call sites cannot forget.

The blur handler keeps its existing role (resetting search and selected index) but is no longer the mechanism that hides the popover when the main window appears.

## Tauri command surface

Replace `open_modal(kind)` with `open_main_window(section)`.

```rust
#[derive(Deserialize)]
pub struct OpenMainWindowArgs {
    pub section: String, // "accounts" | "settings" | "pairing"
}

#[tauri::command]
pub async fn open_main_window(app: AppHandle, args: OpenMainWindowArgs) -> Result<(), AppError> {
    let valid = matches!(args.section.as_str(), "accounts" | "settings" | "pairing");
    if !valid {
        return Err(AppError::BadInput(format!("unknown section: {}", args.section)));
    }
    if let Some(win) = app.get_webview_window("main") {
        win.set_focus().map_err(|e| AppError::BadInput(e.to_string()))?;
        app.emit_to("main", "main://navigate", &args.section)
            .map_err(|e| AppError::BadInput(e.to_string()))?;
        return Ok(());
    }
    let url = format!("main.html?section={}", args.section);
    WebviewWindowBuilder::new(&app, "main", WebviewUrl::App(url.into()))
        .title("sharepaste")
        .inner_size(720.0, 560.0)
        .resizable(true)
        .build()
        .map_err(|e| AppError::BadInput(e.to_string()))?;
    Ok(())
}
```

- Single label `"main"` → at most one main window at any time.
- Already-open path: focus + emit `main://navigate` event with the requested section as payload.
- Unknown sections rejected at the boundary.

`open_modal` cmd, its handler entry in `lib.rs`, and the `modal-*` window labels are removed once nothing references them.

## UI structure

### Files

- New: `clients/desktop/ui/main.html` (`data-route="main"`).
- Removed: `clients/desktop/ui/modal.html`, `clients/desktop/ui/index.html`.
- Updated: `clients/desktop/ui/vite.config.ts` — `rollupOptions.input` becomes `{ popover, main }`.
- `clients/desktop/ui/popover.html` — unchanged.

### Routing

```tsx
// App.tsx
const route = document.body.dataset.route ?? "popover";
if (route === "main") return <Main />;
return <Popover />;
```

### `Main.tsx`

- Reads initial section from `?section=` query.
- Renders a tab bar (Accounts | Settings | Pairing) above the section content.
- Active tab is held in `useUiStore.mainSection` (new field).
- Subscribes to the `main://navigate` event; updates `useUiStore.mainSection` when it fires.
- Section content area renders one of the three section components based on `mainSection`.

### Section components

Move `clients/desktop/ui/src/modals/{Pairing,Settings,Accounts}Modal.tsx` to `clients/desktop/ui/src/views/sections/{Pairing,Settings,Accounts}Section.tsx`.

Per-component changes:
- Remove modal chrome: title bar, close button, "X" affordance.
- Remove `onClose` prop from `PairingModal`. The pairing flow exits when the user switches tabs or closes the window.
- Pairing state lives in the store, not local component state, so switching tabs and back preserves an in-progress pairing. Pairing is cancelled when the main window closes (component unmounted from a destroyed window).

## Popover changes

Removed:
- The inline pairing branch in `Popover.tsx` (`if (modal === "pairing") return <PairingModal/>`).
- The `modal` field and `setModal` setter on `useUiStore` (no remaining consumers in the popover).

Rewired (all via the new `openSection(section)` helper in `ui/src/ipc/commands.ts`, which calls `cmd.openMainWindow({ section })` then `cmd.hidePopover()`):
- Empty-state "Pair a device" → `openSection("pairing")`.
- "Choose account" button → `openSection("accounts")`.
- Footer entries that today open `accounts` / `settings` → `openSection(...)`.

Esc key handler simplifies to `cmd.hidePopover()` only (no modal-close branch).

## Tray menu

Tauri tray right-click menu (built in `lib.rs::build_tray`) gains three items above the existing `Quit`:

- `open_accounts` → `open_main_window({ section: "accounts" })`
- `open_pairing` → `open_main_window({ section: "pairing" })`
- `open_settings` → `open_main_window({ section: "settings" })`
- separator
- `quit`

Tray handler calls the cmd's underlying function directly in Rust (not via IPC) so the menu does not depend on a webview being alive. To make this clean, the body of `open_main_window` is extracted into a plain `fn open_main_window_impl(app: &AppHandle, section: &str) -> Result<(), AppError>`; the `#[tauri::command]` wrapper validates input and calls the impl, and the tray menu handler calls the impl directly.

## Edge cases

- **Main already open, request for a different section:** focus and emit `main://navigate`; UI flips tab. No second window.
- **Main already open, request for the same section:** focus only; the navigate event is idempotent.
- **Main closed while popover hidden:** no special handling; next open creates a fresh window.
- **App startup:** only popover built. Main is not created until first request.
- **macOS activation policy:** the app is currently tray-only (`Accessory`-style). When the main window opens, the dock icon must appear so the user can `Cmd-Tab` to it; on close, revert to `Accessory`. Implementation detail: call `app.set_activation_policy(Regular)` on main create and `Accessory` on `WindowEvent::Destroyed` for label `main`. Flag in the implementation plan; verify on macOS.
- **Pairing in-progress, user switches tab:** pairing state survives tab switches because it lives in the store. Cancelled when the main window closes (component unmounts).
- **Popover open + user triggers main:** popover hides explicitly via `openSection`'s second IPC call; not reliant on blur.

## Testing

### UI unit tests (`ui/src/__tests__/`)

- `Main.tsx` renders the correct section from `?section=` query (`accounts`, `settings`, `pairing`).
- `Main.tsx` updates the active tab when a `main://navigate` event fires.
- Popover empty-state "Pair a device" calls `openMainWindow({ section: "pairing" })` and `hidePopover`.
- Popover "Choose account" (no active account) calls `openMainWindow({ section: "accounts" })` and `hidePopover`.
- `openSection(section)` helper invokes both IPC calls in order.

### Rust tests (`src-tauri`)

- `open_main_window` returns `BadInput` for unknown sections.
- Existing `commands.rs` test layout: add a unit test for the section-validation branch. Window-creation paths are not unit-tested today; manual smoke covers them.

### Manual smoke

- Tray right-click → each menu item opens the main window on the correct tab.
- Open second section while main is already open → tab switches in place, no second window.
- Close main window, re-open from any entry → fresh window with correct tab.
- macOS dock icon appears on main open and disappears on main close.
- Popover hides immediately when an `openSection` is triggered from inside it.

## Migration order

The migration is structured so the app stays functional after each step.

1. **Rust:** add `open_main_window` cmd alongside the existing `open_modal`. Register handler in `lib.rs`. Add the `main://navigate` event constant. No frontend wiring yet.
2. **UI:** add `main.html`, `Main.tsx`, tab shell, `useUiStore.mainSection`. Wire Vite `rollupOptions.input`. Move modal section components into `views/sections/` and strip modal chrome. Add `openSection` helper.
3. **Tray:** add the three menu items + handlers, calling `open_main_window` directly in Rust.
4. **Popover:** rewire empty-state, "Choose account", footer entries to `openSection`. Drop the inline pairing branch and `useUiStore.modal`/`setModal`.
5. **Cleanup:** delete `open_modal` cmd, `modal.html`, `index.html`, and any leftover references in `App.tsx` and `vite.config.ts`.
6. **Tests + manual smoke** per the Testing section.

## Open questions

None at design time. macOS activation-policy implementation is flagged for verification during step 1 of the plan but the design decision (Regular while main is open, Accessory otherwise) is made.
