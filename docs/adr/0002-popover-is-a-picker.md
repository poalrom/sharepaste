# The popover is a picker, not a status surface

The design mock for the tray popover draws it as a HUD instrument panel:
a permanent telemetry strip (`CACHE 11/11 · LAST SYNC 14:22:07`), a cipher badge,
and a per-row `CACHED` marker when offline — 139px of a 480px window spent on
chrome. We deliberately did not build those. The popover exists to be opened,
glanced at, and dismissed in about a second, so bands that only inform in the
failure case do not get to cost rows in the nominal case.

## Consequences

Nominal chrome is 116px — header, search, hint strip, footer — leaving 10 full
rows and a sliver of the 11th. Relay health appears **only when degraded**, as a
single strip carrying `OFFLINE`/`AUTH FAILED` and `LAST CONTACT`, so a healthy
window shows nothing about itself. The cache gauge was cut entirely: `MAX_PER_USER`
is a client-side prune cap, not a quota anyone manages, and the 100-entry limit
is explained instead by a list-end sentinel at the exact point a user hits it.
The cipher badge was cut as decoration that resembles information; if the cipher
is ever disclosed it belongs beside pairing in the Main Window. The per-row
`CACHED` marker was cut because it repeats a window-level fact on every row while
destroying the origin and age the row is there to show.

The same reasoning later emptied the hint strip of its `↑↓ NAV` entry. The plan
drew four hints (`§3`: `↑↓ NAV · ⏎ COPY · ⌘⏎ KEEP · ⌘⌫ DEL`), which at 360px
left no slack: `KEEP OPEN` wrapped and clipped, and the line was a run of
glyphs with no boundary between the key and what it does. Arrow keys are the
one binding a list with a highlighted row does not have to teach, so the strip
now spends its width on the three that do — each a keycap paired with a whole
verb, named the way the reader's own keyboard is labelled (`CTRL+BKSP` on
Windows, `⌘⌫` on a mac). `DEL` is gone in particular because it reads as the
Delete key, which is not the binding.

It reached the deferred disclosure too, in the end. The Android pairing flow
carried `XCHACHA20-POLY1305` in a 34dp footer, put there on the strength of the
sentence above — beside pairing, at the moment a Relay is
being trusted. That footer is gone (`docs/android-redesign.md` §2). It went
because `RELAY MUST BE HTTPS` shared the band and is inert on a phone, which
never chooses a scheme: the scheme arrives inside the pairing code. A two-line
band with one inert line is a band that reads as decoration, which is this
record's own objection applied one step further than this record went.

The residue has to be stated exactly, because it is not a clean win. The cipher
is still disclosed — on each Pairing card, on the phone's Settings Screen and in
the desktop's Pairings section, held there by `PairingsScreenTest`,
`SettingsThatDoNotExistTest` and `PairingsSection.test.tsx`. What is gone is the
**placement** this record asked for. A Pairing card describes a Relay already
trusted; the footer stood at the moment of trusting one, which is the only
moment the fact could inform a decision. So the disclosure survives in a weaker
position than the sentence above specified. That is a consequence of the
argument, not a reversal of it — but it is a cost, and anyone reinstating a
footer should answer the inert second line first. The cipher was never the part
of that band that failed.

The rejections are the point of this record. The mock itself is not in the
repository — it was reviewed from a local artifact — so nothing here can be
recovered by looking at it later. Anyone who sees that mock, or its
descendants, will find bands the shipped popover does not have and assume the
work is unfinished. It is not.
