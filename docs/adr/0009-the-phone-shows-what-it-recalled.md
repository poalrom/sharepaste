# The phone shows what it recalled

**Decided 2026-08-01, not yet built.** The decision is settled and the reasoning below
holds; the Android client still sets the flag this record removes.

`AndroidClipboard.writeText` marks every Recall with `ClipDescription.EXTRA_IS_SENSITIVE`,
which is what Android tells all apps to do with sensitive content regardless of target SDK.
On a Pixel the effect is that the system's paste chip renders `••••••`, so the one thing a
person needs after tapping **Recall Latest** — did I get the right Entry? — is the one thing
the phone will not tell them. Other vendors do not draw that chip at all, and there is no
API to ask which behaviour a given device has, so the app cannot decide this on the person's
behalf. We removed the flag and made the app answer for itself: a **Receipt** carrying the
Entry's **Preview**, with a switch on the Settings Screen to silence it.

## Consequences

**Recalled plaintext is no longer redacted in surfaces we do not own.** The flag suppressed
more than the paste chip — it also obfuscated the entry inside the keyboard's own
clipboard-history panel, which outlives the paste. Removing it means the most recently
recalled text is legible there until the keyboard's retention expires it. That is the price
of the decision, not an oversight, and it is the reason this record exists.

**The Receipt is itself a new surface for plaintext.** A Recall driven from the Standing
Actions notification runs with no app on screen, so the Receipt draws a line of clipboard
content over whatever was in the foreground. It cannot reach a lock screen: the notification
is `VISIBILITY_SECRET`, so **Recall Latest** is only tappable on an unlocked phone. On an
unlocked phone it is visible to anyone looking at it, and to a screen recording. Accepted
under **R3** — an exposure that requires the owner's own unlocked device is a consequence
they own, not an open hole.

**None of this touches the notification.** [ADR 0007](0007-a-phone-only-acts-when-you-act.md)'s
rule stands unchanged: the ongoing notification never previews an Entry and stays
`VISIBILITY_SECRET`, and the string whitelist in `StandingActionsNotificationTest` still
enforces it. What changed is what is visible *after* the action fires.

**The switch is the phone's first real preference.** Until now the Settings Screen carried
three inert `N/A` chips precisely because a phone had nothing to switch. This one exists
because the vendor variance is undetectable — not as a way to avoid making the call.

## Considered Options

**Keep the flag; let the Receipt carry the Preview anyway.** The closest alternative, and
strictly the safer one: the Receipt would work on every device while the keyboard's
clipboard-history panel stayed redacted, which is a durable surface the Receipt is not.
Rejected on a weaker argument than that cost deserves — a dotted `••••••` chip appearing
beside our own legible Receipt, a second apart, reads as a fault in the app rather than as a
protection. Anyone revisiting this should revisit that trade first.

**Keep the flag; show only the recalled text's first character.** What was originally asked
for. Rejected: one character does not separate two URLs, which is the common case, so it
buys the exposure of a preview without the confidence of one.

**Detect whether the device draws a paste chip and adapt.** There is no such API, public or
otherwise; the chip is system-UI behaviour with no capability query. This is the whole
reason the preference is a switch rather than a derived value.
