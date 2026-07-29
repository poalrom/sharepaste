# Unsigned downloads, signed updates

Publishing releases forced the signing question, and we declined to sign:
notarization is $99/yr plus certificate rotation, and the audience for a tool
you stand up behind your own TLS can be asked to clear a Gatekeeper prompt. But
an unsigned `.dmg` is quarantined by the browser that fetched it — the same
`xattr -dr com.apple.quarantine` that `Makefile:58` has always run locally,
except a downloader does not have the Makefile. So we shipped
`tauri-plugin-updater` alongside: the updater fetches through the app's own HTTP
client, which never sets the quarantine attribute, turning Gatekeeper from a tax
on every release into a tax on first install. Unsigned and self-updating are not
in tension here; the second is what makes the first survivable.

The price is stated plainly because it is the surprising part: **an
end-to-end-encrypted, self-hosted clipboard now contacts github.com when it
launches**, disclosing an address, an OS and a version to a third party that no
part of this product previously involved. `README.md` says so in its threat
model, `CONTEXT.md` names the counterparty **Update Source** so it can never be
confused with the **Relay**, and the check can be switched off in Settings.

## Considered Options

**Notarize macOS.** The clean answer, and the one to revisit the moment the
audience stops being people who run `docker compose`. Rejected on cost and on a
worse property than cost: a signing path that exists only in CI can only ever be
debugged on main.

**Manual checks only, no automatic call.** This keeps the self-hosted claim
absolute, and it would still have bought the entire Gatekeeper win — a
user-initiated download through the app is exactly as unquarantined as an
automatic one. Rejected because an update that only reaches people who go
looking for it is most of the reason not to build an updater at all.

**Ask on first run.** Rejected: there is no first-run flow to hang it on beyond
pairing, and a consent modal is answered without reading.

## Consequences

The updater's minisign **public key is compiled into every shipped binary**. Lose
the private half and no existing install can ever auto-update again — each one
has to be replaced by hand. It lives in Actions secrets, password-protected,
with an offline backup, and that backup is the single most load-bearing file in
this project's operations.

### The signing-key drill

There are now **two** irreplaceable keys, and they are backed up by one habit
rather than two. [ADR 0008](0008-sideloaded-and-not-self-updating.md) added the
second — the Android release keystore, which Android's package manager pins, so
losing it means no installed copy can ever be updated again either. Its drill is
recorded here, beside the first, because two drills in two places is one drill
that gets done.

Both private halves live in **one offline directory** — an encrypted volume, not
a laptop's home directory and not any repository, with a second copy on separate
physical media kept somewhere else. Losing that directory is unrecoverable for
every existing install of both clients. The directory holds, beside each key, the
passphrase that opens it and the exact command that rebuilds the Actions secret
from it; a key whose passphrase is only in someone's head is a key that is
already lost.

The Actions secrets are derived from those files and are never the only copy:

| secret | from |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | the minisign private key |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | its passphrase |
| `ANDROID_KEYSTORE_BASE64` | `base64 -w0 sharepaste-release.jks` |
| `ANDROID_KEYSTORE_PASSWORD` | the keystore passphrase |
| `ANDROID_KEY_ALIAS` | the key alias inside the keystore |
| `ANDROID_KEY_PASSWORD` | the key's passphrase |

Rotating either key is not an upgrade path, it is a migration: a minisign
rotation strands every install that has not yet taken an update signed by the old
key, and a keystore rotation strands **every** Android install outright — each
phone must uninstall and reinstall, losing its pairings. Treat both as
never-rotated for the life of the application id.

The endpoint is the `releases/latest/download/latest.json` redirect, which
resolves only to non-draft, non-prerelease releases. That is why 0.x builds
publish as full releases rather than prereleases: marking them honestly would
404 the endpoint permanently.

The update surface is the Settings section plus a tray item that exists only
while an update is pending. Not the popover: [ADR 0002](0002-popover-is-a-picker.md)
rules that informational chrome does not get to cost rows in the nominal case,
and "an update exists" is absent nearly always. The check is automatic; the
download and the restart never are.

**Contact** does not count update traffic. It is defined as evidence of a live
connection to the relay, and now that a device has two counterparties, wiring
the wrong one into it would make the popover's degraded strip lie.
