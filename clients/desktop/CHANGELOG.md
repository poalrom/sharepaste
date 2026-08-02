# Changelog

One `## <version>` section per published release of the desktop app, newest
first.

A section is shown twice — on its release page and in the in-app update prompt —
so write it for someone deciding whether to install, not from the commit log.
`release-gate` refuses to publish a version that has no section here.

## 0.5.1

**Icons.** The phone never had one of its own; this machine's changes colour. Nothing else
moves — no behaviour, no settings, nothing touched on either side of a pairing.

- **The phone shows Sharepaste's icon.** Every version so far has worn Android's default
  green robot — on the home screen, in the share sheet and in the app switcher — because
  the app shipped without a launcher icon at all. It now carries the same three-ribbon
  mark this machine does, and a themed version of it for launchers that recolour icons to
  match the wallpaper.
- **This machine's icon is cyan, not green.** The ribbons were drawn in the colour
  Sharepaste uses to mean *in contact*, which is a status and has no business standing in
  for the product. Same mark, same shape, new colour in the taskbar, the dock and the
  browser tab. Windows keeps its own thumbnail cache, so the old one may linger there
  until it refreshes.

## 0.5.0

**Android only.** Nothing on this machine changes. The phone now says what a recall put
on your clipboard, and several controls that did nothing when pressed now work.

- **A recall tells you what arrived.** Pressing Recall used to be silent: nothing on the
  phone said whether it had worked, or which entry you were about to paste. It now shows
  the entry's first line for a few seconds, so you can see what you have before you paste
  it into a message. **Android's own paste preview works again too** — Sharepaste used to
  blank it out to a row of dots, which on a phone that shows one reads as the app being
  broken. The trade is real and worth knowing: the recalled text is now legible in your
  keyboard's clipboard history until that clears itself, the same as anything else you
  copy. All of it needs your unlocked phone in somebody's hands.
- **You can switch that off.** *Settings → This phone → Show what was recalled*. Off means
  the entry still goes to your clipboard and Sharepaste simply does not say what it was —
  for when the phone is not the only thing in the room. Warnings are not affected: a
  recall that fell back to a cached entry still says so, because that one you have to
  know about.
- **The entry you just offered is where you can see it.** Two success banners used to
  appear at the top of the history and push the newest entry off the bottom of the
  screen — the one Recall Latest will hand over. Both banners are gone, and the list now
  moves to the newest entry when one arrives.
- **Swiping an entry and tapping Delete deletes it.** The red panel the swipe uncovered
  did nothing at all when pressed; the only way through was to complete the swipe. Both
  work now. A tap on an unswiped row still does nothing, which is the point of the swipe.
- **The back gesture no longer closes the app.** Back from the settings screen returns to
  your history, and back out of the pairing screen returns to settings. Back from the
  history still leaves, as it should.
- ***Pairings* is now *Settings*,** because it holds more than pairings. A pairing card
  shows the relay address in full instead of truncating it behind an identifier you
  cannot use, and the note about naming the phone has moved to where the name is actually
  being chosen.
- **Try again after a failed pairing starts over properly.** It cleared the error and left
  the dead code sitting in the field, so pairing again re-sent a code the relay had
  already thrown away, and the camera stayed off. The field is now empty and the
  viewfinder is live; the name you gave the phone is kept.
- **Fewer words in the way.** The three pairing steps are about a third of their old
  length. Two lines that a phone cannot act on are gone: a countdown it could never
  actually count, and a rule about relay addresses you never get to choose. On a phone
  that has not finished starting up, the top of the history no longer shows a long
  internal identifier where your name goes.

## 0.4.2

**Desktop only.** One fix to the pairing screen. Nothing on Android changes.

- **A pair code that runs out now hands you a new one.** The code stayed on screen after
  its two minutes were up — square, digits and a clock reading 0:00 — under a red line
  saying to generate a new one, with nothing on the panel that would. Closing the panel
  and opening it again was the only way, and the line did not say so. The dead code is
  now replaced by the news that it expired, naming the code so there is no doubt which
  one, and a **New code** button that puts a live one up in its place.
- **The two pairing panels no longer open on top of each other.** Opening *Add a pairing*
  while a card's *+ Device* panel was up left both on screen showing the same code, the
  second time under a heading naming no pairing.

## 0.4.1

**Android only.** A freshly installed phone could not use its camera until the app was
closed and reopened, and the viewfinder was drawn over the rest of the pairing screen.
Nothing on this machine changes.

- **Granting camera access now works the moment you grant it.** The pairing screen asks
  for the camera on first sight, and the answer used to arrive after the screen had
  already written the refusal: "Sharepaste cannot use the camera" stayed there until the
  app was closed and reopened. It now notices straight away, and it keeps noticing — turn
  camera access on in Settings, come back, and the viewfinder is waiting. There is a
  **Check again** button beside the refusal too, for anyone who would rather press
  something than trust that.
- **Scanning the code no longer needs the phone named first.** The square is the first
  thing anyone points a phone at, so a scan used to fail on the empty name and spend a
  code that is only good for two minutes on a message asking for it. A scan now fills the
  pairing code in and puts the camera away; type the name and press Pair. Clear the code
  field to scan a different one.
- **The viewfinder stays inside its own frame.** It was painted over the step above it and
  the code field below, hiding the two things you had to read.

## 0.4.0

**Android only.** Nothing on this machine changes — no new desktop feature, no fix, no
setting moved. If you do not use the Android app there is no reason to install this.

- **The Android app was rebuilt to look like this one.** Same dark console, same
  vocabulary, same colours, so a phone and a desktop showing the same history no longer
  look like two products.
- **What a phone will and will not do is now pinned to the top of the screen** instead of
  sitting at the top of the list, where it scrolled away the moment you had more than a
  screenful of entries. "Nothing arrives while this is closed" is always visible, and one
  tap opens the full explanation.
- **Whether the phone is in contact with the relay is always on screen.** It used to
  appear only when something was wrong, which on a phone is almost always — a phone syncs
  only while you are looking at it, so that read as permanently broken.
- **Recall Latest is now the main button**, and the entry it will hand over is marked in
  the list, so you can see what you are about to paste before you press it.
- **Deleting an entry on the phone takes a swipe** rather than a tap next to Recall. A
  delete reaches every paired device and cannot be undone, and the two used to be a
  thumb's width apart.

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
