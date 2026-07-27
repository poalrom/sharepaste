# Changelog

One `## <version>` section per published release of the desktop app, newest
first.

A section is shown twice — on its release page and in the in-app update prompt —
so write it for someone deciding whether to install, not from the commit log.
`release-gate` refuses to publish a version that has no section here.

## 0.2.0

The first published release. Sharepaste keeps a clipboard in sync across your
own machines, end to end encrypted — the relay you host stores ciphertext it
cannot read.

- Pair a machine to a relay with an invite token, or to a machine you have
  already paired using a short code. One machine can hold several pairings and
  syncs to one of them at a time.
- Copy on one machine, pick it from the tray popover on another.
  `Cmd/Ctrl+Shift+V` summons the picker from anywhere; rebind or clear it in
  Settings.
- The main window reads entries in full and filters them, and is where
  pairings, devices and capture rules live.
- Copies made in 1Password and Bitwarden are never captured. Add your own apps
  to the deny-list.
- Entries captured while the relay is unreachable queue on the device and
  upload when it answers again.
- The app can ask github.com for a newer release at launch and install it on
  click. It never downloads or restarts on its own, and the check can be
  switched off under Settings → Updates.

macOS builds are Apple Silicon only.
