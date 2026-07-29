# Sideloaded, and not self-updating

**Deferred, 2026-07-29 — iOS is not shipped.** The decision below stands as written and
nothing in it was overturned; the effort it was written for delivered the Android half
only. It is left intact because its reasoning still holds, but a reader should not infer
any of the following from it:

- **A Release carries three client artifacts, not four**: the macOS `.dmg`, the Windows
  NSIS `.exe` and the universal `.apk`, beside `latest.json`. There is **no `.ipa` and no
  SideStore source JSON**, and nothing in this repository generates either. There is no
  Xcode project at all.
- **`check-versions.mjs` asserts no `MARKETING_VERSION`**, there being nothing to read one
  from. What it does check, with `clients/desktop/src-tauri/tauri.conf.json` still
  authoritative, is `clients/desktop/package.json`,
  `clients/desktop/src-tauri/Cargo.toml`, `clients/core/Cargo.toml`,
  `clients/mobile/ffi/Cargo.toml` and the `versionName` in
  `clients/mobile/android/app/build.gradle.kts` — the two Rust manifests because their
  crates compile into `libsharepaste_ffi.so` and ride inside the APK.
- **No publish has exercised any of this.** The Android pipeline was built and verified
  locally — a signed universal APK, an in-place update over `adb install -r`, a
  differently-signed APK refused — but no Release carrying an APK has been published, so
  "Obtainium finds it" is reasoned from Obtainium's own source rather than observed.
- The iOS consequences below — the seven-day Personal Team certificate, the three-app cap,
  free-team signing that cannot live in CI — describe the expected shape of an iOS client
  whenever one is built. They describe no code that exists.

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

**Free-team signing cannot live in CI.** It is interactive, tied to an Apple ID in
Xcode, and there is no App Store Connect API key for a free account. So the macOS
runner builds an unsigned IPA and signing happens on the device or the PC — which
means the iOS path can never be dry-run in CI, and iOS build health has to be
verified locally.

**One version, one Release, four artifacts.** `tauri.conf.json` remains
authoritative and `check-versions.mjs` grows to assert the Gradle `versionName` and
the Xcode `MARKETING_VERSION` agree with it, extending a mechanism that already works
rather than inventing a second one. A single Release carries the `.dmg`, the NSIS
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
