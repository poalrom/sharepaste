# An entry exists before the relay names it

> **Renumbered from 0013, 2026-08-03.** Two efforts allocated 0013 the same day and
> [the Filter's](0013-the-filter-is-a-text-field-first.md) landed first, so this one took the
> next free number instead — which is why 0014 and 0015, decided in the same session as this,
> cite a decision numbered after them. Nothing about the decision changed.

An offline capture never reached the local history: `enqueue_capture` encrypts and queues
(`facade.rs:1153-1175`) and only the uploader's ack inserts a row (`uploader.rs:162-165`), so a
device that has just copied three things shows none of them, cannot **Filter** for them, and
cannot **Recall** them — while `capture_or_use` answers *Recognised* to the very next copy of the
same text, proving it holds it (`facade.rs:1118-1124`). The device was withholding its own
content from itself. So an **Entry** exists from the moment of capture, identified by an id this
device assigns; the relay's id becomes a column the core keeps to itself, and what it confers is
not existence but reach.

## Consequences

**The shells stop knowing the relay's ids.** Today the `id` on the wire *is* the relay's, and
every shell holds it — the row key, `delete_entry`, the popover's keyboard cursor. It becomes a
local id, stable from capture until deletion, and the core does the translating. This is a
*narrowing* of the facade: `docs/android-redesign.md:322-327` cut pending rows because drawing
them "would mean widening the facade to make a design read," and the shape that makes them
drawable turns out to remove a relay concept from the shells rather than add one.

**Delete withdraws.** A capture the relay has not taken can be deleted with the same verb and the
same id, and the queued act goes with it. Today there is no way to stop a mistaken copy reaching
the relay — the queue is durable across a force-quit (`.scratch/mobile-client/issues/06:246-266`
proved it) — so making the row visible without making it withdrawable would have shown you the
mistake and handed you nothing. It also obliges a fix at the flush boundary: `flush_once` releases
the database lock across the upload await (`uploader.rs:137-155`), so a delete inside that window
must be detected by `ack` affecting no rows, and the entry deleted on the relay instead of cached.

**`clear_history` has to clear the queue.** It is relay-first and touches only `entries_cache`
(`facade.rs:988-998`), so a queue left standing would repopulate what was just cleared.

**An entry can exist that no other device will ever see.** Every way a pending is lost is now a
row that vanishes rather than a number that ticks down, and a row vanishing reads as *it synced*.
That is the strongest argument against this decision and the reason
[ADR 0015](0015-unreachable-is-not-refused.md) exists; it is a debt this one incurs.

**Two written decisions are overturned deliberately.** `docs/android-redesign.md:322-327` cut
`QUEUED HERE` rows, and `.scratch/filter/spec.md:185-187` states that pendings "surface as a count
in a band, never as rows." Both were right about *pending acts*, which still never appear as rows.
What changed is that the payload of a pending capture was never a pending — it was an entry being
withheld. Anyone finding those lines should know they were reversed, not forgotten.

**The glossary moved under this.** **Entry** loses the relay-assigned id from its
definition-of-existence and **Pending** stops claiming a pending capture is not an entry. The
phrase *pending entry* stays on the _Avoid_ line, for a new reason: the **Pending** is the act,
the **Entry** is the payload, and the phrase fuses two things this decision separates.

## Considered Options

**Keep the count and add an outbox inspector.** Show the queue as a queue, beside the history.
Rejected: it answers "what is stuck" and leaves every original defect standing — the **Filter**
still cannot find your own offline copy and **Recall** still cannot hand it back, because those
operate on the history and the payload still is not in it. A second list to look at is not the
same as the first list being true.

**A union view: pendings as a second row kind in the history list.** Rejected as the expensive
version of the wrong idea. A pending capture stores `ciphertext` and `plaintext_sha256` and no
plaintext (`migrations.rs:38-49`), so a row for one has nothing to preview and nothing to recall;
its `rowid` is not stable, because `requeue_to_back` deletes and re-inserts (`pending.rs:180-194`);
and a pending *use* is an act on an entry already in the list, so it would render the same text
twice — the duplication [ADR 0012](0012-a-repeat-copy-is-a-use.md) exists to prevent. Every one
of those is a symptom of modelling the act as if it were the payload.

**Negative local ids, rewritten to the relay's at ack.** No schema change, and `AUTOINCREMENT`
means the namespaces cannot collide. Rejected: a primary key that changes mid-flight remounts the
row in all three shells — selection lost, scroll position at risk, the popover's keyboard cursor
jumping while it is being aimed at, and the main window's reader showing plaintext under an id
that just went stale. The `entries_cache` rebuild this decision needs instead is the pattern
`REBUILD_PENDING_UPLOADS` (`migrations.rs:96-115`) already establishes.
