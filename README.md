# Sharepaste

Self-hosted, end-to-end encrypted clipboard sync between a person's own machines. A
**Relay** you run yourself stores and fans out ciphertext it can never read; each device
decrypts locally with a key the relay never sees.

Three clients exist: a desktop app for macOS and Windows, an Android app, and an iOS app.
The iOS one is **unsigned and signed on your own device by a free Apple Personal Team,
which means it expires every seven days** — the deal that buys it, and everything else it
costs, is under [Clients](#clients).

`CONTEXT.md` holds the vocabulary these documents use — Entry, Preview, Pairing, Recall,
Standing Actions and the rest. `docs/adr/` holds the decisions behind them.

## Layout

```
server/        # Node relay + operator CLI (build context for Docker)
clients/       # Cargo workspace shared by every client
  core/        #   sharepaste-core: protocol, crypto, storage, sync
  desktop/     #   Tauri 2 app (macOS, Windows)
  mobile/      #   uniffi bindings + the Android and iOS apps
db/            # SQLite host volume mounted into the container
docker-compose.yml
```

## Quick start (docker compose)

```bash
docker compose up -d --build
```

Mounts `./db` → `/var/lib/sharepaste` inside the container. SQLite file lives at `./db/db.sqlite`. Operate behind a reverse proxy that terminates TLS (Caddy, nginx).

## Before a phone can pair: TLS with a publicly trusted certificate

This is a setup step, not a recommendation. `docker compose up` publishes **plain HTTP on
port 8443**, and a desktop client will happily talk to that. **Neither phone will, and
there is no setting that makes either of them.** So a relay you intend to use from a phone
needs a real HTTPS front — Caddy, nginx, or a tunnel — before you try to pair.

What both phone apps require, and why none of it is negotiable:

- The requirement is compiled in: `REQUIRE_HTTPS` on Android, `TransportPolicy` on iOS,
  where it is `true` in every build including debug. Either way it is a constant in the
  shipped artifact rather than a preference, so there is no toggle in Settings and none
  can be added without shipping a different app.
- Each trusts its platform's bundled set of public root certificates. **There is no
  certificate pinning and no private-CA path** on either phone, so a self-signed
  certificate and a private CA fail exactly like `http://` does.

A relay behind an untrusted certificate, or none, produces "cannot reach the relay" on the
phone. The fix is the certificate, never the app. Check it from anywhere but the phone
first:

```bash
curl https://relay.example.com/healthz
{"ok":true}
```

The address the phone is handed at pairing time is that HTTPS one.

One code path, no setting behind it — the choice, not an oversight. A cleartext toggle is a
switch that quietly downgrades an end-to-end-encrypted product's transport for everyone who
ever flipped it, and a private-CA path is a trust store to maintain on every phone. The
desktop deliberately still works against a cleartext relay; the phone deliberately does
not.

## Clients

Releases live at <https://github.com/poalrom/sharepaste/releases/latest>. One version, one
Release: every client on a Release is built from the same commit.

The Android artifact rides the same Release as the desktop bundles, and has since
**0.3.0** — `sharepaste-<version>-universal.apk` is on every Release from that one onward.
The Obtainium instructions below are written from the pipeline and from that published
Release, and the in-place update was verified for real with `adb install -r`; the one step
nobody here has run end to end is Obtainium's own "check for updates" finding a *new*
release and installing over an existing copy.

### macOS — `.dmg`

Apple Silicon only; there is no Intel bundle. macOS 12+ (Monterey).

The bundle is neither signed nor notarized, so macOS quarantines whatever the browser
fetched. Drag the app into `/Applications`, then clear it once:

```sh
xattr -dr com.apple.quarantine /Applications/sharepaste.app
open /Applications/sharepaste.app
```

That is a first-install tax only: from then on the app updates itself, and an update it
fetched through its own HTTP client carries no quarantine attribute and is verified
against a minisign key compiled into the binary
([ADR 0005](docs/adr/0005-unsigned-downloads-signed-updates.md)).

### Windows — NSIS `-setup.exe`

Windows 10/11, x64. One installer, unsigned, so SmartScreen asks once. The same in-app
updater applies afterwards, and on Windows the installer is itself the update artifact.

### Android — `sharepaste-<version>-universal.apk`

One APK for every device: `arm64-v8a` and `x86_64` in a single artifact, so there is
nothing to choose. `minSdk` 29 (Android 10). It **is** signed, with our own release key,
and that signature is the whole update mechanism — Android's package manager refuses any
update not signed by the same key.

**The app contains no update code at all.** It never contacts the releases page, and its
only network counterparty is your relay
([ADR 0008](docs/adr/0008-sideloaded-and-not-self-updating.md)).
[Obtainium](https://github.com/ImranR98/Obtainium) is what watches for new versions and
installs them:

1. **Add app** (bottom navigation).
2. Paste `https://github.com/poalrom/sharepaste` into **App source URL** and tap **Add**.
   The one-tap equivalent, which fills that field in, is
   `obtainium://add/https%3A%2F%2Fgithub.com%2Fpoalrom%2Fsharepaste` — deliberately the
   bare `obtainium://` URI rather than Obtainium's own `apps.obtainium.imranr.dev`
   redirect, because routing you through a third-party host to install a self-hosted
   clipboard is exactly the counterparty this project avoids.
3. On the app's page, set:
   - **Attempt to filter APKs by CPU architecture if possible** → **off**. There is one
     universal APK; there is nothing to filter, and a wrong guess means no candidate at
     all. It is also why the asset is named `…-universal.apk` rather than something
     containing `arm64`.
   - **Trim version string with RegEx** → `^v(.*)$`. The release tag is `v<version>` and
     the APK reports `<version>`; without this the tracked version never compares equal
     to the installed one.
   - Leave **Include prereleases** off, and leave the zip/tarball toggles off — with
     tarballs on, the desktop `sharepaste.app.tar.gz` on the same Release becomes an
     install candidate.
4. **Install**. Android asks permission to "install unknown apps"; that grant belongs to
   **Obtainium**, which holds `REQUEST_INSTALL_PACKAGES`, not to Sharepaste.

Every later release is then an **Update** button, installed in place — same key, so your
pairings and history survive. [`.github/obtainium.json`](.github/obtainium.json) carries
those settings as an Obtainium import file if you would rather import than tap.

Downloading the `.apk` from the releases page and tapping it works too. You just have to
come back for the next version yourself.

**Pairing a phone needs a device that is already paired.** The phone has no invite-token
path: an already-paired desktop mints a short code in its pairing pane and renders it as a
QR code, and the phone scans it — or you type the code, if the camera is unavailable. So
the order is: relay, desktop, then phone.

### iOS — `sharepaste-<version>.ipa`

iPhone and iPad, iOS 16 or later. The `.ipa` is **unsigned**, and tapping it on the
releases page installs nothing: there is no Apple Developer Program membership behind this
project, so the app is signed on the device itself by a **free Apple Personal Team** —
your own Apple ID ([ADR 0008](docs/adr/0008-sideloaded-and-not-self-updating.md)). Every
awkward thing below follows from that one fact, and none of them is a defect waiting to be
reported.

**The app expires every seven days.** A free Personal Team's provisioning profile lasts a
week; when it lapses the app stops launching until it is re-signed. No release fixes that
and no setting avoids it — the only thing that would is the $99 a year this project
declined to pay. [SideStore](https://sidestore.io) re-signs it on the phone itself, with
no computer involved, and that is the whole reason an `.ipa` is published at all: it is an
update path for a device that already has the app, not a download for a stranger.

**Installing needs a computer once.** SideStore has to be side-loaded from a desktop
first, and before it can sign anything it needs a **pairing file** generated there and its
own on-device helper — a local VPN profile it uses to reach itself. That is SideStore's
setup rather than ours (<https://docs.sidestore.io>), and it is a standing dependency with
moving parts: an iOS release can break the chain, and the fallback is the weekly trip back
to the computer that SideStore exists to avoid.

With SideStore running, add one source and let it install and update the app:

```
https://github.com/poalrom/sharepaste/releases/latest/download/sidestore-source.json
```

That URL always resolves to the newest Release, so the source keeps working without ever
being re-added. [`.github/sidestore-source.json`](.github/sidestore-source.json) is the
file it serves, and the `.ipa` rides the same Release as the desktop bundles and the APK —
from the first Release cut after the client landed, and not before it. Said plainly rather
than left to be discovered: no published Release carries an `.ipa` yet, and nobody here
has yet watched SideStore take one version over another in place with the pairing intact.

**It costs the entire free-team allowance.** A free team may have three apps installed at
once, and this arrangement uses all three: SideStore, the copy of Sharepaste SideStore
installed, and the copy an `xtool dev` build installs from a checkout. The last two are
*different apps* — each signing route rewrites the bundle identifier its own way, so they
get separate containers, separate databases and separate pairings. **The SideStore install
is the real one; the development one is disposable.** Never migrate between them.

**Two shortcuts have to be assembled by hand before the Standing Actions do anything.**
There is no ongoing notification on iOS, so the two verbs arrive as App Intents — building
blocks for the Shortcuts app rather than finished buttons. You build one shortcut chaining
*Get Clipboard* into **Offer**, and one chaining **Recall Latest** into *Copy to
Clipboard*. Until you do, neither verb exists anywhere outside the app. That is the design
and not an omission: Shortcuts is what touches the pasteboard, so the app never reads one
it was not handed and never writes one unasked. The Settings Screen carries a section
saying which two to assemble; this paragraph exists so nobody concludes the verbs are
missing.

**Pairing an iPhone needs a device that is already paired**, exactly as on Android. The
phone has no invite-token path: an already-paired desktop mints the short code and renders
it as a QR, and the phone scans it. The relay has to be behind a publicly trusted
certificate before any of that works, because this client refuses cleartext in every
build. So the order is the same one: relay, desktop, then phone.

**This is built for one person's devices** — one Apple ID, and the two or three devices
its free team can hold at a time. What is above records how that install works; it is not
a distribution channel and is not offered as one. Anyone is welcome to read this
repository and build their own copy, and the free-team limits are exactly why there is
nothing here that hands a working app to somebody else.

## What a phone does, and what it never does

**Nothing arrives in the background. Ever.** Something copied on a laptop does not reach
the phone until you open the app or tap **Recall Latest**, and there is **no
"new clipboard item" notification** — not a delayed one, not a quiet one, none. This is a
decision and not an unfinished feature: no mobile OS lets a backgrounded app watch the
clipboard or hold the relay's event stream, so a phone that appears to receive
continuously could only be doing it by polling, and a cache that is *usually* fresh is
worse than one you never trust. [ADR 0007](docs/adr/0007-a-phone-only-acts-when-you-act.md)
records this at length for exactly the reader who is about to file it as a bug.

**The phone never captures on its own.** Every Entry a phone creates is an **Offered
Capture** — you hand the content over, through Android's notification action or its share
sheet, or through the shortcut you assembled on iOS. A desktop performs **Watched
Capture** and notices clipboard changes itself; Android 10 and later allow a clipboard
read only by the app holding input focus, and iOS restricts pasteboard reads to the
foreground and raises the system's paste banner on every one, so a phone cannot,
regardless of how it is built.

**The two Standing Actions work without opening the app**: offer the clipboard, and recall
the latest Entry. Each platform reaches them its own way. On Android they live in an
ongoing notification, re-posted after a reboot, which never previews an Entry and is
marked secret, so nothing renders on a lock screen. On iOS there is no such notification:
they are App Intents, and they become reachable only once you have assembled the two
shortcuts described above. **Recall Latest** fetches from the relay rather than reading
the cache — the one operation that must never hand over something stale — and says so
visibly when it has fallen back to the newest cached Entry because the relay was
unreachable.

**A phone shows itself out of Contact almost always, and that is nominal.** Contact means
a live connection to the relay; the sync session opens when the app comes to the
foreground and tears down when it leaves. The desktop's rule — show relay health only when
degraded — is inverted on the phone, because applying it unchanged would paint a perfectly
healthy phone as permanently broken.

Everything else is at parity with the desktop: several **Pairings** with one **Active
Pairing** and a separate **Viewed Pairing**, history, **Recall** of any Entry, and Offer.

## Local dev (no docker)

```bash
cd server
npm install
DB_PATH=../db/db.sqlite npm start -- serve
```

`DB_PATH` defaults to `/var/lib/sharepaste/sharepaste.sqlite` (the container path), so set it explicitly for local dev.

Client dev workflows, prerequisites and the manual smoke checklists are in
[`clients/desktop/README.md`](clients/desktop/README.md).

## Operator CLI

Run inside the container:

```bash
# Create a user, get a one-time invite token
docker exec sharepaste sharepaste user create alice

# List users
docker exec sharepaste sharepaste user list

# Revoke a stolen device
docker exec sharepaste sharepaste device revoke <device_id>

# Purge a user's history
docker exec sharepaste sharepaste entry purge --user <user_id>
```

The `--db` flag overrides the DB path; in the compose container it defaults to `/var/lib/sharepaste/db.sqlite` (mounted from `./db`).

## Wire protocol

Endpoints:

- `POST /claim-invite`
- `POST /pair/start`, `POST /pair/claim`, `POST /pair/payload`, `GET /pair/payload`, `GET /pair/poll`
- `POST /devices`, `DELETE /devices/:id`
- `POST /entries`, `GET /entries`, `DELETE /entries/:id`, `DELETE /entries`
- `GET /events` (SSE)

All authenticated endpoints take `Authorization: Bearer <device_token>`.

## Tests

```bash
cd server && npm test
```

Real Fastify + real SQLite tempfiles. No HTTP mocks.

## Threat model assumptions

- Operator runs HTTPS in front of the container (no in-process TLS in this build). For a
  phone this is a hard requirement rather than good practice — see
  [above](#before-a-phone-can-pair-tls-with-a-publicly-trusted-certificate).
- Devices use OS disk encryption (FileVault, BitLocker, etc).
- **Entries are stored decrypted on every device.** The local cache holds plaintext, on
  the desktop and on the phone alike; ciphertext-at-rest was considered for mobile and
  rejected, in order to keep one at-rest story and one storage implementation rather than
  two ([ADR 0003](docs/adr/0003-the-main-window-reads.md),
  [ADR 0007](docs/adr/0007-a-phone-only-acts-when-you-act.md)). On Android the cache lives
  in the app's private `filesDir`, which is what puts it behind Android's file-based
  encryption without a toggle of our own; backup and device-to-device data extraction are
  both switched off, so a plaintext clipboard history never reaches Google Drive. iOS gets
  the same two controls in its own vocabulary: the container directory is excluded from
  backup — re-asserted on every launch rather than trusted from the one that created it —
  and filed under Data Protection as readable only after the first unlock since boot,
  which is the strongest level a Standing Action fired from a locked device can still
  work at. So the history reaches iCloud no more than it reaches Drive, and a phone seized
  powered off holds nothing readable.

  That protection ends when the phone is unlocked, and the consequence should be stated
  rather than dressed up: **an unlocked phone in someone else's hand yields the last
  copied secret to one tap on Recall Latest.** There is no biometric gate in this release.
  No amount of at-rest encryption changes it — an unlocked device has already decrypted
  itself — so the control that works is keeping the phone locked and in your possession.
  That is the owner's responsibility, and naming it here is not an admission of a defect.
- **The phone's only network counterparty is the relay.** Neither phone app ships an
  updater, analytics, a crash reporter or Play Services; the watching is delegated —
  Obtainium on Android, SideStore on iOS — which is the entire reason the property
  survives ([ADR 0008](docs/adr/0008-sideloaded-and-not-self-updating.md)). On iOS that is
  sharper than a convenience: **SideStore is what contacts github.com**, and it must,
  because it is also what re-signs the app every seven days. The app it installs contacts
  nothing but your relay. Verified on the release APK rather than asserted: a whole-VM
  packet capture with an app-not-running control window, plus two independent per-process
  attributions, across pairing, sync, Offer, Recall Latest and a Standing Action fired
  with the app closed. The only address it reached was the relay's, plus DNS to resolve
  the relay's own hostname. This is precisely the property the desktop gave up when it
  gained an update check, kept deliberately on the platform where it matters more. That
  capture was Android's; the iOS client inherits the property by construction rather than
  by measurement — there is no update code in its source to capture — and the difference
  between measured and reasoned is left visible here rather than smoothed over.
- The relay is not the only service the desktop app talks to. At launch it asks the
  **Update Source** (github.com) for the newest release, revealing the machine's address,
  OS and app version. Nothing about an entry, a key or a relay is transmitted, and the
  check can be switched off in the app's Settings. **Contact** never counts this traffic.
- Device-token revocation 401s further requests but does not retroactively un-encrypt
  entries on a stolen device. **Key rotation is out of scope, and a third device raises
  what that costs.** One user key seals every entry for every device, so a key that leaks
  is a history that leaks on all of them — the desktop, the second desktop, and now the
  phone. Nothing in the product re-seals existing entries under a new key; the operator
  CLI can purge a history, which is deletion rather than rotation. The two signing keys
  are un-rotatable too, for an unrelated reason
  ([ADR 0005](docs/adr/0005-unsigned-downloads-signed-updates.md)).
- Downloaded desktop bundles are unsigned and un-notarized. Updates the app fetches for
  itself are minisign-verified against a public key compiled into the binary; a bundle
  downloaded from a browser is not verified by anything but Gatekeeper or SmartScreen. The
  Android APK **is** signed, but with our own release key: that makes it un-substitutable
  once installed, since Android pins the certificate across updates. It is not a
  notarization and nobody has reviewed it — the key is trustworthy to you only because you
  installed a copy signed with it. The iOS `.ipa` is signed by nobody until your own device
  signs it, so it has no such property at all: what lands on the phone is whatever the
  source JSON pointed at, vouched for by the TLS to github.com and by nothing else.

See [ADR 0005](docs/adr/0005-unsigned-downloads-signed-updates.md) for why the desktop app
contacts a third party at all, and
[ADR 0008](docs/adr/0008-sideloaded-and-not-self-updating.md) for why the phone does not.
