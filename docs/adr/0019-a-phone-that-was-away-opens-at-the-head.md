# A phone that was away opens at the head

**Decided 2026-08-16.** When the phone is opened and the first **Catch-Up** of that foreground finds
anything, the **History Screen** puts the list at the head of the **History** — a jump, not an
animation. Nothing else about an open moves the **Place**, and once that first Catch-Up is spent
nothing moves it again except a **Use** this phone made. The **Filter** now survives an open; the
**Viewed Pairing** still does not.

The phone already had a rule that read as if it covered this. `TheNewHeadStaysInView`
(`ui/HistoryScreen.kt:205-210`) follows the head whenever `SharepasteViewModel.headMoves` fires, and
its doc comment names the two causes it fires on: a new **Entry** on the Viewed Pairing, and a Use
this device made. It has never once fired at an open. `EntryAdded` has exactly one session-side site
and it is inside the live SSE receive loop (`clients/core/src/sync/session.rs:703`); the Catch-Up
ingests every row under one database guard and announces the whole burst as a single
`HistoryChanged` (`:559`). So Entries that arrive while the phone is closed reach the screen through
`refreshHistory` (`ui/SharepasteViewModel.kt:903-906`), which replaces the list with new rows on top
and leaves the Place exactly where it was. Two regions of the same product disagreed about what an
arrival is, and the shell believed the half that never runs.

The mid-session argument in that same comment stands and is deliberately kept: *"chasing it would
cost the reader their place to show them a row they already had"*. At an open there is no reader to
cost. Nobody was looking at the list, nothing was under their eyes, and the thing they opened the
phone to find out is precisely whether anything happened. That is the whole of why the same motion
is right at one moment and wrong at the other, and it is why this is a rule about the open rather
than a widening of the arrival rule.

## Consequences

**A remote Use now moves the list, and only at an open.** The Catch-Up cannot tell an arrival from
a Use: it announces on `advanced`, which is the watermark moving (`sync/session.rs:553`), and a Use
comes back as the row it always was with a fresh sequence. Distinguishing them was on the table and
lost, so a laptop merely recalling an old link while the phone was away is enough to put the phone
at the head. Accepted rather than mitigated: at an open the two are indistinguishable to the person
as well — either way the top row is not the one they left. Mid-session the strict rule is unchanged,
and a remote Use still never costs anybody their Place.

**The phone now forgets its Place but keeps its Filter, which is the reverse of the desktop.** The
desktop clears its needle on `setViewedUserId`, on `setMainSection` and on the popover losing focus
(`clients/desktop/ui/src/store/ui.ts:59,63,65`, `views/Popover.tsx:43-46`), and the phone's own
`onLeaveForeground` cleared it for the reason CONTEXT.md gives for the Viewed Pairing. Two costs
follow. A needle now outlives the Pairing it was typed against, so a reopen can land on `0/100` and
`NO MATCHES` over a list nobody filtered here — visible, labelled, one tap from the `✕`. And the
needle's real lifetime becomes *until the process dies*: nothing persists it, so a warm resume keeps
it and a cold start does not, which is a lifetime no person can predict and no test can pin without
killing the process.

**A mid-session arrival is now silent.** ADR 0007 states that there is no "new clipboard item"
notification, ever; the scroll was the only thing that said an Entry had landed. After this, a phone
left open on the table is a phone whose list is quietly stale until the next open or the next scroll
up. No band and no chip is bought to cover it — ADR 0002 charges rent for chrome that only informs,
and ADR 0018 is the standing warning against reaching for a new control as the answer. If it bites,
the answer is a **Notice**, and it will cost rows then.

**The gate is spent by the first Catch-Up that finds something or by the first hand on the list,
whichever comes first, and there is no clock in it.** A Catch-Up twenty minutes into a session can
therefore still move the list — but only for somebody who has not touched it, who has no Place to
lose. This is what keeps the rule alive on the network this product actually runs on: ADR 0007 makes
being out of contact the nominal case, so an open with no signal followed by a late Catch-Up is
common, not an edge, and a gate spent on the first failed attempt would have surrendered exactly the
case this record exists for. The core cannot say which Catch-Up is the open's — every reconnect
re-enters the same `Connecting` → backfill → `Online` sequence (`sync/session.rs:738` → `:588`) — so
the arming is the shell's own, in `onEnterForeground`, and no core change is made.

**Two shipped assertions reverse.** `HistoryListTest.a_new_entry_brings_the_top_of_the_list_back_into_view`
(`:1006-1020`) asserted the mid-session behaviour this removes and becomes its opposite; the
Viewed Pairing case at `:1133-1152`, whose own comment conceded *"that is not ideal and it is not
this effect's business"*, becomes a jump and stops being a non-goal. The local-Use follow
(`:1070-1085`) and the delete case (`:1099-1116`) stand untouched, and so does the rule that a
reorder from typing snaps while a reorder from a Use animates. `.scratch/filter/spec.md` row 19 is
superseded by this record and not by a second spec.

**The head-move signal stops being a bare `Flow<Long>`.** Nothing has ever read the id it carries,
because `animateScrollToItem(0)` ignores it; what the screen now needs to know is *jump or follow*,
which is one bit the emitter has and the collector cannot derive. The two motions are one rule with
a phase, not two rules, and the phase is named at the source — the same discipline that
`HistoryScreen.kt:191-198` demands of the causes.

**Reading panels are left open.** A **Place** is where the viewport is; an expanded row twenty rows
below the head costs nothing and cannot be seen. Closing them would mean hoisting per-row reading
state out of the rows, since `LazyColumn` keys it by Entry id inside its own saveable holder
(`HistoryScreen.kt:565-571`), and the consistency is not worth that machinery.

## Considered Options

**Forget the Place at every open, unconditionally.** The recommendation, and much the smaller change:
no gate, no phase, no bit to carry, and CONTEXT.md's own argument for the Viewed Pairing and the
Filter — *transient view choices, forgotten when the window closes* — covers the Place without
amendment. It would also have made the three shells agree by accident, since the desktop's Main
Window is destroyed on close and rebuilt on open (`src-tauri/src/lib.rs:230-244`) and its popover is
pulled back to the first row by its own selection reset. Rejected on the owner's call. The cost it
avoided is every consequence above; the cost it carried is that a two-second flip to an
authenticator and back loses a place nobody meant to give up.

**Diff the id set around the Catch-Up's refresh to tell an arrival from a Use.** Exact, needs no core
change, and it would have kept the rule honest to the word "arrival". Rejected on the owner's call
as a distinction without a difference at an open, where the person cannot tell the two apart either.
It would also have been one more derived truth to keep true across the two paths that write
`entries`.

**Have the Catch-Up announce `EntryAdded` per row.** Fixes the disagreement at its source and makes
the live path and the Catch-Up uniform, which is the version a future reader will think of first.
Rejected: it re-adds one event per backfilled row, which `clients/core/src/event.rs:143-146` records
both shells having discarded once already, and R1 puts any such change behind
`.scratch/pendings-in-history/` and `.scratch/deepen-the-modules/issues/04`, both queued against the
same files. A shell-side rule needs no protocol argument.

**A "3 new" band or chip instead of motion.** The alternative ADR 0002's record demands be answered:
inform rather than move. Rejected because it costs rows in the nominal case to report something the
list itself says, and because being moved is what was asked for.

**Soften the mid-session case for somebody who looks busy** — an open reading panel, a needle in the
field. Rejected: "busy" would be derived from the screen, which is the proxy that ADR 0011 broke
once already when **Last Use** made *"head changed and the old head is still present"* stop meaning
"an Entry arrived". Causes are named at the source in this file or they rot.

**Clear the Filter at an open whenever there is news.** Would keep the needle for the quiet opens and
show the news on the loud ones. Rejected: the app undoing somebody's typing while they were not
looking is worse than a needle they can see and clear.

**Bound "the first sync" by time.** The obvious way to stop a late Catch-Up moving the list.
Rejected: a magic number where an edge already exists, and the edge — a hand on the list — is the
thing the person actually meant by "if I already moved somewhere".
