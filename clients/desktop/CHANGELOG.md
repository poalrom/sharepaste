# Changelog

One `## <version>` section per published release of the desktop app, newest
first.

A section is shown twice — on its release page and in the in-app update prompt —
so write it for someone deciding whether to install, not from the commit log.
`release-gate` refuses to publish a version that has no section here.

## 0.3.0

Sharepaste runs on Android now, and the pairing pane shows the short code as a
QR so a phone can join without anyone typing 122 characters.

- **An Android client.** Scan the QR in a desktop's pairing pane and the phone
  joins the same user: same history, and an entry can go back onto the phone's
  clipboard. Offering from the phone puts it here. Android 10 and later,
  installed and updated with Obtainium — the phone carries no update code of its
  own, so the only thing it ever talks to is your relay.
- **What a phone deliberately will not do**, because it reads as a bug
  otherwise: nothing arrives in the background, ever. No mobile operating system
  lets a backgrounded app watch the clipboard, so something copied on this
  machine does not reach the phone until the app is opened or Recall Latest is
  tapped. Both verbs are on an ongoing notification, so neither needs the app
  opened.
- **A phone can only reach a relay over HTTPS** with a publicly trusted
  certificate. Desktops paired to a plain-HTTP relay are unaffected and keep
  working exactly as before.
- The pairing pane keeps the typed code beside the QR, as the fallback for a
  camera that is missing or refused.
- Two sync fixes that matter on this machine too. An entry that failed to
  decrypt no longer causes the entries after it to be skipped. And an entry
  captured while the relay was unreachable now appears in your own history as
  soon as it uploads, instead of waiting for the next reconnect.
- Underneath, the protocol moved out of the desktop app into one core that both
  clients share, so a fix lands on both at once. Nothing to re-pair, no setting
  changed, and your history stays where it is.

macOS builds are Apple Silicon only.

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
