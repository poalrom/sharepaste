# The history is ordered by last use

The history was ordered by relay-assigned entry id — capture order — and a **Recall** left no
trace anywhere: it read a cached plaintext, wrote the clipboard, and told nobody. Recalling an
entry now makes it the head of the history on **every** device. The relay records a **Last Use**
per entry, orders and prunes by it, and fans the change out as an ordinary entry row. The entry
keeps its id, its `created_at` and its **Origin**; nothing is created and nothing is duplicated.

## Consequences

**The relay learns which entries you use.** Reads are bulk — `GET /entries?since=` hands back
everything above a watermark — so until now the relay knew what you captured and when, and
nothing whatever about which entry you ever *used*. A use is a write, so it now holds a
per-entry access log, and repeated uses single out the handful of entries you reuse constantly,
which are the credential-shaped ones. This is not an oversight to be closed later: ordering that
agrees across devices means the shared party decides the order. What the record does **not**
carry is the device — the relay can observe that from the authed request either way, so storing
and fanning it out would only add a rival to **Origin** on the entry, against
[ADR 0001](0001-device-metadata-out-of-band.md)'s grain, for no one's benefit. Floor kept low
per **R2**.

**Ciphertext you keep recalling is never aged out.** Both caps — 100 entries and 30 days
(`serve.ts:31-32`, mirrored in `entries_cache.rs:24-25`) — now measure from Last Use rather than
from capture. An entry in weekly use lives on the relay indefinitely. That is the point rather
than a side effect: eviction and ordering reading different facts is the version where the count
cap deletes the row sitting at the top of the list, because a frequently recalled entry is by
definition an old capture with a low id.

**The watermark stops being an entry id.** `last_seen_id` means "everything up to here has been
fetched" (`session.rs:22`) and never regresses (`:568`). A bumped entry has to come back down
that same pipe, so an entry carries a relay-assigned sequence that is re-set on use, and the
fetch filters on the sequence rather than the id. The whole row is re-sent, ciphertext included
— bounded by `maxEntryBytes` at 64 KB, and paid only when something is used. SSE needs no new
frame: the existing `entry` frame is republished, and the cache's upsert on `(user_id, id)`
lands it in place.

**The row's timestamp changes meaning.** `relativeAge(entry.created_at)` on the list row
(`EntryRow.tsx:125`, `HistorySection.tsx:242`) becomes the age of the last use, so the age column
descends again instead of showing a three-week-old entry above a two-minute-old one. The detail
pane keeps saying `CAPTURED` (`EntryDetail.tsx:115`), truthfully, and gains a use line when the
two differ. **Origin** is untouched.

**Recall Latest follows the head.** The Standing Action takes the top of the history, which is
now the entry last *used* rather than the one last *captured* — so recalling an old snippet on
the phone changes what the notification's action will hand you afterwards. The verb is invoked
blind, and what makes that safe is already built:
[ADR 0009](0009-the-phone-shows-what-it-recalled.md)'s **Receipt** names the **Preview**, so the
phone says which entry it gave you. Recalling the head is still recorded, even though the order
cannot move — it renews tenure, and an exception here would let a daily-recalled entry age out
at thirty days. The phone's *in-app* verb is unaffected:
[ADR 0010](0010-the-phone-recalls-what-you-can-see.md) made it `RECALL FIRST`, taking the first
displayed row from cache, which follows this ordering without knowing about it. The
notification's verb keeps the name `RECALL LATEST` deliberately, even though "latest" now means
last *used*: `RECALL FIRST` would name a row nobody can see from a notification, and a third
name for a third meaning of the same act costs more than a word whose sense has shifted on a
surface that displays nothing.

**A use that cannot be sent is pending.** Uses queue beside captures and reach the relay in the
order they were made, so an outage cannot reorder what happened during it. The relay stamps them
on arrival, exactly as it already stamps a pending capture, which means a batch flushed after an
outage lands above what other devices did online during it. That distortion is the existing
behaviour of **Pending**, not a new one.

**Reading still changes nothing.** [ADR 0003](0003-the-main-window-reads.md) stands: opening an
entry in the main window leaves its place alone. Only a use moves it.

**Relay skew is not handled.** [ADR 0001](0001-device-metadata-out-of-band.md) latches a missing
route off for the session because a self-hosted relay can be older than its clients. This
decision does not: the relay is updated first, and a use write to a relay without the route
fails like any other failed write. The recall itself still succeeds — it never needed the relay.

## Considered Options

**Move the row: delete it and re-insert the same ciphertext under a fresh id.** Free on the
existing protocol, and the tempting one — a new id is above every device's watermark, so every
device pulls it with no client change at all. Rejected because identity churns on every recall
and because a device that was offline when the delete was published never learns of it: the
`since=` fetch cannot reveal a deletion, which `facade.rs:789` already documents as a known gap.
It would turn a rare hole into a permanent duplicate row after every single recall.

**A promoted band above a capture-ordered history.** Keeps **History** meaning exactly what it
meant. Rejected: it moves the seam onto all three surfaces, and the popover is one line per
entry by design ([ADR 0002](0002-popover-is-a-picker.md)). It also only defers the question —
you still have to say whether a promoted entry remains in the history below it.

**A second feed for uses**, `GET /uses?since=`, carrying `(entry_id, last_use)` pairs instead of
whole rows. Cheaper on the wire. Rejected: two watermarks can sit at different points, so a
device can hold exactly the right entries in the wrong order with no way to detect it. Today,
"the backfill succeeded" means the device is correct, and that property is worth more than the
bytes.

**Per-device ordering**, with no use ever reaching the relay. It concedes nothing to the relay
at all. Rejected: a list that disagrees between the phone and the laptop is not the thing that
was asked for.
