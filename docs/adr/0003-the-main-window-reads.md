# The main window reads; the popover picks

**One binding below has moved.** `⌘⌫` no longer deletes on either surface: it clears the
Filter, and deleting an Entry is `⇧⌘⌫` / `CTRL+SHIFT+BKSP`
([ADR 0013](0013-the-filter-is-a-text-field-first.md)). The reasoning below — that the
filter holds focus, so the binding cannot be a bare key — is what led there and stands
unchanged.

[ADR 0002](0002-popover-is-a-picker.md) established that the tray popover is a
picker — opened, glanced at, dismissed in about a second. It pays for that speed
by collapsing every entry to one whitespace-normalised line of at most 200
characters, which means there is nowhere in the product to *read* an entry.
Three `ss://` URLs that diverge at character 60 are indistinguishable in the
popover by construction. The main window therefore gains a **History** section:
the same list, beside a pane that renders the selected entry's full plaintext.

## Considered Options

**A reading pane only, no list interaction.** Rejected: a reader you cannot
navigate is a viewer for whatever the popover happened to leave selected. Once
the list is there, arrow keys and copy are what a reader is for.

**No history in the main window at all**, leaving it the settings surface its
glossary entry described. Rejected because the gap above is real and the fix
costs nothing on the wire: the *complete* plaintext already ships over IPC for all hundred
rows on every `list_history` — as `EntryView.preview` when this was written, and since the
extraction as `Entry.plaintext` beside a separately rendered `preview`
(`clients/core/src/event.rs:48-59`, built by the one constructor below it). The pane is a
rendering decision, not a data one.

## Consequences

**Two surfaces now copy, and their bindings differ.** In the popover `⏎` means
*copy and get out of the way*; a window has nothing to get out of the way of, so
here `⏎` copies and stays, and `⌘⏎` does not exist. `⌘⌫` deletes on both. The
design mock bound bare `Delete`/`Backspace` to deleting an entry — rejected: the
filter takes focus on any keystroke, exactly as in the popover, so a bare key
either collides with editing or fires when the user thought they were typing.
That also keeps ADR 0002's hint-strip rule intact: no arrow-key hint, and no
`DEL`.

**The reader shows plaintext unmasked.** Deliberate, and recorded so it is not
re-argued. Masking the pane while the list beside it prints the same first line,
and while `⏎` puts the real thing on the system clipboard, is theatre. The
control that actually protects a secret is the deny-list, which stops it
becoming an entry at all.

**It stops at the same hundred rows as the popover.** The relay keeps everything
— `GET /entries` prunes on neither age nor count — but the local cache prunes at
`MAX_PER_USER = 100` and thirty days (now `clients/core/src/storage/entries_cache.rs:24-25`), and it stores
**plaintext at rest**. Deepening the reader by raising that cap trades a fuller
list for more secrets on disk on every paired machine, which is the wrong trade
in an end-to-end-encrypted clipboard. Back-paging the relay is possible but
needs a `before` parameter the route does not have, an on-demand decryption
path, and a decision about whether fetched plaintext is written back. Both are
out of scope; a list-end sentinel names the limit at the row where it bites.

**The window reads one pairing while the device syncs another.** See the
**Viewed Pairing** entry in `CONTEXT.md`. Every entry command is already
user-scoped and none require the pairing to be active, so this needed no backend
change — but Viewed and Active are now genuinely different things, and a band
states so whenever they diverge.
