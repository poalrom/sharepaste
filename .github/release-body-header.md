Desktop app for macOS (Apple Silicon) and Windows (x64), and an Android app as
one universal `.apk`. The relay is not released as a binary — operators build it
from `docker compose`.

**These bundles are not signed or notarized.** macOS quarantines anything a
browser downloaded, so a fresh install needs one command before it will launch:

```sh
xattr -dr com.apple.quarantine /Applications/sharepaste.app
```

That is a first-install tax, not a per-release one: updates taken from inside
the app are fetched by the app's own HTTP client, which never sets the
quarantine attribute, and are verified against a signing key compiled into the
binary. The reasoning is in
[ADR 0005](https://github.com/poalrom/sharepaste/blob/main/docs/adr/0005-unsigned-downloads-signed-updates.md).

The macOS build is Apple Silicon only — there is no Intel bundle.

## Android — `sharepaste-<version>-universal.apk`

One APK for every device: it carries `arm64-v8a` and `x86_64`, so there is
nothing to choose. `minSdk` is 29 (Android 10). It **is** signed, with our own
release key, and that signature is the whole update mechanism — Android refuses
any update not signed by the same key, so **the app contains no update code and
never contacts this page**. Its only network counterparty is your relay. See
[ADR 0008](https://github.com/poalrom/sharepaste/blob/main/docs/adr/0008-sideloaded-and-not-self-updating.md).

**Install and update it with [Obtainium](https://github.com/ImranR98/Obtainium)**,
which watches this releases page for you:

1. In Obtainium, tap **Add app**.
2. Paste `https://github.com/poalrom/sharepaste` into **App source URL**, and tap
   **Add**. (Or open `obtainium://add/https%3A%2F%2Fgithub.com%2Fpoalrom%2Fsharepaste`
   on the phone, which fills that field in for you.)
3. Turn **Attempt to filter APKs by CPU architecture if possible** *off* — there
   is one APK and it needs no guessing — and leave the archive/tarball toggles
   off so the desktop `.app.tar.gz` on this Release is never offered as an
   install. Set **Trim version string with RegEx** to `^v(.*)$` so the tag
   `v<version>` compares equal to the version the app reports.
4. Tap **Install**. Android will ask you to allow *Obtainium* to install unknown
   apps; that permission belongs to Obtainium, not to Sharepaste.

Every later release is then an **Update** button in Obtainium, installed in
place: the same signing key means your pairings and history survive.
[`.github/obtainium.json`](https://github.com/poalrom/sharepaste/blob/main/.github/obtainium.json)
holds those settings as an Obtainium import file if you would rather not set
them by hand.

Downloading the APK from this page and tapping it works too — you just have to
come back here yourself for the next version.

---
