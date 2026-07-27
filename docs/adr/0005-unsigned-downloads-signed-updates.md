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
