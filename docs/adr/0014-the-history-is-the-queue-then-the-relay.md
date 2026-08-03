# The history is the queue, then the relay

With an **Entry** existing before the relay names it
([ADR 0013](0013-an-entry-exists-before-the-relay-names-it.md)), it needs a place in a list whose
only sort key is relay-stamped **Last Use** — a value it does not have and will not have until
flush. The **History** is therefore two regions in one order: this device's pending acts, newest
first, in the order it will send them, above the entries the relay has ordered by last use. No
device clock enters the model.

## Consequences

**Nothing reshuffles at flush.** The queue drains FIFO by `rowid` (`pending.rs:115`) and the relay
stamps each act on arrival, so post-flush `last_use` order *is* rowid order and the display is
`rowid DESC` either way. The rows you were looking at before the relay came back are in the same
places afterwards. This is the property the decision was chosen for, and every other choice here
was made to keep it.

**Back of the queue is the head of the history.** [ADR 0012](0012-a-repeat-copy-is-a-use.md) moves
a re-copied pending to the *back* of the queue while a re-copied entry goes to the *front* of the
history, which reads as a contradiction and is not one: the queue drains oldest-first, so last to
flush is newest. Recalling an un-flushed capture therefore does the same thing — `requeue_to_back`,
no relay id required, no third `PendingKind`. One gesture, one visible result, before and after
flush.

**Uses join captures.** Surfacing only captures would leave the original defect in place for the
case where the text was already held: you perform the gesture the whole list is ordered by and the
list ignores you, because the entry's `last_use` is a relay stamp from before the outage. An entry
with a queued use sorts by its *latest* pending rowid — latest, because
`pending::find_by_hash` matches captures only (`pending.rs:157`), so two offline re-copies of one
entry enqueue two uses and the row must rise on the second.

**Standing Actions stops being stale.** Its contract is "recall the entry at the head of the
history — the last one used, not the last one captured" (`CONTEXT.md`), and offline that head was
whatever preceded the outage. It is now genuinely the most recent act, with no change to the verb.

**Recall Latest drains before it answers.** With the relay reachable and acts still queued, a
local act and a remote entry cannot be compared — one has no stamp. Rather than adjudicate by
fiat in the operation the code calls "the operation that has to be right every single time"
(`facade.rs:867`), the queue is flushed first, after which `(last_use DESC, id DESC)` decides and
every device gives the same answer. The fallback needs no new vocabulary: `RecallSource::Relay`
already means the round trip succeeded, `RecallSource::Cache` that it did not and the caller must
say so (`facade.rs:874-877`). The cost is a verb that gets slower exactly when a queue exists,
which the phone already pays through `drainPending` (`SharepasteRepository.kt:607-668`).

**The list shrinks at flush after a long outage, and that is retention, not a fault.** Un-flushed
rows are never evicted, so 150 offline captures are 150 visible rows and 150 delivered acts; at
flush they become entries and `entries_cache` prunes to its hundred. Online that pruning is
continuous and invisible. Offline it is deferred to one moment. The same rule, batched.

**`MAX_PER_USER` on the queue is deleted**, with `EnqueueResult.dropped_oldest` and
`warn_if_dropped` (`pending.rs:6,90`, `facade.rs:1166`). It was silently destroying undelivered
clipboard content to protect a number, in the one situation where the local copy is the only copy
that exists. One of four silent loss paths, gone by subtraction.

**The un-flushed region is unbounded while offline.** A very long outage grows the local database
with no ceiling, and puts an arbitrary number of rows above a popover designed for ten
([ADR 0002](0002-popover-is-a-picker.md)). `list_history`'s cursor has to page across the seam.
Accepted: the alternative is discarding acts, and rows are cheaper than data.

**The count chrome all survives.** The footers count what the viewport does not show, the pairing
cards remain the only place a switched-away-from pairing's queue is visible
(`PairingsSection.tsx:147`), and the phone's band carries the sentence — *"Sharepaste sends them
while this app is open"* — that is the only statement anywhere of
[ADR 0007](0007-a-phone-only-acts-when-you-act.md)'s foreground-only sync. A tint cannot say that,
and without it amber rows look broken rather than waiting.

**This is not the promoted band ADR 0011 rejected.** That option was "a promoted band above a
capture-ordered history," rejected because "it moves the seam onto all three surfaces" and
"you still have to say whether a promoted entry remains in the history below it"
(`0011:88-91`). Both objections are about a *duplicate* — the same entry in two places. Here there
is one row in one list; the seam is where the relay's knowledge ends, and it is stable across
flush rather than moving with every use. The resemblance is close enough that someone will try to
undo this, which is why it is written down.

## Considered Options

**Give an un-flushed act a provisional local timestamp, overwritten at ack.** It sorts naturally
and needs no second regime. Rejected for the reason `.scratch/last-use/spec.md:90-94` already
gave when refusing the same thing for uses: "it is the first device clock in the model." The
asymmetry named there does dissolve once captures are local too, but two clocks would still meet
in one column and be compared by a sort.

**Captures only, leaving queued uses invisible.** Smaller, and it keeps the sort key change to
rows that have no `last_use` at all. Rejected: it surfaces half of a two-part definition —
**Pending** is "a capture, or a use" — and leaves the list lying in the case
[ADR 0012](0012-a-repeat-copy-is-a-use.md) made most common.

**Cap the display at a hundred and leave the queue at a thousand.** No shrink, no destroyed acts,
two numbers. Rejected on one consequence: an act you cannot see, you cannot withdraw, and
withdrawal is the strongest thing [ADR 0013](0013-an-entry-exists-before-the-relay-names-it.md)
buys. A rule with a footnote about the newest hundred is worse than a list that gets long.

**One cap of a hundred over the merged list, evicting from the bottom.** No shrink, one number.
Rejected because the thing evicted is an undelivered act: the product would discard clipboard
content to protect a display invariant, precisely while the relay is unreachable. Losing rows is a
display event; losing acts is data loss.
