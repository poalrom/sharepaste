# Sharepaste

End-to-end-encrypted clipboard sync between a person's own machines. A
self-hosted relay stores and fans out ciphertext it can never read; each
machine decrypts locally with a key the relay never sees.

## Language

### Identity

**User**:
The identity that owns a clipboard history. Named by a `username`, keyed by an
opaque `user_id`.
_Avoid_: account, owner, customer

**Device**:
One paired machine belonging to a user — a desktop, a phone or a tablet. Every
entry records the device it came from.
_Avoid_: client, install, node

**Device Label**:
A device's human-readable name, chosen when that device is paired.
_Avoid_: device name, account label

**Pairing**:
The local record binding this machine to one user on one relay. A machine may
hold several; exactly one is active at a time.
_Avoid_: account, profile, connection

**Active Pairing**:
The one pairing a machine syncs and captures to. The popover mirrors it, and
entries captured on this machine upload to it. Survives restarts.
_Avoid_: current account, selected account, default

**Viewed Pairing**:
The pairing whose history the main window is displaying. Defaults to the active
pairing and may differ from it. A transient view choice only: it changes nothing
about syncing or capture, and is forgotten when the window closes.
_Avoid_: current account, selection, focus

**Pair Slot**:
The relay's record of a pairing handshake in progress: short-lived, single-use,
and claimed by exactly one device. It expires on its own and burns after too
many wrong proofs, so it is never the lasting record a Pairing is.
_Avoid_: pairing, pair record, handshake session

**Relay**:
The self-hosted service that stores and fans out ciphertext. Holds no key
material and can never read an entry.
_Avoid_: backend, host, cloud, server

### Clipboard

**Entry**:
One clipboard payload, captured and encrypted on its origin device. It exists
from the moment of capture, on the device that made it. The relay-assigned id
it acquires later is what makes it shared, not what makes it real.
_Avoid_: clip, item, paste, snippet

**Preview**:
The decrypted, single-line rendering of an entry as shown in a list.
_Avoid_: excerpt, summary

**Act**:
Something a device does that the relay has to be told about: a capture, or a
use. An act is real from the moment the device performs it, whether or not the
relay has heard of it — which is what a pending one is waiting to change.
_Avoid_: operation, action, mutation, event

**History**:
A user's entries, newest first. Two regions in one order: this device's pending
acts, in the order it will send them, above the entries the relay has ordered
by last use. Not capture order — an entry recalled today leads a history of
entries captured since — and the seam does not move at flush, because the relay
stamps a pending act on arrival exactly where the device already showed it.
_Avoid_: log, feed, buffer

**Use**:
Any act the relay records against an entry so that it leads the history:
capturing it, recalling it, or copying its text again on a device that already
holds it. Reading an entry is not a use, and neither is resending a pending —
there is no relay record to move.
_Avoid_: bump, touch, promote, access, reuse

**Last Use**:
The moment of an entry's most recent use, on whichever device, as recorded by
the relay. It orders the entries the relay knows and measures how long an entry
is kept, so an entry in regular use is never the one dropped to make room. An
entry whose latest act is still pending has no last use for that act: the queue
orders it until the relay stamps it.
_Avoid_: bumped at, updated at, modified, sort key, last accessed

**Filter**:
A narrowing of the history on screen to the entries whose text contains what
was typed. Never asks the relay: it hides rows a device already holds, so it can
only ever find what has already reached that device. It narrows whichever history
is shown rather than choosing which one that is, so unlike the viewed pairing it
outlives the surface being put down and picked up again.
_Avoid_: search, query, find, lookup

**Capture**:
The act of a device turning local content into an entry — unless that device
already holds the same text, in which case the copy is a use of what it holds
and no entry is made. How the device comes by that content is not uniform: see
**Watched Capture** and **Offered Capture**.
_Avoid_: watch, grab, sync-up

**Watched Capture**:
Capture where the device noticed the clipboard change itself, unprompted. Desktop
only — no mobile OS lets a backgrounded app watch the clipboard.
_Avoid_: auto-capture, monitoring, polling

**Offered Capture**:
Capture where the person handed the content over, so the device never sees a
clipboard it was not shown. The only capture a phone or tablet performs.
_Avoid_: manual capture, import, paste

**Recall**:
Placing an entry's plaintext back onto this device's clipboard, the reverse of
capture. Distinct from reading an entry, which shows it without touching the
clipboard. A recall is a use, so the entry leads the history on every device
afterwards; reading it changes nothing.
_Avoid_: copy, restore, paste, retrieve

**Undecryptable**:
An entry this device holds ciphertext for but cannot decrypt, because it does
not have the key the entry was sealed with.
_Avoid_: corrupt, invalid, broken

**Origin**:
The device an entry was captured on, as distinct from the device viewing it.
_Avoid_: source, sender, author

**Pending**:
An act this device holds but has not yet placed on the relay, because the relay
could not be reached: a capture, or a use. The payload of a pending capture is
an Entry from the moment it is made; what is missing is the relay's id, and so
no other device knows of it yet. Pendings reach the relay in the order of the
acts that queued them — recalling, re-copying or resending one is a fresh act,
which moves it to the back of the queue and thereby to the head of the history.
_Avoid_: outbox, backlog, unsent, pending entry

**Refused**:
A pending the relay turned down for what it is rather than for being out of
reach — too large, malformed. It leaves the queue so that nothing waits behind
it, and stays on this device until it is resent or deleted. Being unreachable
is never a refusal: that is what the queue is for.
_Avoid_: failed, error, dead letter, bounced

**Resend**:
Putting a refused pending back in the queue. A fresh act and not a retry, so it
goes to the back of the queue, leads the history, and carries nothing forward
from the refusal that preceded it.
_Avoid_: retry, requeue, try again

**Contact**:
The most recent moment a device had a live connection to the relay, evidenced
by any traffic from it. Frozen when the connection drops; asserts nothing about
pending uploads, and nothing a device sends to the update source counts toward
it.
_Avoid_: sync, last seen, heartbeat, online since

**Catch-Up**:
Everything a device asks the relay for on connecting, being the acts it missed
while it was not connected. One request and one answer, not a stream: a device
that was away learns what changed in a single step, and only then does it start
hearing about acts as they happen.
_Avoid_: backfill, resync, replay, initial load

### Surfaces

**Popover**:
The tray window: a keyboard-first picker for pasting a recent entry. Desktop
only.
_Avoid_: tray menu, panel, quick view

**Main Window**:
The full surface: history, pairings, devices, capture rules. The only place an
entry can be read in full rather than merely picked. A phone has no window and
splits the same surface in two: the History Screen and the Settings Screen.
_Avoid_: preferences, dashboard, console

**History Screen**:
The phone's first surface: the viewed pairing's history, with offer and recall
as on-screen verbs.
_Avoid_: main screen, home, feed

**Settings Screen**:
The phone's second surface: this phone's pairings, and what is true of the phone
itself rather than of any one pairing. Reached only from the History Screen.
_Avoid_: settings menu, preferences, pairings screen, about

**Place**:
Where in a history a surface is scrolled to: the rows a person can see, and the
one their eyes are on. A fact about a screen and not about an entry, so never
the head — the head is which entry leads the history and is the same on every
device at once, while a place is one surface's own. Never on the relay: nothing
anywhere records where anybody was looking.
_Avoid_: scroll position, viewport, offset, index

**Standing Actions**:
The verbs a device exposes without being opened: offer the clipboard, recall the
entry at the head of the history — the last one used, not the last one captured.
Unlike the popover they show nothing and pick nothing.
_Avoid_: quick actions, shortcuts, widget, tile

**Receipt**:
Confirmation that a verb did what was asked, needing nothing back. Transient,
and the same whether the app was open or closed when the verb ran.
_Avoid_: toast, snackbar, confirmation, success message

**Notice**:
A statement that something needs doing or knowing — a refusal, a warning, a
consequence. Unlike a receipt it persists until dismissed, because it exists to
be acted on.
_Avoid_: alert, banner, error, message

### Distribution

**Release**:
One published version: a version number, notes, and one installable artifact per
platform. Every client shares the number, so a release always names the same code
everywhere.
_Avoid_: build, drop, version bump

**Update**:
A device replacing its installed app with a newer release. Distinct from sync,
which moves entries, never code.
_Avoid_: upgrade, patch, sync

**Update Source**:
The public location the newest release is asked for. A desktop asks directly,
which makes the update source the only counterparty besides the relay that it
ever contacts, and unlike the relay it is not self-hosted: it sees a device's
address, though never an entry. A phone never asks — something else asks on its
behalf — so a phone's only counterparty is the relay.
_Avoid_: update server, endpoint, CDN, release feed
