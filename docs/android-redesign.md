# Android redesign — record

> Written after the fact, not before it. The popover and the Main Window each got
> a plan document first; this one was implemented directly from the mock and this
> file records what was built, what was corrected, and what was deliberately cut.
> Read it the way you read the other two: as the argument, not the diff.

Target: bring `clients/mobile/android` to the FUI/HUD language the desktop
already speaks, rework the information architecture inside the platform's
constraints rather than around them, and leave every element the mock draws
either implemented for real or deliberately dropped with a reason.

The mock is a self-unpacking HTML bundle held **outside version control** at
`tmp-design/Sharepaste Android App.html`, with its extracted payload beside it as
`.extracted.html`. Those references resolve only on a machine that has the
artifact; every value the implementation needs is transcribed here.

Vocabulary is fixed by [`CONTEXT.md`](../CONTEXT.md): **User**, **Device**,
**Device Label**, **Pairing**, **Active Pairing**, **Viewed Pairing**, **Relay**,
**Entry**, **Preview**, **Origin**, **Contact**, **Pending**, **Undecryptable**,
**Standing Actions**. Every user-visible string uses those words.

---

## 0. Decisions

| # | Decision | Consequence |
|---|---|---|
| 1 | **The phone gets a palette, and it is the desktop's** | `Theme.kt` loses Material You; `Fui.kt` ports `ui/src/styles.css` |
| 2 | **Ported from the desktop's corrected tokens, not from the mock's** | The mock's `--text-dim` fails WCAG; the desktop measured and raised it once (§1) |
| 3 | **Contact is permanent chrome**, inverting ADR 0002 rather than copying it | "Not in contact" is the nominal reading on a phone (ADR 0007) |
| 4 | **The foreground-only fact is pinned chrome**, clipped to one line with `WHY ▸` | It was the list's first item, so it was the first thing to scroll away |
| 5 | **Recall Latest outranks Offer**: solid, first, `1.6f` against `1f` | It is the verb that must never hand over something stale |
| 6 | **One target per Entry row; Delete is a swipe** | Two word-buttons was 20 targets a screen, destructive beside safe |
| 7 | **An Undecryptable row keeps both controls inline** | Recall disabled-not-hidden; Delete is the only thing left to do with it |
| 8 | **Erasures are confirmed inside the card**, never in a dialog | The scope stays on screen while the choice is made |
| 9 | **Three N/A chips** for the settings a phone does not have | A missing switch and an unbuilt screen look identical without them |
| 10 | **Insets are applied once, at the app root** | Android 15 draws edge to edge whether or not the app asked |
| 11 | **No vendored fonts** | Same call as the desktop: Share Tech Mono's `0/O` and `1/l` are wrong for `ss://` URLs |
| 12 | **No light scheme** | A HUD is emitted light on a void; a light rendering is a different design and a second audit |

---

## 1. Why the tokens come from the desktop and not the mock

Measured against the panel (`#070c12`), the mock's own text ramp fails at the
sizes a phone uses: `--text-muted` `#5d7c88` is 4.39:1 and `--text-dim` `#3b545f`
is 2.45:1. `docs/popover-redesign.md` §1 raised them once, with numbers, and
those raised values are what shipped in `clients/desktop/ui/src/styles.css`. A
phone re-deriving its palette from the mock would fail the same audit a second
time, so `Fui.kt` is a port of the CSS, not of the design file. The ratios are
carried in comments beside each colour.

The same applies to the atmosphere. Grid at **0.22** and vignette at **0.35**,
which are the mock's own reduced values; **scanlines only on a chrome band**,
never behind an Entry's Preview, because the desktop measured the full overlay
stack at roughly 9% of contrast.

One glyph correction the mock could not have known: its Recall arrow is **⤓**
(U+2913), which no font bundled with Android carries. It falls back to a
different arrow on a good device and to a tofu box on a thin one, so the row uses
**↓**. `Fui.Glyph`'s KDoc lists the glyphs that are safe and says why.

---

## 2. Anatomy

```
┌ 52dp  identity ── SHAREPASTE / user @ relay host ──── [SHOWING] ── ◎ ─┐
├ 30dp  Contact ─── ▪ IN CONTACT WITH THE RELAY ── scanlines ──────────┤
├ 30dp  policy ──── ⌾ NOTHING ARRIVES WHILE THIS IS CLOSED ── [WHY ▸] ─┤
│       (open: the verbatim sentence + four NO-… chips)                │
├─ conditional bands, each its own colour rule ────────────────────────┤
│  standing actions unreachable · notice · divergence · pending        │
├─ LazyColumn, 68dp rows ──────────────────────────────────────────────┤
│  preview (mono 14sp, one line)                              [ ↓ ]    │
│  FROM MBP-14  (only when the Origin is another Device)               │
│  ← swipe reveals a 96dp alert panel: ✕ DELETE                        │
│  Undecryptable: alert tint, left rule, ⊘ marker, [↓ off] [✕]         │
├ 48dp  verbs ───── [ RECALL LATEST solid 1.6f ] [ OFFER 1f ] ─────────┤
└──────────────────────────────────────────────────────────────────────┘
```

112dp of chrome, fixed. The pairing flow and the Pairings screen share the
52dp `TitleBand`; the pairing flow adds a 34dp footer carrying
`XCHACHA20-POLY1305` and `RELAY MUST BE HTTPS` — the two facts a phone cannot
discover for itself, beside the moment a Relay is being trusted (ADR 0002).

---

## 3. What moved, and why

**Contact.** Was a list item in the ordinary body colour, shown for every phase.
Now a permanent 30dp band with a lit square: nominal green only for *in contact*,
standby for out-of-contact and resting, caution while looking, alert for the one
true fault. Green is the exceptional state on a phone; a band that went green
whenever nothing was wrong would be grey almost always and read as a warning.

**The foreground-only note.** Was the list's second item, and therefore the
second thing to leave the screen. Now one clipped amber line in chrome with the
verbatim sentence and four `NO …` chips one tap behind `WHY ▸`. The open/closed
choice is `rememberSaveable`, not a field on `UiState`: it changes nothing about
the phone.

**The verb bar.** Two equal outlines were truthful about their symmetry in the
code and mute about which one a person reaches for — and left the screen without
a single emitter. Recall Latest is now the solid one and comes first.

**The Entry row.** Preview, and an Origin line only when the Entry came from
another Device. One 48dp Recall target. Delete is a left swipe onto a 96dp alert
panel, which is the guard the desktop's unguarded `✕` never had: a delete fans
out over SSE to every paired device and cannot be undone. The swipe **asks** and
springs back — the list is the source of truth, so a delete the Relay refuses
does not hide a row that is still there.

**The newest row is drawn as the emitter's**: a 2dp cyan rule, the emitter's own
tint, a brighter Preview and a filled 96dp `RECALL` in place of the glyph. That
is the row `RECALL LATEST` will hand over, and the bar's whole argument — that
Recall is the verb that must never hand over something stale — needs the list to
say which row it means.

**The Pairing card.** Headed by the **User** and `user_id @ relay host`, never by
this machine's Device Label — the desktop's mistake, and the card is where it was
made. The Device Label is a line inside it. Exactly one card carries `SYNCING`;
`SHOWING` moves independently. An erase is armed inside the card with a
`CANNOT BE UNDONE` badge, the sentence naming the User *and* the Relay, and
`KEEP IT` / the destructive verb — the destructive one solid, because the person
already asked for it and the strip exists to make them read what it costs.

**Settings.** Two quoted notes at full length plus `WATCHED CAPTURE · N/A`,
`DENY-LIST · N/A` and `UPDATE CHECK · NONE`. The third is the one the desktop
cannot show: a phone carries no update code at all (ADR 0008), so the Relay is
its only counterparty.

**Notices.** Each now carries a label naming the outcome — `OFFERED`,
`RECALLED`, `NOT PAIRED`, `MAY BE STALE`, and one per reachable refusal
(`NOTHING TO SEND`, `TOO BIG · 64 KB CAP`, `ALREADY HERE`). They are the same
labels the invisible activity and the share target now put above the same
sentences in their Toasts, because a Standing Action and an in-app press are the
same operation and reporting it in two idioms would make them look like two.
**The log line those two write is still the bare sentence**, which is a contract
with `StandingActionsNotificationTest` and with the acceptance sequence.

Only the stale Recall tints its whole band: it is the only notice about *what is
now on the clipboard* rather than about what the app just did. A refusal is ruled
down its left edge instead, in the colour of what to do about it — amber for the
two that need something done, inert for `ALREADY HERE`, which is the app working
correctly and costs the person nothing.

---

## 4. Strings

Two registers, decided per string rather than by habit, and written into
`strings.xml` rather than transformed at render time — so what is on screen is
greppable and no locale's casing rules are in the loop.

* **Capitals** for chrome, controls and telemetry.
* **Sentence case, at full length**, for anything the app has to explain. The
  load-bearing ones are unchanged: `foreground_only_note`, `recall_from_cache`,
  `standing_actions_blocked`, `pairings_forget_confirm`, `pairings_clear_confirm`,
  `settings_label_note`, `settings_absent_note`, the three offer refusals and the
  two camera failures.

Three splits worth knowing about:

| Was | Now | Why |
|---|---|---|
| `entry_undecryptable` — one sentence | `entry_undecryptable_marker` (`⊘ UNDECRYPTABLE`) + `entry_undecryptable` (the sentence) | The row draws a marker and an explanation |
| `pending_count` — `%1$d Entries are waiting…` | `pending_count` with no count, beside a 34sp readout | The number is what is being reported; the sentence is what it means |
| — | `pairings_pending` — `%1$d ENTRIES WAITING FOR THE RELAY · …` | A card has no room for a readout and has to carry its own count |

`offer_button` and `recall_latest_button` keep their long titles: those are the
**notification's** action labels, and a notification action has no surrounding
screen to say what it acts on. The bar uses `offer_bar` / `recall_latest_bar`.

The label register runs slightly past what the mock draws. It names four
outcomes (`OFFERED`, `RECALLED`, `STALE`, `NOT PAIRED`) plus the three refusals
and `REFUSED · FLAGGED SENSITIVE`; the app also has notices for a cleared
History, a forgotten Pairing and a failure, and leaving those three unlabelled
beside seven labelled ones would have been the inconsistency the labels exist to
remove. `CLEARED`, `FORGOTTEN`, `DID NOT WORK`, `DID NOT PAIR` and
`NOTHING TO RECALL` are that extension, in the same voice.

---

## 5. Cut, deliberately

* **`QUEUED HERE` rows.** The mock's pending state lists the queued payloads as
  rows. A **Pending** is not an Entry — it carries no relay-assigned id and the
  facade exposes only a count — so drawing rows for them would mean widening the
  facade to make a design read. The band and its readout say the same thing
  truthfully.
* **A running `02:00` countdown** under the viewfinder. Nothing on this phone
  knows when the computer printed the code, so the strip states the Relay's
  120-second slot as a rule: `CODES EXPIRE AFTER 02:00`.
* **The animated scan sweep** in the viewfinder. That rectangle holds a live
  camera preview in the real app; the mock's sweep is a stand-in for it.
* **The pulse** the mock puts on two status lights (`RELAY REFUSED THIS PHONE`
  and `CHECKING FOR NEW ENTRIES…`). Declined rather than missed: an infinite
  Compose animation never lets a test's `waitForIdle` return, and 116
  instrumented tests wait on exactly that. The desktop's pulse is CSS behind a
  `prefers-reduced-motion` gate and pays no such price. Re-opening this means
  driving the pulse off `withInfiniteAnimationFrameNanos` under a
  reduced-motion check, and proving the suite still settles.
* **A separate 96dp `RECALL` for the emphasised row and a 48dp glyph for the
  rest** is *not* cut — see §3 — but the mock's row-1 `box-shadow: glow-inset`
  is. Compose has no inner shadow, and a fake one costs a layer on every frame
  of a scrolling list to say what the rule and the tint already say.
* **The mock's status bar, home indicator and lock screen.** Those are the
  platform drawing itself.

---

## 6. Test impact

Every tag survived. What changed:

* `ContactReadoutTest` — the note is clipped until asked, so the test now asserts
  the pinned line, presses `TAG_FOREGROUND_WHY`, and asserts the verbatim
  sentence. Stronger than before: the fact is reachable from any scroll position.
* `HistoryListTest` — asserts the pending sentence *and* its readout, and gains
  `a_readable_entry_is_deleted_by_a_swipe_and_not_by_a_tap`, which presses where
  the delete panel sits (nothing happens) and then swipes (it is asked for).
* `PairingsScreenTest`, `PendingOnANonActivePairingTest` — the card's queue reads
  `pairings_pending`.
* `PairingMessagesTest` — the failure sits under the button that failed, at the
  foot of a flow taller than a phone, so the two failure tests scroll to it.
* `StatusLight` and `QuotedNote` merge their semantics: one node, one utterance,
  and `assertTextEquals` gets a sentence instead of a list of children.

Two accessibility changes came out of review rather than out of the mock, and
neither has a test yet. Every glyph control — Recall on a row, the Pairings
door, the Undecryptable `✕`, the back arrow — now carries a real
`contentDescription` and `Role.Button`, with the glyph's own semantics cleared,
so a screen reader reads "Recall" rather than "↓". And the background-policy
band is the target rather than the `WHY ▸` chip inside it, because a 22dp chip
is half of `Fui.TargetSmall` and the band's whole argument rests on that tap
landing.

---

## 7. Verification

1. `:app:compileDebugKotlin`, `:app:testDebugUnitTest` — clean, no warnings.
2. `:app:connectedDebugAndroidTest` on `spike35` against the `docker compose`
   Relay: **116 tests, 0 failed** (11 skipped — `StandingActionsOnAClosedPhoneTest`
   is the host-driven sequence and skips unless `-e closedPhone true`).
3. Driven by hand on the emulator, against that Relay: the pairing flow with the
   camera refused, the History paired and in contact, `WHY ▸` open and closed,
   the newest row drawn as the emitter's while the rest keep their glyph, a
   swipe that deletes (and a short swipe that correctly does not), a Recall
   Latest that reported `RECALLED`, a duplicate Offer that reported
   `ALREADY HERE` under an inert rule, and the Pairings screen with its card,
   badges, cipher line and three N/A chips.
4. Reviewed on two axes by sub-agents (standards, spec). Nine findings taken,
   including two the emulator alone would not have shown: `#7FF3FF` on
   `#E04B41` measures 3.1:1 and every solid fill now takes the void as its ink,
   and the `WHY ▸` chip was a 22dp target inside a 30dp band. Two findings were
   declined, and both are in §5.

## 8. Risks

* **The swipe is undiscoverable.** Nothing on a row says a Delete exists. That is
  the trade the guard buys, and an Undecryptable row — the one a person most
  wants gone — keeps its `✕` inline for exactly that reason.
* **The chrome can crowd the list.** With a notice, a divergence band, a pending
  band and the blocked-notification note all up at once, the `LazyColumn` gets
  the remainder of the screen. Every one of those is transient except the last,
  which appears only when notifications are switched off.
* **`Fui.kt` and `styles.css` are two copies of one palette.** Nothing checks
  that they agree. The ratios are in comments on both sides; a token changed on
  one client and not the other is a silent divergence.
