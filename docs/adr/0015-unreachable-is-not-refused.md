# Unreachable is not refused

The uploader had three ways to lose an act and told only the log about each: `BadInput` acks and
deletes the row (`uploader.rs:203-211`), a use of a vanished entry is dropped (`:151`), and
anything unmapped — including `PairExpired`, since 410 matches none of the three arms — fails at
the head forever, blocking everything behind it, with `attempts` counted and never compared to
anything (`pending.rs:200`). Once a **Pending** is a visible row
([ADR 0013](0013-an-entry-exists-before-the-relay-names-it.md)) each of those becomes a row that
silently disappears or sits amber forever. So a pending the relay turned down *for what it is*
becomes **Refused**: it leaves the queue, so nothing waits behind it, and stays on the device
until it is **Resent** or deleted. Being out of reach is never a refusal — that is what the queue
is for.

## Consequences

**The uploader stops destroying data.** `pending::ack` on `BadInput` with
`tracing::warn!("dropped malformed pending entry")` is the code deleting a person's clipboard
content and telling nobody who could act on it. That is wrong today, independently of any of
this; it has simply been unobservable. A refusal is now a red row with the reason on it.

**Head-of-line blocking is fixed as a side effect, without loosening the order.** A refused act
is never going to be delivered by waiting, so stepping over it removes nothing from the sequence
that was ever going to reach the relay. Everything still queued is still strictly FIFO.

**Only `BadInput` qualifies.** The test is *whose problem is it*. A 400 or a 413
(`http/client.rs:84-85`) is a fact about **this act** — too large, malformed — and will be refused
identically forever. 5xx is a fact about the moment and must stay transient, or a relay restarting
mid-flush would refuse whatever happened to be at the head and shred the ordering. Surviving a
relay that is not there is the entire purpose of the queue.

**`PairExpired` moves to the `Auth` arm.** It is a fact about the **Pairing**, not the act.
Refusing a hundred rows for one lapsed pairing would be a hundred red rows and a hundred buttons
for a single fix that will make all of them work again. It belongs with the failure that brings
the session down and lets the connection chrome say what is wrong (`uploader.rs:198-202`). That it
currently falls through to the generic retry arm is a bug this decision exposes rather than causes.

**`attempts` and `last_error` are read for the first time.** Both have been written on every
failure since the queue shipped and neither has ever reached a shell.

**Resend is a fresh act, not a retry.** It goes to the back of the queue and therefore to the head
of the history ([ADR 0014](0014-the-history-is-the-queue-then-the-relay.md)), carrying nothing
forward from the refusal — the same rule `requeue_to_back` already applies, for the same reason
(`pending.rs:174-178`). It is not a **Use**: there is no relay record to move. If nothing about
the cause has changed it will be refused again, which is honest; the button exists for after you
have changed the relay's limit or the active pairing, not to make the same request louder.

**Rows now have three tones.** Plain, amber for pending, alert-red for refused — and alert-red
already means **Undecryptable** (`EntryRow.tsx:94-110`). A refused capture sealed with a key the
device later lost on re-pairing is both, and has one slot for two claims. Rare, reachable, and
unresolved here.

**The queue keeps no give-up of its own.** A refusal is a verdict the relay delivered, never a
conclusion the client reached by counting. Nothing expires, nothing is discarded on the device's
own authority.

## Considered Options

**Leave the loss paths silent and ship the rows anyway.** Rejected as the one outcome worse than
the count. A number that ticks down when something is destroyed is uninformative; a row that
appears and then disappears is misleading, because the disappearance reads as *it synced*.

**A give-up threshold: refuse after N failed attempts.** It would catch the unmapped errors too.
Rejected because N cannot be justified — any value is a guess about a network — and because it
converts a transient outage of the right length into permanent-looking red rows.

**Treat everything that is not a plain connection failure as a refusal.** Simple rule. Rejected:
a relay restarting returns 5xx, and this would refuse the head of the queue every time one did.

**A full dead-letter queue with its own surface.** Rejected: the only two things a person can do
with a refused act are keep it or discard it, and both verbs already exist on the row.
