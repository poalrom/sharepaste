# Pairing, not Account — in the code as well as the copy

`CONTEXT.md` has always defined **Pairing** as the local record binding this
machine to one user on one relay, and has always listed *account* under its
_Avoid_ line. Every line of shipped code said the opposite: `Account`,
`list_accounts`, `forget_account`, `set_active_account`, `AccountsSection`,
`useAccountsStore`, the tray item `open_accounts`, the route
`main.html?section=accounts`. We renamed the code to match the glossary rather
than amend the glossary to match the code.

## Considered Options

**Rename the user-visible strings only**, leaving `Account` as the internal
type. Rejected: it installs a permanent translation layer, and every future
reader has to learn that `AccountsSection` renders Pairings — which is the exact
confusion the glossary exists to prevent.

**Amend the glossary and let *Account* win.** This was the honest option — it is
what shipped, and it costs nothing. It was rejected on one concrete case. A
machine can hold two pairings to the **same user** on **different relays**:
`alice` on the production relay and `alice` on a lab instance. Those are one
account and two pairings. With only the word *account* available, the design
mock for this window had to invent a composite label, `ALICE / LAB`, that no
part of the system produces — a hand-written string papering over a missing
word. The distinction is load-bearing, and it had already caught a real bug:
`AccountsSection` once headed each row with `label`, the **Device Label**, so
every pairing rendered as an account named "Laptop".

## Consequences

Rust: `list_pairings`, `forget_pairing`, `set_active_pairing`, `PairingSummary`,
and the events `pairing-added` / `pairing-removed` / `active-pairing-changed`.
`AccountRegistry` became `PairingRegistry` and moved from `core::account` — a
module that then had nothing left in it — into `core::pairing::registry`, beside
the invite, payload and shortcode code it is the counterpart to.
TypeScript: `Pairing`, `usePairingsStore`, `PairingsSection`. The route value and
tray id move to `pairings`. `?section=pairing` survives as a distinct value
meaning *the Pairings pane with the add-flow open*, so the tray's "Pair device…"
keeps working — and it is now the only place the singular appears as a route.

The pairing **flow** and the pairing **pane** could no longer share a name; the
flow component becomes `PairingFlow`. That collision is the price of the rename
and is worth noting, because `PairingFlow` next to `PairingsSection` looks
arbitrary until you know a `PairingSection` used to mean the former.

Two places keep the old word on purpose. The storage layer keeps its `accounts`
table: renaming it means a migration on every paired machine to buy nothing a
reader of the repository cannot get from this record, so `accounts_repo` is the
one deliberate survival. And `keychain.rs` still takes an `account` argument —
that is the *keyring's* vocabulary for an entry name, not ours, and translating
it at our boundary would make our code disagree with the API it calls.
