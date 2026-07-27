# Device metadata travels out of band, not on entries

The popover shows which device an entry came from and, when several users are
paired on one machine, which user's history is on screen — neither of which the
relay sends today. We add a single authed `GET /me` returning
`{ user: { id, username }, devices: [{ device_id, label, created_at, revoked_at }] }`,
which each device mirrors into a local `devices` table and joins against
entries at read time.

## Considered Options

Denormalising `device_label` onto `GET /entries` rows and the SSE `entry` frame
was the obvious alternative and needs no client-side storage at all. We rejected
it because a device label is a property of a **device**, not of an **entry**:
copied onto entries it duplicates a mutable 128-char string across every cached
row and live frame, and it freezes — renaming a device would leave the old label
scattered through history with no way to correct it short of a cache rewrite. It
would also have left `username` needing a second route regardless.

## Consequences

The relay's entry path — `POST /entries`, `GET /entries`, the SSE frame — is
untouched, which is where sync bugs are most expensive. A renamed device reads
by its stale label until the next reconnect refreshes the mirror; an entry from
a device id the mirror has never seen triggers one debounced refresh. Legacy
memberships may carry `device_label = NULL`, which the UI renders as a short
device-id slice. `GET /me` is also the endpoint the Main Window needs to list a
user's devices, which it cannot do today.

A separate route also means a **client can be newer than the relay it is paired
to** — self-hosted deployments skew by definition. A relay without `/me` answers
404, which is permanent rather than transient, so the desktop latches the route
off for the session after saying why once, and re-probes on the next reconnect
(redeploying the relay is what drops the stream, so that is exactly when the
route may have appeared). Entries keep syncing throughout; Origins fall back to
a short device-id slice and the username stays unmirrored.
