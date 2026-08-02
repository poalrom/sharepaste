# Android redesign — record

**Superseded in part, 2026-08-02.** This record is unchanged and still describes what that
effort built. Six of its statements have since been overtaken, and one has come right again:

- **Decision 5** (`RECALL LATEST` *"must never hand over something stale"*), the verb in the
  layout diagram, and the emitter row's caption — the verb is `RECALL FIRST` and takes the
  first row of the displayed list. [ADR 0010](adr/0010-the-phone-recalls-what-you-can-see.md).
- **The chrome budget** — 112dp/82dp becomes 168dp/138dp, and the layout diagram has no Filter
  band in it.
- **The crowding note**, which argues about the height above the list the Filter has now added
  to.
- **The `ALREADY HERE` evidence line** — a repeat copy is a **Use**, not a refusal.
  [ADR 0012](adr/0012-a-repeat-copy-is-a-use.md).
- **Unchanged after all:** *"scrolled to index 0 on a new Entry"*. The code had drifted to
  scrolling on any new head; keying it back on `EntryAdded` is what this line always said.

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
| 6 | **One target per Entry row at rest; the swipe arms a real Delete** | Two word-buttons was 20 targets a screen, destructive beside safe. "At rest" is enforced rather than assumed — §3 |
| 7 | **An Undecryptable row keeps both controls inline** | Recall disabled-not-hidden; Delete is the only thing left to do with it |
| 8 | **Erasures are confirmed inside the card**, never in a dialog | The scope stays on screen while the choice is made |
| 9 | **Three N/A chips** for the settings a phone does not have | A missing switch and an unbuilt screen look identical without them. The phone's one live switch sits under its own `THIS PHONE` heading so the chips are not read as switches nobody finished wiring |
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
│       (the User is … until the Relay’s /me mirror answers)           │
├ 30dp  Contact ─── ▪ IN CONTACT WITH THE RELAY ── scanlines ──────────┤
├ 30dp  policy ──── ⌾ NOTHING ARRIVES WHILE THIS IS CLOSED ── [WHY ▸] ─┤
│       (open: the sentence + four NO-… chips + ▴ CLOSE)               │
│       (▴ CLOSE: gone for good; the 30dp goes to the list)            │
├─ conditional bands, each its own colour rule ────────────────────────┤
│  standing actions off · notice · divergence · pending                │
├─ LazyColumn, 68dp rows, scrolled to index 0 on a new Entry ──────────┤
│  preview (mono 14sp, one line)                              [ ↓ ]    │
│  FROM MBP-14  (only when the Origin is another Device)               │
│  ← swipe arms a 96dp alert panel: ✕ DELETE — a press fires it        │
│  Undecryptable: alert tint, left rule, ⊘ marker, [↓ off] [✕]         │
├ 48dp  verbs ───── [ RECALL LATEST solid 1.6f ] [ OFFER 1f ] ─────────┤
└──────────────────────────────────────────────────────────────────────┘
```

112dp of chrome at first run, and 82dp once the policy band has been
acknowledged. That band is the only piece of the fixed chrome a person can
retire, and it retires permanently rather than per session — which is only
defensible because the sentence itself is not lost with it (§3, §4).

The pairing flow and the Settings Screen share the 52dp `TitleBand`, and that is
now all they share. The pairing flow's 34dp footer is gone whole.
`RELAY MUST BE HTTPS` was inert on a phone: the scheme arrives inside the
pairing code and is never chosen here, so the line stated a rule about a choice
the reader does not make. `XCHACHA20-POLY1305` went with the band that carried
it.

**The cipher half of that is a real loss and is recorded as one.** ADR 0002 cut
the popover's cipher badge as decoration that resembles information, and held
that if the cipher is ever disclosed it belongs beside pairing — at the one
moment a Relay is being trusted. It is no longer disclosed there. On the phone
it survives only on the Settings Screen's Pairing card, where it describes a
Relay already trusted rather than one being trusted. That is a weaker placement
than ADR 0002 asked for, and it is what deleting the band cost.

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
choice is `rememberSaveable`, not a field on `UiState`: expanding is exploration
and changes nothing about the phone.

Dismissal is the opposite kind of act, so it is owned in the opposite place.
`▴ CLOSE` is an acknowledgement — it goes out to the preference store and the
band does not come back, across a force-stop. That is only allowable because
nothing is lost with it: the sentence reads at full length on the Settings
Screen, under the heading that says what this phone *is*. And expanding
deliberately does not dismiss, because §6 made the whole 30dp band the tap
target: one tap fewer is not worth a stray tap silently retiring the app's most
important disclosure.

**The verb bar.** Two equal outlines were truthful about their symmetry in the
code and mute about which one a person reaches for — and left the screen without
a single emitter. Recall Latest is now the solid one and comes first.

**The Entry row.** Preview, and an Origin line only when the Entry came from
another Device. One 48dp Recall target. Delete is a left swipe onto a 96dp alert
panel, which is the guard the desktop's unguarded `✕` never had: a delete fans
out over SSE to every paired device and cannot be undone.

**The swipe asks, and springs back — and the panel it uncovers now answers.**
The panel is the delete rather than a picture of one: a press on it fires the
same `onDelete` a completed drag fires, so dragging all the way and dragging
then tapping are one outcome instead of two behaviours to learn. Drawing a
control that did nothing was the worst available reading of the gesture — it
taught the swipe and then refused the obvious next move, to somebody who had
that instant discovered it. The row still springs back either way, because the
list is the source of truth: a delete the Relay refuses must not hide a row that
is still there.

"One target at rest" survives that, and it is enforced rather than assumed.
`SwipeToDismissBox` composes the background under the row on **every** frame,
and an opaque colour is not a pointer target — a press where the row holds no
button falls straight through to the panel behind it. So the panel is a control
exactly while `swipe.dismissDirection == EndToStart`. Without that condition
every row would carry an undoable Delete under most of its width: the two
targets a thumb apart that decision 6 exists to remove, only now one of them
invisible. The arming *is* the guard.

**The newest row is drawn as the emitter's**: a 2dp cyan rule, the emitter's own
tint, a brighter Preview and a filled 96dp `RECALL` in place of the glyph. That
is the row `RECALL LATEST` will hand over, and the bar's whole argument — that
Recall is the verb that must never hand over something stale — needs the list to
say which row it means.

**The Pairing card.** Headed by the **User**, never by this machine's Device
Label — the desktop's mistake, and the card is where it was made. The subtitle
under that heading is the relay host alone. The `user_id` used to lead it, and
the argument for keeping it is that it is the only strictly unique thing here:
two Pairings can share a username. It goes anyway, because the subtitle is not
the disambiguator — the `ConfirmStrip` is, and a choice with no way back is
where naming the User *and* the Relay earns its space. On the card the uuid
bought nothing and cost the host, which ellipsised away behind it. The Device
Label is a line inside the card. Exactly one card carries `SYNCING`;
`SHOWING` moves independently. An erase is armed inside the card with a
`CANNOT BE UNDONE` badge, the sentence naming the User *and* the Relay, and
`KEEP IT` / the destructive verb — the destructive one solid, because the person
already asked for it and the strip exists to make them read what it costs.

**Settings.** The screen is titled `SETTINGS`, and the Pairings are a section of
it. `PAIRINGS` was honest while a Pairing was the only thing here; the moment
the screen grew a preference of its own, a title naming one of its four sections
sent anyone looking for a switch to a screen that does not exist. `Screen.Pairings`
and the `pairings_` string keys stay as they are: this is an information
architecture change, not a routing one, and renaming a symbol nobody reads so it
agrees with a title is churn dressed as tidiness.

Four sections, in order: the Pairing cards, `ADD ANOTHER PAIRING`, `THIS PHONE`,
`ABOUT THIS PHONE`. `THIS PHONE` holds the only thing this phone can be told —
*show what was recalled* (ADR 0009) — and holds it alone. That separation is
decision 9 still doing its work: a live switch three lines above
`WATCHED CAPTURE · N/A` makes the chips read as switches somebody stopped wiring
up, which is the exact misreading the chips exist to prevent. Kept apart, under
a heading about what the phone *is* rather than about what it can do,
`WATCHED CAPTURE · N/A`, `DENY-LIST · N/A` and `UPDATE CHECK · NONE` still read
as absences with reasons. The third is the one the desktop cannot show: a phone
carries no update code at all (ADR 0008), so the Relay is its only counterparty.

`ABOUT THIS PHONE` still carries two quoted notes at full length, but not the
same two. The Device Label note is gone and the foreground-only sentence has its
place — a fact the History Screen lets a person retire for good needs exactly
one surface where they cannot (§4).

**Receipts and Notices.** What the app says after a verb is split by **outcome
kind**, and the two idioms are the distinction rather than an inconsistency.

A **Receipt** confirms that a verb did what was asked and needs nothing back, so
it does not wait: a Toast, the label over the sentence, `LENGTH_LONG` because a
Preview is a line of text somebody has to read rather than a tick to glance at.
A **Notice** says something needs doing or knowing, so it takes the band and
stays until it is dismissed. `Offered`, `Recognised` and `Recalled` are
Receipts. Six outcomes keep the band — `OfferRefused`, `RecalledFromCache`,
`Unpaired`, `HistoryCleared`, `PairingForgotten`, `Failed` — and each still
carries a label naming the outcome: `NOT PAIRED`, `MAY BE STALE`, `CLEARED`,
`FORGOTTEN`, `DID NOT WORK`, and one per reachable refusal (`NOTHING TO SEND`,
`TOO BIG · 64 KB CAP`).

The argument that used to sit here was about the **invocation path**: a Standing
Action and an in-app press are the same operation, and reporting it in two
idioms would make it look like two. That is still true, and it is now enforced
by the shape instead of by matching labels. `standing/Said.kt` is deleted; both
paths build one `Receipt` through one function, so "a Recall from the
notification and a Recall from the row produce an identical Receipt" is a fact
about one function rather than a claim about two. What the two idioms
distinguish is no longer *who called* but *what kind of thing happened*.

`RecalledFromCache` is the variant that proves the line is real. It looks
exactly like a confirmation — a Recall, done — and it is not one, because
ADR 0007 says it may never be silent. One outcome type rendered two ways would
have permitted it into something that vanishes while unread; two types make that
unrepresentable. It also stays the one Notice that tints its whole band: it is
the only one about *what is now on the clipboard* rather than about what the app
just did. A refusal is ruled down its left edge instead, in the colour of what
to do about it — amber, because both refusals an Offer can still receive need
something done about them. The one that did not, `ALREADY HERE`, is no longer a
refusal at all: under [ADR 0012](adr/0012-a-repeat-copy-is-a-use.md) a repeat
copy is a **Use** of the Entry the phone already holds, and it draws the
`ALREADY SAVED` Receipt rather than a Notice.

**Only the Recall Receipt names an Entry**, and only while the `THIS PHONE`
switch allows it. The Offer Receipt does not: the person supplied that content a
second ago, and only Recall hands back something they did not choose (ADR 0009).
**The log line names none**, on either path. `receiptLogged` is a separate,
preview-free sentence from the one the Toast draws, because a Toast is transient
and aimed at the person who just pressed the control, while a log is durable and
readable by anything holding `READ_LOGS` or a cable. That asymmetry is a
contract with `StandingActionsNotificationTest` and with the acceptance
sequence.

---

## 4. Strings

Two registers, decided per string rather than by habit, and written into
`strings.xml` rather than transformed at render time — so what is on screen is
greppable and no locale's casing rules are in the loop.

* **Capitals** for chrome, controls and telemetry.
* **Sentence case, at full length**, for anything the app has to explain. The
  load-bearing ones are unchanged: `foreground_only_note`, `recall_from_cache`,
  `pairings_forget_confirm`, `pairings_clear_confirm`, `settings_absent_note`,
  the three offer refusals and the two camera failures.

Two strings left that list, and neither for the sake of being shorter.

`standing_actions_blocked` was rewritten to lead with what is gained rather than
with what is missing, and its heading went from `STANDING ACTIONS · UNREACHABLE`
to `STANDING ACTIONS · OFF`. The band appears because a person switched
notifications off, which is a choice they made; naming their choice a fault is
both wrong and unpersuasive. `OFF` is what is true, and the sentence now says
what allowing them would buy instead of what refusing them has broken.

`settings_label_note` is gone from the Settings Screen entirely. It carried one
load-bearing fact — the Relay names a device once at `POST /devices` and has no
rename route — and it carried it on the screen that only displays the result.
That fact now sits inside `pair_label_explainer`, beside the field where a name
is still being chosen and can still be chosen differently. A rule stated where
it can be acted on beats the same rule stated where it cannot, and the slot it
vacated went to the foreground-only sentence, which is the one that must never
become unreachable.

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
`NOTHING TO RECALL` are that extension, in the same voice. The register itself
survived the Receipt/Notice split intact and now spans both idioms: `OFFERED`
and `RECALLED` head a Toast, the rest head a band, and a label means the same
thing either way — the outcome, in a word, over the sentence that explains it.

---

## 5. Cut, deliberately

* **`QUEUED HERE` rows.** The mock's pending state lists the queued payloads as
  rows. A **Pending** is not an Entry — it carries no relay-assigned id and the
  facade exposes only a count — so drawing rows for them would mean widening the
  facade to make a design read. The band and its readout say the same thing
  truthfully.
* **A running `02:00` countdown** under the viewfinder — and, in the end, the
  strip that replaced it. The reasoning was right the first time and simply had
  further to go. This phone is the claimer: it reads a shortcode carrying no
  timestamp, and nothing in the protocol tells it when the computer printed one,
  so a clock here would be invented. What went up instead was
  `CODES EXPIRE AFTER 02:00` — a rule rather than a countdown. But a rule is
  precisely what a person cannot act on while pointing a camera at a square, and
  the fact is already delivered where they can act on it, in the `DID NOT PAIR`
  sentence at the moment a code has actually gone stale. So the strip is cut
  too. Restoring either means widening the QR payload or the wire protocol to
  carry `expires_at` to the claimer, and would still leave a typed code without
  one.
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
* `HistoryListTest` — asserts the pending sentence *and* its readout, and covers
  the swipe in two cases rather than one, because there are now two routes to
  one delete. `the_delete_panel_is_a_control_only_while_the_swipe_holds_it_open`
  presses where the panel sits on an un-swiped row (nothing happens), then holds
  the drag open and presses again (the delete is asked for);
  `a_completed_swipe_still_deletes_the_entry_on_its_own` keeps the drag-all-the-way
  route honest. Two routes, one `onDelete`, and the guard is the arming.
* `PairingsScreenTest`, `PendingOnANonActivePairingTest` — the card's queue reads
  `pairings_pending`.
* `PairingMessagesTest` — the failure sits under the button that failed, at the
  foot of a flow taller than a phone, so the two failure tests scroll to it.
* `StatusLight` and `QuotedNote` merge their semantics: one node, one utterance,
  and `assertTextEquals` gets a sentence instead of a list of children.

Two accessibility changes came out of review rather than out of the mock, and
neither has a test yet. Every glyph control — Recall on a row, the Settings
door, the Undecryptable `✕`, the back arrow — now carries a real
`contentDescription` and `Role.Button`, with the glyph's own semantics cleared,
so a screen reader reads "Recall" rather than "↓". And the background-policy
band is the target rather than the `WHY ▸` chip inside it, because a 22dp chip
is half of `Fui.TargetSmall` and the band's whole argument rests on that tap
landing.

---

## 7. Verification

> §7 records the redesign's own verification pass, on the build §§1–6 describe.
> The `fix-android-ux` branch changed the History Screen, the Settings Screen
> and the pairing flow afterwards and is verified on its own terms; the counts
> and the screen names below are the ones that were true then.

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

* **The swipe is undiscoverable.** Nothing on a row says a Delete exists. **This
  risk survives unchanged.** Making the revealed panel a real button made the
  gesture's reward honest; it did nothing to make the gesture findable, and no
  work since has been aimed at that. It is still the trade the guard buys, and
  an Undecryptable row — the one a person most wants gone — still keeps its `✕`
  inline for exactly that reason.
* **The chrome can crowd the list.** Partly retired, and it is worth being
  precise about which part. Two of the things that could stack up there no
  longer can: a plain Offer and a plain Recall are Receipts and never occupy
  chrome at all, and the foreground-only band can be dismissed for good. A
  Notice, a divergence band, a pending band and the notifications-off note can
  still take the top of the screen together. What changed is that height is no
  longer the only mitigation — the `LazyColumn` scrolls to index 0 when an Entry
  arrives, so the row `RECALL LATEST` will hand over is visible however much
  chrome is above it. What is left of the risk is the rest of the list, not its
  head.
* **`Fui.kt` and `styles.css` are two copies of one palette.** Nothing checks
  that they agree. The ratios are in comments on both sides; a token changed on
  one client and not the other is a silent divergence.
