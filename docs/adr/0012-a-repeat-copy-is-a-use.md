# A repeat copy is a use, not a new entry

With the history ordered by **Last Use**
([ADR 0011](0011-the-history-is-ordered-by-last-use.md)), copying text that is already in the
history would produce two identical rows — one at the top, one buried — in a list that now
claims to be ordered by use. So a capture whose text the device already holds is not a capture:
it is a **use** of the entry that is already there. The relay cannot make that judgement —
`encrypt` draws a fresh nonce on every call (`crypto.rs:19`), so the same plaintext never
produces the same ciphertext — so the device makes it, against the plaintext in its own cache.

## Consequences

**The same copy dedupes on one device and duplicates on another.** A device can only match
against plaintext it holds: at most 100 entries, and never an **Undecryptable** one. A phone
paired last week has a shorter memory than the laptop that has been paired for a year, so the
identical action produces an entry on one and a use on the other, with nothing on either screen
explaining the difference. Accepted as the price of matching at all. It is the one part of this
that a person can catch being inconsistent, and the honest defence is that the alternative is
duplicate rows everywhere rather than duplicate rows sometimes.

**The match is exact bytes.** No trimming and no normalisation, so the same URL copied with and
without a trailing newline is two entries. Trimming would recognise more, and it would mean a
**Recall** hands back the *stored* variant rather than the one just copied — invisible, and in a
shell the difference between a command that runs and one that waits. A duplicate row is visible
and explains itself; a substituted variant does neither.

**Capture is conditional now.** "The act of a device turning local content into an entry"
acquires an exception, and the offer path grows an outcome in which nothing was queued —
`OfferOutcome` can no longer assume it produced something.

**The phone says which of the two happened.** A recognised **Offered Capture** draws its own
**Receipt** — *already saved, moved to the top* — because reusing the ordinary one would claim
content was saved when nothing was, on a list the person can immediately check. **Watched
Capture** stays silent, as it is today; it has no receipt and should not grow one.

**The consecutive-duplicate filter goes.** `last_capture` (`facade.rs:195`) existed to drop a
repeat before it cost an encrypt and an upload. Content matching covers every case it covered,
and leaving both in place would mean two mechanisms with different answers to the question "what
is a repeat copy" — one dropping it silently, the other recording a use.

**Matching includes pendings, not just entries.** A **Pending** has no relay id and so cannot be
used. But matching only against entries would mean copying the same text twice while offline
makes two pending captures — precisely the duplicate this decision exists to prevent.
Re-copying something still pending moves that pending to the back of the queue instead, which is
the rule the ordered flush already follows.

## Considered Options

**Leave it out of scope: a repeat copy stays a new entry, as today.** The smaller change, and it
keeps **Capture** unconditional. Rejected: the list now advertises itself as ordered by use, and
two identical rows in it read as the feature half-working. The behaviour would have to be
written down as a non-goal, which is a poor substitute for it not being surprising.

**Dedupe on the relay.** Rejected as impossible rather than unwanted. The relay holds
randomly-nonced ciphertext and cannot tell two copies of one text apart; anyone proposing it
again should be pointed at `crypto.rs:19`.

**Make encryption deterministic so the relay can match.** It would work, and it would leak the
equality of plaintexts across the entire history to the relay — strictly more disclosure than
the access log ADR 0011 already concedes, and unlike that one it would be inferable forever from
data already stored. Not seriously entertained; recorded so it is not re-proposed as the obvious
fix for the non-uniformity above.
