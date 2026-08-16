# Silence is per verb

**Decided 2026-08-16.** `SHOW WHAT WAS RECALLED` keeps its exact scope and gains a neighbour,
`CONFIRM OFFERS`, which silences the **Receipt** that says an **Offered Capture** was taken.
Neither switch reaches the other's verb, and one function — `silences` in `ui/Receipt.kt` — is the
whole of what either does.

[ADR 0009](0009-the-phone-shows-what-it-recalled.md) gave the phone a switch because a **Recall**
hands back something the person did not choose and its Receipt names the **Preview** — a line of
clipboard content drawn over whatever was in the foreground. That argument does not reach an Offer.
An Offer Receipt names nothing; the person supplied the content a second ago, and what they may
want silenced is not a disclosure but the app speaking at all. Two different reasons to be quiet,
so two switches: a person who does not want their clipboard named on screen and a person who does
not want a Toast over the app they were reading are asking for different things, and one control
would make each of them pay the other's price.

## Consequences

**The Settings Screen is no longer a screen with one switch, and that was an argument ADR 0009
made.** *"The switch is the phone's first real preference"* was written to justify one control on a
surface that had carried three inert `N/A` chips precisely because a phone had nothing to switch.
A second live switch weakens that: the reasoning that admits two admits a third, and the chips
beside them now read a little more like switches somebody stopped wiring up. `ThisPhoneSection`
keeps them apart from the chips and the census in `PairingsScreenTest` names both permitted
switches, so a third one fails a test rather than passing review — but the *architectural* claim
that this phone has one thing worth switching is spent, and anyone adding a fourth confirmation
should read this paragraph before reaching for a switch as the answer.

**A recognised Offer still speaks with `CONFIRM OFFERS` off.** `ALREADY SAVED` is not the ordinary
Offer Receipt in different words: nothing was saved, and going quiet there says *saved* by
omission on a list the person can turn to and check a second later — the exact claim
[ADR 0012](0012-a-repeat-copy-is-a-use.md) made a separate Receipt in order not to make. So the
switch has an exception its own label cannot state, and the note under it states it instead. That
note is load-bearing, not decoration.

**The share target is gated for the first time.** `ShareTargetActivity` reported unconditionally
and reasoned that it was allowed to: no share can produce a `Receipt.Recalled`, so there was
nothing for the one switch to silence. Every share produces an Offer, so that reasoning expired
with this decision. It was the path nobody would have thought of, and the answer is structural
rather than vigilant. *Which* Receipts a switch reaches is one exhaustive `when` over the sealed
`Receipt` — `silences` — so a fourth confirmation cannot reach a Toast until an author decides
there which switch owns it. And *asking* is one function too: both windowless surfaces report
through `reportReceipt`, so there is no third site to forget. The two of them had begun to diverge
into near-copies of each other, which is the same mistake `StandingActions.TAG` was extracted to
stop.

What is left untested is the two lines of that function — that it reads the store and applies the
predicate. The predicate has its own tests at all four switch positions, the in-app path is proved
end to end against a live relay, and a Toast drawn by a window that is finishing is not something
the instrumentation can watch. Collapsing two copies into one halved that surface rather than
covering it, and this paragraph is the honest accounting.

**A `Notice` is still not silenceable from this screen.** `Receipt.Aloud` shares a Toast with a
confirmation and nothing else — a refusal, an unpaired phone, a **Recall** served off the cache —
and every sentence it carries is something to act on. Two switches make the app quieter; they do
not make it stop warning, and `ADR 0007`'s rule that a cache fallback may never be silent is
untouched.

**None of this touches the notification or the log.** The ongoing notification still previews no
Entry and stays `VISIBILITY_SECRET` (ADR 0007), and `receiptLogged` still fires on every path at
every switch position: a log is durable and readable by anything holding `READ_LOGS`, so it says
what it always said, which names no Entry.

## Considered Options

**Widen `SHOW WHAT WAS RECALLED` to cover both verbs and rename it.** The recommendation, and the
smaller change: one preference, one gate, one label needing no exception, and it keeps the
Settings Screen's one-switch argument intact. Rejected on the owner's call. The cost it avoided is
the one recorded above; the cost it carried is that silencing the Offer Receipt and hiding a
recalled Preview would have become the same request, and someone who wants the second without the
first would have had no way to say so.

**Widen the meaning and keep the name.** Cheapest of all, and rejected before it was put to
anyone. The comment above `settings_show_recalled`, the label and the note all promise a
Recall-only scope, so this would have made three statements false to save one string.

**Silence `Receipt.Recognised` along with `Receipt.Offered`.** Would let `CONFIRM OFFERS` stand
without an exception, which is most of what a control is. Rejected: see the second consequence.
The noise it removes is a sentence a person can check against the list in front of them; the
silence it buys states the one thing that is not true.

**One condition per report site, as before.** Three `if`s where there were two is not obviously
worse than a shared function, and it is the smaller diff. Rejected because `Receipt.Recalled` was
split across two variants once precisely so that a guard could name the whole of what it guards,
and because the share target is standing proof that the site everyone forgets is the site with no
test watching it. A rule about somebody's settings written down three times is three places for
those settings to end up half-applied.
