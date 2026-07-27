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
One paired machine belonging to a user. Every entry records the device it came
from.
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
The act of a device noticing a local clipboard change and turning it into an
entry.
_Avoid_: watch, grab, sync-up

**Undecryptable**:
An entry this device holds ciphertext for but cannot decrypt, because it does
not have the key the entry was sealed with.
_Avoid_: corrupt, invalid, broken

**Origin**:
The device an entry was captured on, as distinct from the device viewing it.
_Avoid_: source, sender, author

**Pending**:
Captured entries queued on this device because the relay could not be reached.
_Avoid_: outbox, backlog, unsent

**Contact**:
The most recent moment a device had a live connection to the relay, evidenced
by any traffic from it. Frozen when the connection drops; asserts nothing about
pending uploads, and nothing a device sends to the update source counts toward
it.
_Avoid_: sync, last seen, heartbeat, online since

### Surfaces

**Popover**:
The tray window: a keyboard-first picker for pasting a recent entry.
_Avoid_: tray menu, panel, quick view

**Main Window**:
The full surface: history, pairings, devices, capture rules. The only place an
entry can be read in full rather than merely picked.
_Avoid_: preferences, dashboard, console

### Distribution

**Release**:
One published version of the desktop app: a version number, notes, and one
installable bundle per platform.
_Avoid_: build, drop, version bump

**Update**:
A device replacing its installed app with a newer release. Distinct from sync,
which moves entries, never code.
_Avoid_: upgrade, patch, sync

**Update Source**:
The public location a device asks for the newest release. The only counterparty
besides the relay that a device ever contacts, and unlike the relay it is not
self-hosted: it sees a device's address, though never an entry.
_Avoid_: update server, endpoint, CDN, release feed
