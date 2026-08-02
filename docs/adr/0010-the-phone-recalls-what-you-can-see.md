# The phone recalls what you can see

**Decided 2026-08-02.** The History Screen gained a **Filter**, and with it the first row of
the list stopped meaning "the newest **Entry**". That broke an agreement the screen had been
keeping since it was built: the row it marks — emitter bar, tint, a named button instead of a
glyph — is *"the row the verb bar acts on … a person should be able to see which one that is
before they press it"*. Under a Filter, the marked row and the entry `RECALL LATEST` hands
over are usually different rows.

We resolved it in favour of the screen. The verb is now **`RECALL FIRST`**: it takes the first
row of the displayed list, from cache, on the **Viewed Pairing**. It performs no round trip
and it is disabled when the list is empty.

The alternative was to keep the round trip and let the marker follow the button. We rejected it
on the user's own argument, which is the better one: a button that fetches, discovers something
newer, and hands *that* over is a button that recalls an **Entry** the person never saw. A stale
row is the lesser surprise, because staleness is already disclosed — the **Contact** readout is
a permanent band on this screen precisely because a phone is out of contact almost always
([ADR 0007](0007-a-phone-only-acts-when-you-act.md)) — while "this is not the row you were
looking at" is disclosed nowhere and cannot be.

`RECALL LATEST` was the right verb while its promise was *the newest thing that exists*, a claim
about something not on screen, which is why it always fetched and why a fallback to cache had to
say so out loud. `RECALL FIRST` promises *that row, there*, and a promise you verify with your
eyes before pressing needs no warning attached to it.

## Consequences

**`Notice.RecalledFromCache` is deleted.** `SharepasteViewModel.recallLatest()` was its only
raiser and the verb bar was that method's only caller, so the whole chain is orphaned: the
notice, its `TAG_NOTICE_STALE` band, the whole-band caution tint that no other notice wears,
and the `recall_from_cache` / `recall_from_cache_badge` strings on the in-app path. Two tests
lose their subject rather than their tag — the offline half of `OfflineOfferAndRecallTest`,
which proved a failed fetch is reported as a cache read, and the half of `RoundTripTest` that
proved the band is *absent* when the relay answered. There is no fetch left to fail or succeed.

**The phone has no on-screen staleness warning.** What remains is the **Contact** readout,
which states when this device last reached the relay and leaves the inference to the reader.
That is a weaker disclosure than a band that must be dismissed, and it is the price of the
decision. It is affordable only because the verb no longer claims freshness; a future change
that restores a freshness claim to this button must restore the warning with it.

**The in-app verb and the Standing Action are no longer the same act.** The notification's
Recall keeps `recall_latest` — round trip, **Active Pairing**, `Receipt.Aloud` on a cache
fallback — and `StandingActionsOnAClosedPhoneTest` still proves it. The screen's verb is cache,
**Viewed Pairing**, no warning. `SharepasteApp.kt` currently justifies its actions record on the
grounds that these call *"the same repository entry points the Standing Actions do"*, and that
sentence stops being true. The divergence is deliberate: one verb has a list in front of it and
the other has nothing.

**The verb now follows the Viewed Pairing.** `recallLatestOnActivePairing` took from the
**Active Pairing** while the list showed the **Viewed** one, so under a divergence today's
button already hands over an entry from a history the person is not looking at, and today's
marker already lies about it. Taking `entry.userId` from the row fixes that as a side effect,
matching the rule the per-row Recall has followed since it was written: a row must recall from
the **History** it is a row of.

## Considered Options

**Keep the round trip when nothing can disagree.** Filter empty *and* viewing the Active
Pairing, the first row genuinely is the newest entry of the pairing being synced, so the button
could fetch exactly as before and keep the warning; otherwise take the displayed row from cache.
This preserves `Notice.RecalledFromCache`, both tests, and the two doc comments that use it as
the worked example of the Notice-versus-Receipt line. Rejected because the fetch can still swap
in an entry the person never saw — the exact surprise the change exists to remove — and because
it is a compound condition on a screen whose bands each earn their keep by having one rule.

**Keep `RECALL LATEST` and mark by identity.** Pass the unfiltered head's id into the rows and
mark whichever row matches it, marking nothing when the newest entry does not survive the
Filter. Truthful, and it changes no behaviour at all. Rejected because it answers the
disagreement by removing the anchor: under a Filter the screen would show a list with no
distinguished row and a full-width solid button that acts on none of them.
