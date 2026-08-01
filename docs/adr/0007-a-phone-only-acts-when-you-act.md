# A phone only acts when you act

**Deferred, 2026-07-29 — the iOS half is not shipped.** Every constraint recorded here
holds, and the Android client was built against it. But there is no iOS client, so the iOS
surfaces below — App Intents driven by Shortcuts, the Share Extension that cannot exist,
the iCloud backup exclusion — are the intended shape rather than shipped code. See the
deferral note in [ADR 0008](0008-sideloaded-and-not-self-updating.md).

The desktop client's core loop is **Watched Capture**: a background thread notices
every clipboard change and turns it into an entry. No mobile OS permits this, and
the ways they forbid it are not obstacles to route around — they are the platform's
position. Android 10 and later allow clipboard reads only to the app with input
focus or the default IME, `onPrimaryClipChanged` does not fire otherwise, and the
restriction applies regardless of `targetSdk`. iOS restricts pasteboard reads to the
foreground and raises a paste banner on every read. So on a phone every entry is
**Offered Capture**: the person hands the content over, and the device never sees a
clipboard it was not shown.

Everything else here follows from the same root. A phone cannot watch, cannot hold a
stream, and cannot be woken; therefore it acts only when acted upon. This record
exists because a future reader will find a mobile client that does not auto-capture,
does not notify on new entries, and shows itself as out of contact almost always,
and will conclude the work is unfinished. It is not — the same job ADR 0002 does for
the popover's missing bands.

## Consequences

**The surfaces are asymmetric, and no framework choice would have changed that.**
The two verbs — offer the clipboard, recall the latest entry — are **Standing
Actions**, and each platform exposes them its own way. On Android, an ongoing
notification whose action launches a transparent activity, because the activity is
what holds window focus: a `BroadcastReceiver` fired from a `PendingIntent` reads
the clipboard as empty, and a foreground service does not help either, since a
foreground service is not the same as a focused app. It is deliberately *not* backed
by a foreground service — Android 15 caps `dataSync` foreground services at six
hours per twenty-four, and Android 14 made ongoing notifications user-dismissible
anyway, so the service would buy nothing and cost a hard timeout. It is re-posted on
`BOOT_COMPLETED`. On iOS, App Intents in the main app binary, driven by Shortcuts:
the user's shortcut chains *Get Clipboard* into our intent, so **Shortcuts** touches
the pasteboard and we never do — no banner, no permission, no app switch — and
**Recall** inverts it, returning a string for *Copy to Clipboard* to write.

**There is no iOS Share Extension, and there cannot be.** A free Apple Personal Team
cannot use entitlement-gated capabilities, which include App Groups and Keychain
Sharing. An extension is a separate process with its own container: without those it
can read neither the SQLite cache nor the user key nor the device token, so it can
neither encrypt an entry nor authenticate to the relay. There is no workaround
worth having — relaying through a URL scheme forfeits the extension's only advantage
by forcing an app switch anyway, and burns a second of the three app-ID slots a free
team allows. See [ADR 0008](0008-sideloaded-and-not-self-updating.md).

**Sync is foreground-only, and on iOS that is forced rather than chosen.** The relay
pushes solely over SSE and keeps no replay: no `id:` frames, no `Last-Event-ID`, no
buffer, so every event emitted while a client is disconnected is lost permanently
and `GET /entries?since=` is the only catch-up. Neither phone can hold that stream —
iOS suspends the app and Background Modes is entitlement-gated alongside push, while
Android would need the foreground service already ruled out. So the session opens on
resume and tears down on background, and **Recall Latest** does not read the cache:
it fetches, decrypts, and writes the clipboard, so that the one operation which must
be correct always is. Offline it falls back to the newest cached entry and must say
so visibly, or it will hand over yesterday's link.

The cost is stated plainly because it is the part that will feel like a bug:
**something copied on a laptop does not reach a phone until the app is opened or
Recall Latest is tapped.** There is no "new clipboard item" notification, ever.

**Contact still means what it always meant, and therefore reads differently.** Its
definition — evidence of a live connection to the relay — is untouched. But a phone
is out of contact almost always, so ADR 0002's rule that relay health appears *only
when degraded* would render a healthy phone as permanently broken. Whatever the
phone shows about its own connection has to treat "not connected" as the nominal
case.

**The notification never previews an entry and is marked secret**, so the surface
cannot leak on a lock screen. This does not close the real hole: with plaintext at
rest and no biometric gate (ADR 0003's reasoning, mirrored), one tap on **Recall
Latest** hands the last copied secret to whoever is holding an unlocked phone. A
biometric gate is the only control that addresses it, and it is deliberately not in
the first release. Backup exclusion is consequently load-bearing rather than merely
correct: `android:allowBackup` defaults to `true` and the iOS app container is
included in iCloud backups by default, so an unexcluded cache would sync a plaintext
clipboard history to Google Drive or iCloud.

A later decision widens that last hole deliberately: the Android client stops redacting its
own clipboard writes and draws a **Receipt** carrying the recalled Entry's Preview, so that
a person can tell whether they got the right one. See
[ADR 0009](0009-the-phone-shows-what-it-recalled.md). The rule opening this paragraph is
untouched — the notification still previews nothing and is still secret. What moved is what
is visible after the action fires, on a phone that is by construction already unlocked.

## Considered Options

**An Android IME.** The default keyboard is the one app Android still lets watch the
clipboard continuously, so this is the only route to genuine desktop parity on any
phone. Rejected: it means shipping a keyboard good enough to type on all day, and it
deepens the Android/iOS split rather than closing it.

**Read the clipboard on every app foreground.** Legal on Android and nearly free.
Rejected because it is silent surveillance of your own clipboard triggered by app
launch — it captures things never meant to leave the device — and on iOS it raises
the paste banner on every launch until the user disables the prompt in Settings.

**A share target as the only gesture.** Genuinely identical on both platforms, needs
no clipboard access and no permission, and receives the text inside the intent.
Kept on Android as a secondary path, rejected as the primary: it cannot pull anything
out, so **Recall** would not exist, and Select-then-Share is unavailable in places
where Copy works.

**FCM push, with the relay waking the phone.** The only route to instant delivery,
free, and needing no licence. Rejected on the threat model rather than the effort:
the relay would hold a Google credential and learn when to wake a device, and
Google's infrastructure would gain per-device delivery metadata for an
end-to-end-encrypted self-hosted tool whose README names exactly two counterparties.
ADR 0005 already treats adding one third party as surprising enough to need a
record. It is also Android-only regardless, since APNs needs the paid membership
declined in ADR 0008.

**A background top-up to keep the cache warm** — WorkManager's fifteen-minute floor
on Android, `BGAppRefreshTask` on iOS. Rejected because a cache that is *usually*
fresh is worse than one that is never trusted: you trust it, and then you paste
something stale. Freshness would also become platform-asymmetric and unpredictable,
bought for a few hundred milliseconds.

**A cheap change-check route on the relay** — `GET /entries/head`, or an ETag on
`GET /entries`, latched off on 404 exactly as ADR 0001 established for `/me`.
Deferred, not rejected: it is a wire-protocol widening in service of a client that
does not exist yet, and the full fetch is already fast at these volumes.
