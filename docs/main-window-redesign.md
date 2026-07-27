# Main Window redesign — plan

Target: bring `clients/desktop`'s main window to the same FUI/HUD language as the
tray popover, add the History reader the product has never had, and rename
`Account` to `Pairing` from the tray menu down to the Rust command names.

The mock is a self-unpacking HTML bundle held **outside version control** at
`tmp-design/Sharepaste Main Window.html`, with its extracted payload beside it as
`tmp-design/Sharepaste Main Window.extracted.html` (`:934-1332` markup,
`:1334-1654` the state model). Those references resolve only on a machine that
has the artifact; this plan is written to stand without it.

Vocabulary is fixed by [`CONTEXT.md`](../CONTEXT.md), which this change extends
with **Active Pairing** and **Viewed Pairing** and which redefines **Main
Window**. The token set and `fui/` components landed with the
[popover redesign](popover-redesign.md) and are consumed here, not rebuilt.

---

## 0. Decisions

Settled in review; the rest of this document is downstream of them.

| # | Decision | Consequence |
|---|---|---|
| 1 | **History joins the Main Window as a full picker**, with a reading pane | [ADR 0003](adr/0003-the-main-window-reads.md) |
| 2 | The History pane carries a **pairing selector** in its header | Verified free: every entry command is user-scoped and none require the pairing to be active |
| 3 | Selecting is a **pure view — the Viewed Pairing** | New glossary term; never persisted; a band when Viewed ≠ Active |
| 4 | **Custom titlebar, window stays resizable** | `decorations(false)`, 980×680 default, 860×560 minimum, 3 new capability permissions |
| 5 | **`⏎` copies and stays; `⌘⌫` deletes.** No `⌘⏎` | Rejects the mock's bare `Delete`; keeps ADR 0002's hint-strip rule |
| 6 | **No masking** in the reading pane | Recorded as a rejection in ADR 0003 |
| 7 | **Cipher disclosed beside pairing**, per ADR 0002 — and it is `XCHACHA20-POLY1305` | The mock's footer `AES-256-GCM` is simply wrong |
| 8 | **`Pairing` replaces `Account` in copy and code** | [ADR 0004](adr/0004-pairing-not-account.md) |
| 9 | Card heads with the **username**, relay as subtitle | The `⌫` confirm strip names the full `user @ relay` as the guard |
| 10 | The popover's footer gains a **History icon that carries the selection** | One optional `entry_id` route param |
| 11 | The reader stops at the **same 100 entries** as the popover | List-end sentinel; ADR 0003 |

---

## 1. Corrections to the mock

Each deliberate, each with a reason the mock could not have known.

1. **`AES-256-GCM` → `XCHACHA20-POLY1305`, and it moves.** `core/crypto.rs:2-3`
   encrypts with `XChaCha20Poly1305`; the mock's badge names a cipher this
   product does not use. Per ADR 0002 it belongs beside pairing, not in permanent
   footer chrome.
2. **`LAST SYNC 14:22:07` → `LAST CONTACT 3m AGO`.** *Sync* and *last seen* are
   both on **Contact**'s _Avoid_ line; the value is relative everywhere else; and
   a wall-clock time is meaningless three days later.
3. **`// CONSOLE` is cut.** On **Main Window**'s _Avoid_ line, and redundant
   beside the `/ HISTORY` slot next to it.
4. **Non-active pairings light `standby`, not `alert`.** `list_accounts:51-54`
   reports `Disconnected` for every pairing without a session, which is the
   normal resting state. The mock's `LIGHT` map (`:1362`) would red-light two of
   three rows on every open. Alert is reserved for `AuthFailed`.
5. **Overlay stack takes the popover's audited values** — grid 0.22, vignette
   0.35, scanlines on chrome bands only at 0.25 — not the mock's 0.5/0.5/0.7,
   which cost ~9% of measured contrast on the text beneath.
6. **Reading pane at `--size-data` (13px)/1.6**, not the mock's 12px. It is the
   one surface whose whole job is being read.
7. **`V0.4.2` → the real version**, injected from `package.json` at build time.
8. **`pending` is actually rendered.** The mock's caption promises "queue depth
   per row" and its markup never draws it, though its own seed data carries
   `pending: 3` and `pending: 12`.
9. **Hint strip is `⏎ COPY · ⌘⌫ DELETE`**, not `↑↓ NAV · ⏎ COPY · ⌦ DEL`. ADR
   0002 cut the arrow hint and cut `DEL` specifically because it reads as the
   Delete key.

## 2. Gaps the mock leaves

- **Undecryptable entries.** `preview` is `""` for them (`commands.rs:649-653`),
  so the mock's pane would render blank. Alert `PanelMessage` — *"Encrypted with
  a key this device doesn't have."* — `COPY` disabled, `✕` live. Mirrors the
  popover's row rule.
- **Unbounded entries.** Nothing caps entry size: not capture, not
  `entries_cache`, not `POST /entries`. The pane renders the first 64 KB with a
  `SHOW ALL` control beyond it, and the byte readout is what explains why.
- **Row origin.** Follows the popover's rule — omitted when the entry came from
  this device, always stated in the detail pane's meta line.

---

## 3. Anatomy

```
┌ .fui-panel — 980×680 default, 860×560 min, notch, cyan hairline ─────────┐
│ titlebar 30   SHAREPASTE  / HISTORY                       ▼   ⤢   ✕      │ drag region
├──────┬───────────────────────────────────────────────────────────────────┤
│ rail │ header 38   History   [alice @ relay.fra-01 ▾]   HST · 100 KEPT   │
│  76  ├───────────────────────────────────────────────────────────────────┤
│      │ NOT SYNCING · LAST CONTACT 3d AGO                                 │ ← Viewed ≠ Active
│ ▤ HI │ ⌕ [ Filter entries…            ]  8/100      ⏎ COPY · ⌘⌫ DELETE   │
│ ◎ PA │ ┌─────────────────────────────┬───────────────────────────────┐   │
│ ⊕ SE │ │ 01  ss://Y2hh…  IPHONE · 6h │ ENTRY 01                 79 B │   │
│      │ │ 02  npm run dev        2m   │ ss://Y2hhY2hhMjAtaWV0Zi1wb2x5 │   │
│      │ │ …                           │ MTMwNTpQVMn0aGlzaXNub3RhcmVh… │   │
│      │ │ — OLDEST OF 100 KEPT —      │                               │   │
│      │ │                             │ IPHONE-15 · 13:48:55 · 34m    │   │
│      │ │                             │ [      COPY      ]        [✕] │   │
│      │ └─────────────────────────────┴───────────────────────────────┘   │
│ v0.1 │ [COPIED] ss://Y2hhY2hhMjAtaWV0Zi1wb2x5…                           │ ← toast
├──────┴───────────────────────────────────────────────────────────────────┤
│ ● ONLINE   ALICE@RELAY.FRA-01.SHAREPASTE.IO        LAST CONTACT 3m AGO   │ 30px
└──────────────────────────────────────────────────────────────────────────┘
```

The footer states **device-wide** facts and therefore always describes the
**Active Pairing**. The band under the pane header states **Viewed Pairing**
facts, and exists only when the two differ.

Rail items are 62px tall, glyph over a 9px uppercase label, selected state
carrying `--surface-active` plus a 2px `--cyan-500` left edge — the same
treatment `.fui-row[data-selected]` gives a list row, so "this is the selected
thing" reads identically in both places.

---

## 4. The rename (ADR 0004)

**Rust.** `commands::list_accounts` → `list_pairings`, `forget_account` →
`forget_pairing`, `set_active_account` → `set_active_pairing`, `AccountSummary` →
`PairingSummary`. `events.rs`: `ACCOUNT_ADDED`/`ACCOUNT_REMOVED`/`ACTIVE_CHANGED`
→ `PAIRING_ADDED`/`PAIRING_REMOVED`/`ACTIVE_PAIRING_CHANGED`, wire names
`pairing-added` / `pairing-removed` / `active-pairing-changed`, with the payload
structs renamed to match. `lib.rs`: tray id `open_accounts` → `open_pairings`,
`is_valid_section` becomes `history | pairings | settings | pairing`.

**Storage keeps `accounts`.** Renaming a SQLite table costs a migration on every
paired machine and buys a reader nothing this document does not. `accounts_repo`
is the one deliberate survival of the old word.

**TypeScript.** `Account` → `Pairing`; `store/accounts.ts` → `store/pairings.ts`
with `usePairingsStore` and `useActivePairing`; `cmd.listAccounts` →
`cmd.listPairings` and siblings; `events.onAccountAdded` → `onPairingAdded` and
siblings; `AccountsSection` → `PairingsSection`; and the pairing **flow**
`PairingSection` → `PairingFlow`, since the pane now owns the plural name.

**Test ids** move with their subjects: `use-`, `trash-`, `active-badge-` and
`add-account-row` → `pair-use-`, `pair-forget-`, `pair-active-`,
`add-pairing-row`.

---

## 5. Window chrome (§0.4)

`lib.rs:167-171` gains `.decorations(false)`, `.inner_size(980, 680)`,
`.min_inner_size(860, 560)`. `capabilities/default.json` gains
`core:window:allow-minimize`, `core:window:allow-toggle-maximize` and
`core:window:allow-start-dragging`; without the last, `data-tauri-drag-region` on
the titlebar is inert.

`styles.css` gains a `body[data-route="main"]` shell mirroring the popover's, and
`main.html` sets that attribute. The titlebar is the drag region, with the three
`IconButton`s excluded from it so a click on `✕` is a click and not a drag.

Verify on Windows that drag-to-edge Aero snap survives `start_dragging`.

---

## 6. History pane (§0.1, §0.2, §0.3, §0.5, §0.11)

`useFilteredEntries()` (`store/history.ts:30`) and `relativeAge` /
`normalizePreview` (`lib/format.ts`) are consumed unchanged — the filter matching
raw plaintext is what lets a query find a word on an entry's third line, which
matters more here than in the popover.

New UI state on `store/ui.ts`: `viewedUserId?: string`, defaulting to the Active
Pairing and reset to it whenever the window mounts. `store/history.ts` keys
nothing per user, so switching the Viewed Pairing re-hydrates it via
`list_history`.

| Row state | Meta column |
|---|---|
| Origin is another device | `IPHONE-15 · 6h`, label truncated to 12 chars, full in `title` |
| Origin is this device | `2m` |
| Undecryptable | `KEY MISMATCH`, alert tone |

Detail pane: header `ENTRY 04` + byte count (`new TextEncoder().encode(text).length`,
rendered `79 B` / `4.2 KB` / `1.1 MB`); body mono 13px/1.6, `pre-wrap`,
`break-word`, capped at 64 KB with `SHOW ALL`; meta line
`IPHONE-15 · 13:48:55 · 34m` (the clock time only when the entry is from today,
otherwise a date); solid `COPY` plus an alert-hover `✕`.

Empty states via `PanelMessage`: `NO PAIRINGS ON THIS DEVICE`, `HISTORY EMPTY`,
and `NO MATCHES` + `Nothing matches "q"` + `⌫ CLEAR FILTER`.

The list-end sentinel — `— OLDEST OF 100 KEPT —` — renders only when the list is
at the cache cap, so a user with nine entries never sees a limit that has not
bitten them.

---

## 7. Pairings pane (§0.8, §0.9)

Card: heading `username ?? user_id` in the display font; subtitle
`user_id @ host`; `THIS DEVICE: MBP-14`; `StatusLight` with `ONLINE` / `STANDBY`
/ `AUTH FAILED`; amber `3 PENDING` when non-zero; `USE`, `+ DEVICE`, `⌫`. Active
card takes a `--cyan-a40` border and a `--cyan-500` left stripe; a pairing in
`AuthFailed` takes an `--alert-400` stripe.

`+ DEVICE` calls `pair_start` for **that** pairing and opens the show-code panel
inside that card. `PairingFlow.tsx:67` restricts show-code to the active pairing
for no reason the command shares — `pair_start` takes any `user_id`. With
show-code on the card, the chooser drops to two options: invite and code.

`⌫` opens the existing confirm strip, whose text names the full
`user_id @ server_url` rather than the heading, because two pairings can share a
heading and this is the one action that cannot be undone.

The cipher line — `XCHACHA20-POLY1305` — sits in the card's footer beside the
relay, where the decision to trust that relay is being made.

---

## 8. Settings pane

Structural parity with the mock, restyled onto the tokens: a `CAPTURE` group
(capture toggle, launch at login) with an `ON` / `PAUSED` readout beside each
switch; `GLOBAL HOTKEY` keeping the existing blur/Enter commit
(`SettingsSection.tsx:52-59`) and its explanatory hint; `DENY-LIST` with an
`N APPS` count in its header; and a `DESTRUCTIVE` group, alert-bordered, whose
`CLEAR…` is disabled without an Active Pairing and whose confirm strip names the
scope.

---

## 9. Wiring (§0.10)

`is_valid_section` gains `history` and `pairings`. Tray gains `History…` and
renames `Accounts…` → `Pairings…`. `OpenMainWindowArgs` gains
`entry_id: Option<i64>`, appended to the URL as `&entry=` and consumed once by
`Main` on mount to seed the History selection; a stale id simply selects nothing.

The popover's `Footer.tsx` gains a third `IconButton` (`aria-label="History"`)
that passes the currently selected entry. It does **not** get a hint-strip entry:
ADR 0002 established there is no width for a fourth, which is exactly why the
affordance is a visible icon rather than a hidden binding.

---

## 10. Work breakdown

**P0 — rename.** §4, everything. Every later phase edits renamed files, so this
lands first and alone.

**P1 — chrome + shell.** §5, `Titlebar`, `Rail`, pane header, footer, toast;
`Main.tsx` becomes the shell.

**P2 — History.** §6, plus the Viewed Pairing selector and band.

**P3 — Pairings.** §7, including `+ DEVICE` per card and the cipher line.

**P4 — Settings.** §8.

**P5 — wiring.** §9: route, tray, popover handoff.

**P6 — tests.**

## 11. Test impact

`Main.test.tsx` asserts `role="tablist"` and `tab-accounts`; the rail replaces
tabs, so it moves to `role="tab"` items inside the rail with `rail-history` /
`rail-pairings` / `rail-settings`. `AccountsSection.test.tsx` and
`PairingSection.test.tsx` rename with their subjects. `helpers.ts` mock IPC
learns `list_pairings` and the renamed events.

New: the reader renders full multi-line plaintext where the row shows one
collapsed line; `⏎` copies without closing and `⌘⌫` deletes while the filter has
focus, and a bare `Backspace` does **not**; an undecryptable entry disables
`COPY`; the sentinel appears at 100 rows and not at 9; selecting a non-Active
pairing renders the band and does not call `set_active_pairing`; the popover's
History icon passes the selected entry id.

## 12. Risks

- **`start_dragging` and Aero snap** on Windows is unverified from here.
- **A 64 KB pane cap is a guess** at where rendering hurts; nothing measures it.
- **The rename touches every layer at once.** It is mechanical, but a missed
  event-name string fails silently at runtime rather than at compile time — the
  Rust constant and the TS listener are the two halves to keep in step.
- **Two pairings to one user still look alike** at a glance (§0.9); the confirm
  strip is the only guard on the destructive path.
