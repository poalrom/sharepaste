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

**Relay**:
The self-hosted service that stores and fans out ciphertext. Holds no key
material and can never read an entry.
_Avoid_: backend, host, cloud, server

### Clipboard

**Entry**:
One clipboard payload, captured and encrypted on its origin device and
identified by a relay-assigned id.
_Avoid_: clip, item, paste, snippet

**Preview**:
The decrypted, single-line rendering of an entry as shown in a list.
_Avoid_: excerpt, summary

**History**:
A user's entries, newest first, as held by the relay and mirrored locally.
_Avoid_: log, feed, buffer

**Capture**:
The act of a device turning local content into an entry. How the device comes by
that content is not uniform: see **Watched Capture** and **Offered Capture**.
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
clipboard.
_Avoid_: copy, restore, paste, retrieve

**Undecryptable**:
An entry this device holds ciphertext for but cannot decrypt, because it does
not have the key the entry was sealed with.
_Avoid_: corrupt, invalid, broken

**Origin**:
The device an entry was captured on, as distinct from the device viewing it.
_Avoid_: source, sender, author

**Pending**:
A capture this device holds but has not yet placed on the relay, because the
relay could not be reached. Not an Entry: it carries no relay-assigned id, and
no other device knows of it. It becomes an Entry when the relay takes it.
_Avoid_: outbox, backlog, unsent, pending entry

**Contact**:
The most recent moment a device had a live connection to the relay, evidenced
by any traffic from it. Frozen when the connection drops; asserts nothing about
pending uploads, and nothing a device sends to the update source counts toward
it.
_Avoid_: sync, last seen, heartbeat, online since

### Surfaces

**Popover**:
The tray window: a keyboard-first picker for pasting a recent entry. Desktop
only.
_Avoid_: tray menu, panel, quick view

**Main Window**:
The full surface: history, pairings, devices, capture rules. The only place an
entry can be read in full rather than merely picked. A phone presents its
functions without a window.
_Avoid_: preferences, dashboard, console

**Standing Actions**:
The verbs a device exposes without being opened: offer the clipboard, recall the
latest entry. Unlike the popover they show nothing and pick nothing.
_Avoid_: quick actions, shortcuts, widget, tile

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
