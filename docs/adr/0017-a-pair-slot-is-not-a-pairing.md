# A Pair Slot is not a Pairing

`CONTEXT.md` defines a **Pairing** as the local record binding this machine to
one user on one relay: permanent, one per user per relay, and a thing a person
manages. The relay's `pairings` table is none of those. It is a two-minute,
single-use handshake slot with `claimed_at`, `consumed_at` and `failed_attempts`
that burns after three wrong proofs and is swept once it expires. Two concepts
wear one word, which is the exact fault [ADR 0004](0004-pairing-not-account.md)
found on the client and fixed there. We name the relay's one **Pair Slot**, add
the term to `CONTEXT.md`, and rename the relay's identifiers to match —
`PairingRow`, the `pairings` repository namespace, and the helpers in
`server/src/server/pairing-slot.ts` that already half-admit the distinction in
their own names.

**The SQLite table keeps the name `pairings`.** That asymmetry is deliberate and
is the only part of this record worth reading twice.

## Considered Options

**Rename the table too.** The consistent answer, and what ADR 0004 did on the
client. Rejected on cost against benefit: a table rename is a migration against
every installed self-hosted database, and it is the one rename no reader is paid
back for. Nobody learns this domain from the schema. A person meets
`PairSlotRow` and `pairSlots.claim(...)` while reading a route; they meet
`pairings` only inside a SQL string that the deepened module owns and that
nothing else reads. ADR 0004's argument was that a permanent translation layer
is the cost of leaving the code wrong — but a single table name, owned by one
module, is not a layer.

**Leave the relay its own vocabulary.** Rejected. It is not the relay's own
vocabulary; it is the client's vocabulary used for something else. That is worse
than a synonym, because the two things are both real and both reachable in one
conversation: a Pair Slot becomes a Pairing, and a sentence about "the pairing"
during a handshake is genuinely ambiguous today.

## Consequences

`migrate.ts`, the retention sweep and the `pairings` SQL strings keep the old
name, inside the module that owns them. A future migration that has to touch
that table for some other reason may rename it then, when the migration is being
paid for anyway.

Do not "fix" the mismatch on sight. It is recorded here so that the next reader —
human or agent — can tell a deliberate asymmetry from an oversight, which is the
only reason this record exists.
