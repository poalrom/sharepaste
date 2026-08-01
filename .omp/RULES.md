# Constitution — sharepaste

Standing decisions for this repo. Apply them without asking and cite the number (`R1`).
Global rules (`G<N>`) apply too and are not repeated here.
If a rule is wrong, do not work around it: say so and I will amend it.

## Defaults — override with a logged reason

1. **R1** Add a dependency edge between any two tickets that rewrite the same file, even when it narrows the parallel frontier. Zero overlap beats a wider frontier plus a later reconciliation.
2. **R2** Set every support floor as low as it goes while going lower costs nothing; raise a floor only with the cost named.
3. **R3** Record an exposure that requires my own unlocked device as an accepted consequence I own, never as an open hole.

## Accepted under R3

Exposures I own rather than holes I have left open. Each of these needs my own phone,
already unlocked, in front of somebody. The reasoning is in
[ADR 0009](../docs/adr/0009-the-phone-shows-what-it-recalled.md) and is not restated here.

1. **The Recall Receipt draws over the foreground.** A Recall run from the Standing Actions
   notification puts a line of clipboard content over whatever app was on screen. The
   notification is `VISIBILITY_SECRET`, so it never fires from a lock screen.
2. **The system paste chip renders the recalled text.** On the vendors that draw one it is
   legible again, which is the point of removing `EXTRA_IS_SENSITIVE` rather than a side
   effect of it.
3. **The keyboard's clipboard-history panel keeps it un-redacted** until its own retention
   expires it. The durable one of the three, and the one worth revisiting first.

`logcat` is deliberately not a fourth. The Recall logs `receiptLogged`, a fixed sentence
naming no Entry, kept separate from the sentence the Toast shows.
