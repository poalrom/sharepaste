# Sideloaded, and not self-updating

**Shipped, 2026-08-02 — the iOS half exists, and three of these four caveats are spent.**
Written as a deferral on 2026-07-29, when the effort this record belonged to delivered the
Android half only. Nothing in the decision was overturned, so the correction is confined to
what that note told a reader not to infer:

- **A Release carries four client artifacts**, which is what the Consequences below always
  said it would: the macOS `.dmg`, the Windows NSIS `.exe`, the universal `.apk` and an
  unsigned `sharepaste-<version>.ipa`, beside `latest.json` and the SideStore source JSON
  kept at `.github/sidestore-source.json`. There is still **no Xcode project**, and that
  clause survives as a decision rather than an absence: the client is a SwiftPM package
  plus an `xtool.yml`, built and signed by `xtool`
  ([ADR 0006](0006-one-protocol-three-shells.md)).
- **`check-versions.mjs` asserts a seventh version**, and it is the iOS one: the
  `CFBundleShortVersionString` in the hand-written `clients/mobile/ios/Info.plist`. It
  joins the six it already reads, with `clients/desktop/src-tauri/tauri.conf.json` still
  authoritative — `clients/desktop/package.json`,
  `clients/desktop/src-tauri/Cargo.toml`, `clients/core/Cargo.toml`,
  `clients/mobile/ffi/Cargo.toml` and the `versionName` in
  `clients/mobile/android/app/build.gradle.kts`, the two shared Rust manifests because
  their crates compile into the binary each phone carries. The plist entry is tighter than
  the `MARKETING_VERSION` this record promised, and it is tighter *because* there is no
  Xcode project to hold one: the plist string is the literal value SideStore compares an
  installed app against, so a version that drifts does not merely look untidy — SideStore
  stops seeing an update and offers a fresh install beside the real app, with an empty
  database.
- **The publish caveat splits in two, and only the Android half has expired.** An APK has
  ridden every Release since 0.3.0, published on 2026-07-29, so "a Release carries one" is
  now observed. What is still reasoned rather than watched, on both phones, is the update
  channel: nobody here has seen Obtainium find a *new* Release and install it over an
  existing copy, and on iOS nothing at all has been published — no Release has carried an
  `.ipa`, and SideStore has not been watched taking one version over another **in place**
  with the pairing intact. The iOS work does not close until it has.
- **The iOS consequences below describe shipped code.** The seven-day Personal Team
  certificate, the three-app cap and free-team signing that cannot live in CI are the terms
  this client runs under today, not the expected shape of one that might be built.

Neither mobile store is in play: there is no Google Play account and no Apple
Developer Program membership, and we declined to buy either. Android is signed with
our own keystore and published as a universal APK on the same GitHub Release as
everything else; iOS is published as an **unsigned** IPA and signed on the device
itself by a free Apple Personal Team. Installation and updating are delegated
entirely — Obtainium watches the releases page on Android, SideStore reads a source
JSON committed to this repository on iOS — and **the mobile client ships no update
code at all**.

This continues ADR 0005 rather than contradicting it. That record declined to pay
Apple $99 for notarization and built `tauri-plugin-updater` for one specific reason:
an unsigned `.dmg` fetched by a browser is quarantined, and an update fetched through
the app's own HTTP client is not, *"turning Gatekeeper from a tax on every release
into a tax on first install."* Neither mobile OS has a quarantine attribute, so that
rationale simply does not reach them. Worse for the updater's case: Android's package
manager already refuses any update not signed by the same certificate, which is
strictly stronger than a minisign public key compiled into the binary, and a
sideloaded iOS app has no route to install an IPA at all.

The pleasant consequence is worth stating, because ADR 0005 had to disclose the
opposite. That record's most uncomfortable sentence was that *"an
end-to-end-encrypted, self-hosted clipboard now contacts github.com when it
launches."* A phone does not. With no updater, **the mobile client's only network
counterparty is the relay** — the property the desktop had to give up, recovered for
free on the platform where it matters more. It is why the **Update Source** glossary
entry now distinguishes a desktop, which asks, from a phone, which never does.

## Consequences

**There is now a second irreplaceable secret.** ADR 0005 calls the minisign private
key *"the single most load-bearing file in this project's operations."* The Android
keystore joins it: Android pins the signing certificate, so losing the keystore means
no existing install can ever be updated again — only uninstalled and replaced. It gets
the same drill, and the drill is written down **once**, in
[ADR 0005's Consequences](0005-unsigned-downloads-signed-updates.md#the-signing-key-drill),
where the first key's already was: one offline directory, one habit, both keys, with
`ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` and
`ANDROID_KEY_PASSWORD` alongside `TAURI_SIGNING_PRIVATE_KEY` in Actions secrets.

**The iOS tax is recurring, and that is the sharpest difference from ADR 0005.**
There, Gatekeeper became "a tax on first install". Here a free Personal Team
certificate lasts **seven days**: when it lapses the app stops launching until
re-signed. SideStore automates the refresh on the device; Sideloadly's daemon needs
the PC on the LAN. A free team also caps three apps at a time and roughly ten app
IDs a week — which is one reason [ADR 0007](0007-a-phone-only-acts-when-you-act.md)
ships no Share Extension, since an extension would consume a second slot before its
missing entitlements even mattered.

**Free-team signing cannot live in CI.** It is interactive — Apple's 2FA stands in
the way — and there is no App Store Connect API key for a free account. So the macOS
runner builds an unsigned IPA and signing happens on the device or the PC — which
means the iOS path can never be dry-run in CI, and iOS build health has to be
verified locally.

> **Corrected, 2026-08-01.** This paragraph originally read "tied to an Apple ID in
> Xcode". That is false, and it mattered: it implied a Mac was required and was one
> reason the iOS half was deferred. [xtool](https://github.com/xtool-org/xtool) signs
> with a free Personal Team from Linux or WSL with no Xcode involved, so the PC in
> "the device or the PC" can be the Windows box. The conclusion survives on the
> remaining two clauses.

**One version, one Release, four artifacts.** `tauri.conf.json` remains
authoritative and `check-versions.mjs` grows to assert the Gradle `versionName` and
the iOS `CFBundleShortVersionString` agree with it, extending a mechanism that already
works rather than inventing a second one. A single Release carries the `.dmg`, the NSIS
`.exe`, the universal `.apk`, the unsigned `.ipa`, `latest.json` and the SideStore
source JSON, so the two manifests pin tag URLs into the same release and cannot
disagree. Because the core is shared ([ADR 0006](0006-one-protocol-three-shells.md)),
a single version also makes it impossible to fix the protocol for one client and
forget the other. The price is that a mobile-only fix publishes a desktop release
offering nothing to desktops, and ADR 0005 rules out softening that by marking 0.x
builds as prereleases — that would permanently 404 the updater endpoint.

Two details of the SideStore source JSON are load-bearing and easy to get wrong: it
needs a top-level `downloadURL` in addition to the one inside `versions[]`, or
SideStore rejects a file AltStore accepts; and `version` must match
`CFBundleShortVersionString` exactly, case-sensitively.

## Considered Options

**Pay Apple the $99.** Named because it is a better engineering answer than the one
chosen, and the choice was financial. It would end the seven-day tax outright —
certificates last a year — and unlock App Groups, Keychain Sharing, Background Modes
and APNs, which is the only route to the instant delivery ADR 0007 gives up. Rejected
on cost, and on consistency: ADR 0005 already declined to pay Apple for this project.

**An in-app updater on Android**, mirroring the desktop's Settings surface via
`PackageInstaller`. Rejected because it reintroduces exactly what delegation removes:
the app would contact github.com, so the README threat model grows a counterparty
needing the same disclosure and opt-out toggle ADR 0005 demanded — all to duplicate a
signature check Android performs for free.

**A self-hosted F-Droid repository.** The most correct Android answer: a real signed
index and a real client. Rejected as infrastructure to stand up and keep running for
two devices owned by one person, buying nothing over Obtainium until Sharepaste has
users who are not us.

**Manual downloads only, no source JSON.** Rejected in ADR 0005's own words: *"an
update that only reaches people who go looking for it is most of the reason not to
build an updater at all."*
