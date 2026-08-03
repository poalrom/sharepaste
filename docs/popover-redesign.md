# Tray popover redesign — plan

> Completed plan, kept as a record. Three things it names have moved since.
>
> Its Rust citations predate the extraction of `sharepaste-core`: paths of the form `core/…`
> and `commands.rs` were relative to `clients/desktop/src-tauri/src/` and now live under
> `clients/core/src/`. See the note at the top of
> [ADR 0006](adr/0006-one-protocol-three-shells.md).
>
> And §6's `normalizePreview()` no longer exists in the UI, nor do `originLabel` and
> `relayHost`: the core owns those three renderings in `clients/core/src/render.rs`, as
> `preview`, `origin_label` and `relay_host`, so a phone and a desktop cannot cap a Preview
> at two different limits. `relativeAge` stayed in `clients/desktop/ui/src/lib/format.ts`.
> The cap moved with the rule — 80 characters in the core, not the 200/400 this plan
> prescribes — and `Entry` now carries a rendered `preview` beside the full `plaintext`
> rather than one field meaning both. Read §6, the P1 list and the verification list with
> that substitution.
>
> The last is its delete binding. `⌘⌫` / `Ctrl+⌫` clears the search input now, and
> deleting the selected entry is `⇧⌘⌫` / `CTRL+SHIFT+BKSP`: read decision 13, §9's binding
> table, the hint strip at §139 and the P1 and verification lists with that substitution.
> The reason the plan gives for wanting a modifier at all — the input holds focus
> essentially always — is exactly the reason the unmodified combination went back to it.
> See [ADR 0013](adr/0013-the-filter-is-a-text-field-first.md).

Target: bring the `clients/desktop` popover to the FUI/HUD language of the
design mock, with readability corrections where the mock trades legibility for
style, and with every element it draws either implemented for real or
deliberately cut.

The mock is a self-unpacking HTML bundle held **outside version control** at
`tmp-design/Sharepaste Popover.html`, with its extracted plain-HTML payload
beside it as `tmp-design/Sharepaste Popover.extracted.html` (`:935-1147`
markup, `:1149-1367` the state/row model). Those line references resolve only
on a machine that has the artifact; this plan is written to stand without it —
every value the implementation needs is transcribed here.

Vocabulary is fixed by [`CONTEXT.md`](../CONTEXT.md): **User**, **Device**,
**Device Label**, **Pairing**, **Relay**, **Entry**, **Origin**, **Contact**,
**Pending**, **Undecryptable**. New code and every user-visible string use those
words.

---

## 0. Decisions

Settled in review; the rest of this document is downstream of them.

| # | Decision | Consequence |
|---|---|---|
| 1 | **The popover stays a picker**, not a status surface | Telemetry must earn its band; rows win ties |
| 2 | **No cache gauge, no permanent telemetry strip.** `LAST CONTACT` folds into a strip shown only when degraded | 116px chrome nominal; `entries_cache::count_for` never built |
| 3 | Footer shows the **username**, and only when >1 pairing exists | `GET /me` must carry it; `N PAIRED` in the no-active state |
| 4 | **Device metadata travels out of band** via one `GET /me` | [ADR 0001](adr/0001-device-metadata-out-of-band.md) |
| 5 | **Contact is stamped from relay heartbeats**, tapped below the SSE parser | Exact liveness, no relay change, one DB write per outage |
| 6 | **Keep the explicit `⧉` button**: row = copy + hide, `⧉` = copy + stay | `COPIED` strip becomes reachable |
| 7 | **No cipher badge in the popover** | Sync payload is `{ user_id, last_contact_at }` |
| 8 | **Origin shown only for entries from other devices**; no per-row `CACHED` | Column is silent until it has news |
| 9 | **Tokens global, shell scoped, `fui/` shared**; Main Window is a planned follow-up | No throwaway work |
| 10 | **Opaque window, faked notch** | `popover.rs` geometry untouched |
| 11 | **No vendored fonts** | `--font-display` falls back to the UI stack |
| 12 | **Decryption banner deleted**; the row is the surface | `decryption-error` testid retired |
| 13 | **`⌘⌫` deletes; no confirmation anywhere** | See §9 — `✕` stays unguarded, deliberately |

---

## 1. Readability audit — measured

Contrast against `--surface-panel` #070c12 (WCAG relative luminance).

| Mock token | Ratio | Verdict | Change |
|---|---|---|---|
| `--text-body` #9fc2cc | 10.3:1 | pass | keep for chrome; previews get a brighter tone |
| `--text-muted` #5d7c88 | 4.39:1 | borderline fail at 10–11px | → **#8fb0bb (8.5:1)** |
| `--text-dim` #3b545f | **2.45:1** | fail — carries the row index, meta, hint strip, ESC chip | → **#6d8b98 (5.42:1)** |
| `--text-dim` under the scanline overlay | **2.24:1** | worse in situ | overlay policy below |
| preview (`--type-data`, 12px mono) | — | primary content at the panel's smallest size | → **13px/1.3**, **#b3d0d9 (12.1:1)** |
| `--text-dim` index on a selected row | 4.35:1 | just under | index switches to `--text-emitter` when selected |

Deviations from the mock, each deliberate:

1. **Overlay stack.** grid 0.55 + scanlines 0.55 + vignette 0.75 sit under the
   list text; the scanline layer alone (22% black on every third pixel) costs
   ~9% of measured contrast. Take: grid **0.22**, vignette **0.35**, scanlines
   **only on chrome bands** at **0.25**, never behind row text.
2. **Sweep animation.** Play **once on window show** (the `focus` event already
   wired at `Search.tsx:30`), 900ms; nothing under
   `prefers-reduced-motion: reduce`, which also gates the status-light pulse.
3. **Typeface.** Share Tech Mono is a thin display mono with ambiguous `0/O`,
   `1/l` — wrong for paths, base64 and `ss://` URLs. Take the **system mono
   stack** for all data, system UI stack for everything else (§0.11).
4. **Tracking.** 0.22em for single words (`SHAREPASTE`, `COPIED`, `NO MATCHES`);
   **0.06em** for multi-word readouts (hint strip, meta, account).
5. **No hover reflow.** The mock swaps meta → buttons on hover. Take a
   permanently reserved 44px action slot (opacity 0 → 1); meta stays visible.
6. **Preview normalisation.** `preview` is the full plaintext
   (`commands.rs:367`), so an indented or multi-line entry renders as leading
   whitespace — a visually empty row — and up to 100 unbounded strings enter the
   DOM. Take `normalizePreview()`: collapse `\s+`, trim, slice to 200 chars;
   400 chars in `title`.
7. **Chrome budget.** Nominal = header 26 + search 40 + hint 20 + footer 30 =
   **116px**, leaving 364px: **10 full rows plus a deliberate sliver of the
   11th**, which is how the list signals it scrolls. The mock spends 139px for
   10 rows.

---

## 2. Token set

`:root` in `ui/src/styles.css` — **global**, so the Main Window inherits the
palette the day it is converted. Only the *shell* (grid, scanlines, vignette,
notch, panel frame) scopes to `body[data-route="popover"]`.

Adopted verbatim: void/cyan/amber/alert/nominal ramps, cyan alphas, spacing
scale, notch clip-paths, durations, glow vars.

```css
--text-body:  #b3d0d9;   /* was #9fc2cc — row previews          12.1:1 */
--text-muted: #8fb0bb;   /* was #5d7c88 — meta, account, hints   8.5:1 */
--text-dim:   #6d8b98;   /* was #3b545f — index, separators      5.4:1 */
--font-ui:    system-ui, -apple-system, "Segoe UI", sans-serif;
--font-mono:  ui-monospace, "SF Mono", "Cascadia Mono", Consolas, monospace;
--font-display: var(--font-ui);        /* no vendored face */
--size-data:  13px;
--row-h:      34px;
--surface-active: rgba(53,230,246,0.12);
```

Tailwind maps the semantic names onto the vars; `.fui-panel`, `.fui-row`,
`.fui-strip` carry the clip-path and layered-gradient work in
`@layer components`.

---

## 3. Panel anatomy

```
┌ .fui-panel (360×480, notch, cyan hairline, inset glow) ────┐
│ header 26px   SHAREPASTE // RELAY                    [ESC] │
│ search 40px   ⌕ [ Search history…              ]     8/11  │
│ strip  24px   OFFLINE · LAST CONTACT 3m AGO                │ ← degraded only
│ list   flex   01  ss://Y2hhY2hh…    IPHONE-15 · 6h [⧉][✕] │   34px rows
│               02  npm run dev                 2m  [⧉][✕]  │   own-device row
│               …  — OLDEST OF 100 CACHED —                  │ ← list-end sentinel
│ toast  auto   [COPIED] ss://Y2hhY2hh…                      │ ← conditional
│ hint   20px    ↑↓ NAV · ⏎ COPY · ⌘⏎ KEEP · ⌘⌫ DEL          │
│ footer 30px   ● ONLINE  2 PENDING  ALICE          ◎  ⊕     │
└────────────────────────────────────────────────────────────┘
```

Shared components in `ui/src/views/fui/` — `StatusLight`, `IconButton`
(`aria-label` required), `Notice`, `PanelMessage`, `Strip`. Built shared, not
popover-private, because the Main Window conversion consumes all five.

Row columns, fixed so nothing reflows:
`[16px index][flex preview, truncate, title][auto meta][44px actions]`.

**Meta column** (§0.8):

| Row state | Meta |
|---|---|
| Origin is another device | `IPHONE-15 · 6h` — label truncated to 12 chars, full in `title` |
| Origin is this device | `2m` — device omitted |
| Undecryptable | `KEY MISMATCH`, alert tone |
| The relay has never stamped it | empty |
| Refused | the reason, alert tone |

*The last three rows added on `pendings`, 2026-08-03.* **The slot always shows the
relay's last word**, and it is decided in one place — `timeSlot` in `EntryRow.tsx`,
which both lists import so the two cannot drift. The precedence, top down: the
refusal reason; `KEY MISMATCH`; nothing at all when `last_use === 0`; otherwise
`relativeAge(last_use)` with the Origin beside it.

A refusal wins over `KEY MISMATCH` because it is the actionable one. The empty
slot is gated on the stamp and **not** on `pending`, which is the load-bearing
detail: `entry-settled` deliberately carries no timestamp — a stamp per acked act
is the refetch that event exists to avoid — so a row that settles in place is a
settled row still holding `last_use === 0`, and gating on `pending` would print
`655mo` on the newest row in the list the moment the queue drained. There is one
clock in this system and it is the relay's (ADR 0014); silence is what this device
honestly has until a snapshot brings the stamp.

Rows are **not** dimmed when offline and carry no per-row `CACHED` marker — a
window-level fact is stated once, in the degraded strip. *(Follows from §0.8;
resolved by extension rather than asked.)*

*Extended on `pendings`, 2026-08-03.* Still true and still the right rule, and it
is what makes the amber tint admissible rather than a contradiction of it:
**offline is a window-level fact, un-flushed is a per-row one.** A row the relay
has not heard the latest word about takes `rgba(245, 182, 66, 0.08)` behind it —
the same amber as the `N PENDING` in both footers — and nothing on it is
recoloured, which is the half of this rule that ADR 0002 cut the `CACHED` marker
for. A refused row is the exception, and earns it by carrying an action.

`PanelMessage` covers `NO ACCOUNTS PAIRED` (solid `PAIR A DEVICE`), `NO ACTIVE
ACCOUNT` (outline `CHOOSE ACCOUNT`), `HISTORY EMPTY`, and `NO MATCHES` +
`Nothing matches "q"` + `⌫ CLEAR FILTER`.

The search suffix needs the filtered count, which lives in `HistoryList` —
extract `useFilteredEntries()` into `ui/src/store/history.ts`, consumed by both.

---

## 4. `GET /me` — user and devices (ADR 0001)

`EntryView.device_label` already exists on the wire (`events.rs:43`,
`types.ts:9`) and is hard-coded `None` at both producers (`commands.rs:370`,
`session.rs:207`). Labels live in `memberships.device_label`
(`db/repository.ts:28`); `username` lives in `users` (`db/migrate.ts:6`).
Neither is reachable by any client today.

**Relay**
- `db/repository.ts`: `memberships.listByUser(user_id)`.
- `server/routes/devices.ts`: `GET /me`, authed via `verifyBearer`, returning
  `{ user: { id, username }, devices: [{ device_id, label, created_at, revoked_at }] }`
  built field-by-field — never `SELECT *`, so `device_token_hash` and
  `token_sha256` cannot leak. `label` is `device_label ?? null`.

**Desktop**
- `http/dto.rs`: `MeResp`, `UserDto`, `DeviceDto`.
- `http/client.rs`: `me()`, authed GET, mirroring `list_entries`
  (`client.rs:126-137`).
- `storage/migrations.rs`: `devices(user_id, device_id, label, revoked_at,
  updated_at, PRIMARY KEY(user_id, device_id))`; `accounts` gains
  `username TEXT` and `last_contact_at INTEGER`.
- `storage/devices.rs`: `upsert_many`, `map_for(conn, user_id) -> HashMap<String,String>`.
- `sync/session.rs`: fetch and mirror immediately before
  `set_conn_state(Online)` (`session.rs:181`); an entry from an unmirrored
  `device_id` triggers one refresh, debounced ≥60s.
- `commands.rs::list_history` and `session.rs:207` fill `device_label` from the
  mirror; `list_accounts` gains `username`.

**UI** — origin label per §3; unlabelled legacy memberships fall back to a
4-char `device_id` slice with the full id in `title`.

---

## 5. Contact (§0.5)

`routes/events.ts:15-17` writes `: heartbeat` every 15s; `sse.rs:29` sets a 45s
read timeout. But `sse.rs:40` parses with `eventsource_stream::Eventsource`,
which follows the WHATWG dispatch rules — a comment line dispatches **no
event**, so the heartbeat is invisible above the parser.

The tap therefore goes **below** it:

- `sse::run` takes an `Arc<AtomicI64>`; `resp.bytes_stream()` is wrapped so
  every chunk stamps `now_ms()` before `.eventsource()`. Any byte from the relay
  is proof of contact, comments included — and the `: connected` preamble stamps
  the instant the stream opens.
- `list_entries` success stamps the same cell, covering the backfill window.
- **No relay change**, no new `ServerEvent` variant.
- Cost while healthy: one atomic store per 15s. The value is **persisted only on
  the `Online` → not-`Online` transition**, so an outage costs exactly one write.

Surfaced as `{ user_id, last_contact_at }` on a new `sync-state` event, plus
`get_sync_state(user_id)` for hydration (the popover opens long after the last
event fired). Rendered **relative** — `LAST CONTACT 3m AGO`, `NEVER` — by the
same formatter the rows use. Never rendered while `Online`, because the strip
that carries it does not exist then.

---

## 6. Copy, delete, and the strips (§0.6, §0.12, §0.13)

| Action | Behaviour |
|---|---|
| row click, `⏎` | copy + hide — the fast path, unchanged |
| `⧉` (`title="Copy and keep open"`), `⌘⏎`/`Ctrl+⏎` | copy, **stay open**, cyan `COPIED` strip 2.2s |
| copy fails, any path | stay open, same strip in alert tone |
| click/`⏎` on an undecryptable row | stay open, alert strip: *"Can't copy — this entry was encrypted with a key this device doesn't have."* |
| `✕` | delete immediately (unchanged) |
| `⌘⌫` / `Ctrl+⌫` | delete the selected entry |

`⌘⌫` rather than `⌦`/`Backspace` because the search input holds focus
essentially always (`Search.tsx:20-32` focuses it on mount and on every window
`focus`, and the window is shown/hidden rather than unmounted) — a bare key
would either collide with text editing or never fire.

Undecryptable rows stay arrow-navigable so the `01..11` index stays continuous;
only `✕` is live on them. The global decryption banner is deleted — the row is
local, persistent, and points at the actual entry, whereas the banner named an
entry id (`Popover.tsx:82`) that appears nowhere in the UI.

*Extended on `pendings`, 2026-08-03.* The selected-row controls gain `↻` beside
`⧉` and `✕`, shown only on a **Refused** row: the relay turned that act down for
what it is, and a **Resend** is a fresh act rather than a retry, so it leads the
History afterwards and carries nothing forward from the refusal (ADR 0015). Both
`⧉` and `✕` stay live on a refused row — its text is stranded on this device, so
copying it must stay possible, and deleting it withdraws the act with it.

The footer below is **unchanged**, and deliberately: the counts all stay, on both
footers and both pairing cards (ADR 0014). A row you can see does not make the
number redundant — it states how many acts are owed, which the region's height
does not.

Footer: `Online` → nominal `ONLINE`; `Connecting` → caution `SYNCING` (pulse,
reduced-motion aware); `Disconnected` → standby `OFFLINE`; `AuthFailed` → alert
`AUTH FAILED` + the degraded strip carrying `last_error`. `pending > 0` → amber
`N PENDING`. Username shown only when >1 pairing exists; `N PAIRED` when no
pairing is active.

Ages tick via `useNow(60_000)`, running only while `document.hasFocus()` and
recomputing on `focus`. `created_at` is epoch ms (`routes/entries.ts:33`).

---

## 7. Work breakdown

P1–P2 (UI) and P3–P4 (backend) touch disjoint files and can run in parallel
after P0.

**P0 — tokens + shell.** `styles.css`, `tailwind.config.ts`, `Popover.tsx`
frame/header/hint strip, `Footer.tsx` → `StatusLight` + `IconButton`.

**P1 — list.** `lib/format.ts` (`relativeAge`, `normalizePreview`),
`useFilteredEntries`, `EntryRow.tsx` (new anatomy, meta rules, undecryptable),
`HistoryList.tsx` (`⌘⌫`, empty variants, list-end sentinel), `Search.tsx`
(prefix + count suffix).

**P2 — strips.** `Notice`, `PanelMessage`, `Strip`; degraded strip
(offline / auth-failed / contact); `COPIED` and copy-failure toast; banner
removal.

**P3 — `GET /me` (§4).** `server/src/db/repository.ts`,
`server/src/server/routes/devices.ts`; then `dto.rs`, `client.rs`,
`migrations.rs`, `storage/devices.rs`, `session.rs`, `commands.rs`.

**P4 — contact (§5).** `sse.rs` byte tap, `session.rs`, `state.rs`,
`storage/accounts.rs`, `events.rs`, `commands.rs`; `ipc/{commands,events}.ts`,
`store/sync.ts`, `types.ts`.

**P5 — polish.** Sweep-on-show, reduced-motion gate, scrollbar styling,
`AccountsSection.tsx:84` heading fix (stop presenting a Device Label as the
account's name).

**P6 — tests + verification.**

**Follow-up, not this change:** Main Window conversion (711 lines across
`Main.tsx` + 3 sections, ~60 hard-coded palette classes), consuming these tokens
and `fui/` components; cipher disclosure beside pairing.

---

## 8. Test impact

Preserved: `findByPlaceholderText("Search history…")`; `aria-label="Settings"` /
`"Accounts"` on the icon buttons so `getByRole` still resolves;
`data-testid="choose-account"`; `entry-row`, `data-selected`,
`delete-entry-{id}`, `aria-label="Delete entry"`.

Changed: `Popover.test.tsx:112-123` moves from asserting the banner to asserting
the row treatment; `migrations.rs:57` `creates_all_four_tables` is renamed and
gains `devices`; `helpers.ts` mock IPC learns `get_sync_state` and `me`.

New:
- Relay: `GET /me` returns only the caller's user and devices, never token
  material, includes revoked devices, 401s unauthenticated.
- Rust: `devices` migration; `devices::upsert_many`/`map_for`; `list_history`
  fills `device_label` and tolerates unmirrored ids; a byte on the SSE stream
  stamps contact; contact persists on the `Online`→offline transition.
- UI: `relativeAge`/`normalizePreview`; `⌘⌫` deletes while the input has focus;
  bare `Backspace` does **not**; `⧉` copies without calling `hide_popover` and
  shows the toast, row click still hides; undecryptable row never calls
  `copy_to_clipboard` and shows the explanatory strip; degraded strip renders
  `NEVER` when `last_contact_at` is null; origin label absent on own-device rows.

## 9. Verification

1. `npm --prefix server test`, `npm --prefix clients/desktop/ui test`,
   `cargo test --manifest-path clients/desktop/src-tauri/Cargo.toml`.
2. `npm --prefix clients/desktop/ui run build`.
3. Two devices with distinct labels: copy on one, confirm the other's popover
   names the **origin** device and omits it for its own rows.
4. `cargo tauri dev`: filter (suffix tracks), `↑↓`, `⏎` copies + hides, `⧉`
   copies + stays + toast, `⌘⌫` deletes, `Esc` hides.
5. Kill the relay mid-session: light → `OFFLINE`, degraded strip shows
   `LAST CONTACT` within ~45s of the true drop, rows keep their origin and age.
   Restart and watch it clear.
6. Revoke the device token: `AUTH FAILED` + `last_error` in the strip, history
   still readable.
7. Contrast spot-check with overlays composited (preview ≥12:1, secondary
   ≥4.5:1); `prefers-reduced-motion: reduce` → no sweep, no pulse.

## 10. Risks

- **`✕` is a single unguarded click that deletes an entry on every device**
  (`routes/entries.ts:81-84` fans a delete out over SSE), and undo is not
  buildable — re-uploading the retained ciphertext mints a *new* entry, not a
  restoration. This deviates from the confirm-strip convention used at
  `AccountsSection.tsx:173` and `SettingsSection.tsx:138`. Accepted per §0.13;
  the modifier on `⌘⌫` is the only guard.
- **Stale origin labels.** A renamed device reads by its old label until the
  next reconnect; unmirrored ids trigger one debounced refresh.
- **`GET /me` is a new authed surface.** Explicit DTO mapping plus a test
  asserting no token material is the guard.
- **Contact can overstate liveness by ≤45s** when a stream dies silently, bounded
  by the `sse.rs:29` read timeout.
- **`scrollbar-color`** needs Chromium 121 / Safari 18.2. The list uses
  `scrollbar-width: thin; scrollbar-color: var(--cyan-a40) transparent`, with
  the WebKit fallback shipping alongside: `::-webkit-scrollbar { width: 8px }`,
  track `var(--void-1000)`, thumb `var(--void-400)`, thumb hover
  `var(--cyan-700)`.
