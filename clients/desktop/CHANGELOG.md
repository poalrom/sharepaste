# Changelog

One `## <version>` section per published release of the desktop app, newest
first.

A section is shown twice — on its release page and in the in-app update prompt —
so write it for someone deciding whether to install, not from the commit log.
`release-gate` refuses to publish a version that has no section here.

## 0.9.0

**Android only.** Nothing on this machine changes and the relay is not involved, so if you do
not use the phone there is no reason to install this. The phone can now be told to stop
confirming an offer, the same way it can already be told to stop naming what it recalled.

- **A second switch: `CONFIRM OFFERS`.** Under `THIS PHONE` on the settings screen, on by
  default, beside the one that has been there since 0.5.0. Turn it off and offering the
  clipboard — from the button, from the notification, or by sharing text to Sharepaste from
  another app — stops putting a line over whatever you were looking at. The offer itself is
  unchanged: the entry is still captured, still encrypted, still queued or sent, and still
  arrives on your other machines. Only the message goes.
- **One switch per verb, so neither speaks for the other.** Silencing your offers says nothing
  about what a recall may say, and the reverse. They are off for different reasons — a recall
  hands you back something you did not choose and its message names it, while an offer names
  nothing and is only the app talking over you — and one control would have made you accept
  both to get either.
- **A repeat still speaks, on purpose.** Offering text the phone already has does not save
  anything; it moves the entry you already had to the top. That one still says `ALREADY SAVED`
  whatever the switch is set to, because going quiet there would look exactly like the offer
  that did save something, on a list you can check a second later.
- **Nothing that needs doing has been silenced.** A refusal, a warning that a recall came off
  this phone's own copy rather than the relay, an unpaired phone, a failure — all unchanged.
  Neither switch can take away a message that exists to be acted on, and the ongoing
  notification still shows no entry text at all.

## 0.8.3

**Only this machine changes, and only what it says about itself.** A patch over 0.8.2 for one
wrong word on the popover. Nothing on the phone moves, the relay is not involved, and no
entry, key or pairing is touched — if your popover has always read ONLINE, this changes
nothing for you.

- **The popover said CONNECTING at a relay it was already talking to.** On most launches the
  line along the bottom of the popover settled on `CONNECTING` and stayed there for the rest
  of the session — amber, pulsing, and wrong — while the main window one keystroke away read
  `ONLINE` about the same pairing, and entries kept arriving in the popover's own list while
  it claimed to still be looking. The popover asks once what state the connection is in and
  listens for changes after that; it starts up in the same instant the connection does, so
  the answer it got was `CONNECTING` and the news that connecting had finished arrived in the
  moment before it was listening. It listens first and asks second now, so the two windows
  cannot disagree about one pairing, and the count of copies waiting to upload — which the
  popover learned the same way — can no longer stick at a number it has since left behind.

## 0.8.2

**Relay only, and this download does not carry it.** The one fix in this release is on the
server you run yourself, which is not attached to a Release and never has been — you get it
by rebuilding your own container from this commit. Nothing you can see changes on the desktop
or on the phone. Install it so your machines report the same version as the relay they talk
to; there is no other reason, and no order to do anything in.

- **A pairing slot that had run out of guesses could still be used once more.** Adding a
  device to an existing pairing goes through a short-lived slot on the relay that burns itself
  after too many wrong secrets. One of the two routes that accept a secret was not counting
  against that limit, so a slot already at the cap was refused by the pairing endpoint and
  still accepted by the device endpoint — one extra attempt, on a route that should have said
  no. It says no now, and the limit is enforced by the code that owns the slot rather than
  re-derived by each route that reads it. Your existing pairings are unaffected and there is
  nothing to redo.

Underneath, a large amount of this codebase moved without changing what it does: the queue of
acts a device owes the relay, the network seam under the client facade, the live-history
wiring the desktop windows share, the entry row both desktop surfaces draw, and three
separate places on the relay that had each been deciding the same thing differently. If you
are on 0.8.1 and everything works, this changes nothing for you.

## 0.8.1

**Android only.** A patch over 0.8.0's reader, which showed you an entry's whole text and
then gave you no way to take any of it. Nothing on this machine changes and the relay is not
involved; if you have not installed 0.8.0 yet, this is that reader as it should have shipped.

- **Take part of an entry, not only all of it.** Press and hold the text under an opened row
  and it selects, the way text selects everywhere else on the phone, with Copy and Select all
  in the menu Android puts over it. Recall still puts the whole entry on your clipboard; this
  is for the one line out of a config, or the host in the middle of a connection string, that
  until now meant recalling the lot and cutting it back down after you pasted it.
- **Copying out of a row records nothing.** Nothing moves to the top of your history, no
  other machine is told you looked, and what you copied is not captured unless you press
  OFFER yourself — the phone has never watched your clipboard and still does not.
- **The line on the row, and the app's own writing, stay unselectable.** The row's single
  line is what you tap to open it, and the sentence a genuinely empty entry opens onto is the
  app talking rather than anything you copied, so there is nothing there worth taking.

## 0.8.0

**Android only.** Nothing on this machine changes, and the relay is not involved — so if you
do not use the phone there is no reason to install this. The phone can *read* an entry now
rather than only pick one.

- **Tap a row to read the whole thing.** A row has room for one line, so anything longer
  than the phone is wide was something you could recall but never read: three `ss://` URLs
  that differ at character 60 looked identical, and the only way to tell which was which
  was to put one on your clipboard and paste it somewhere to look. Tapping a row now opens
  that entry's whole text underneath it — every line, indented as you copied it — and
  tapping the row again closes it. The line on the row does not change, so a long history
  still scans as one screenful.
- **Reading is not using.** Opening an entry puts nothing on your clipboard, does not move
  the entry to the top of your history, and tells your other machines nothing. Recall is
  still the only thing that does any of that, which means you can go through a history
  looking for something without rearranging it.
- **A row you cannot read says which kind of nothing it is.** An entry that reached this
  phone encrypted with a key it does not have offers nothing to open, as before. An entry
  whose text really is empty now says so when you open it, rather than opening onto a blank
  panel that looks like something failed.

## 0.7.1

**Only the desktop changes. Your relay needs nothing.** A patch over 0.7.0, and all of it
is what the app was calling things: the build it says it is, and the way it writes the
name of a person and a relay. Nothing on the phone moves, no relay contract is touched,
and there is no order to update anything in.

- **The version in the corner is the build you are running.** The main window's rail read
  `v0.1.0` on every build from 0.2.0 through 0.7.0, because it took the number from a
  manifest nobody bumps — while Settings, one pane over, asked the app itself and told you
  the truth. The rail now reads the same manifest the release is cut from, so the two panes
  cannot disagree again.
- **An address is written the way an address is written.** Four places spelled you and your
  relay as `alice @ relay.example`, with spaces around the `@` that no address has: the
  clear-everything sentence in Settings, the pairing picker above History, and both the
  identity line and the forget confirmation on a pairing card. The footer had it right all
  along, so the same app spelled the same thing two ways on two panes at once.
- **A pairing card names the person, not an id.** Those same two lines on the card called
  you by the opaque id the relay filed you under — `u-lab@relay.lab` on one pane while the
  footer, Settings and the History picker all said `alice@relay.lab`. Both now say your
  username, and it still tells two same-named pairings apart, because the relay beside it
  is what always did that. The id appears only for a pairing whose username has not
  reached this machine yet.

## 0.7.0

**Both clients. Your relay needs nothing.** Copying something while your relay is out of
reach used to show you nothing at all: the entry existed, queued and encrypted, but your
history looked as though the copy had never happened — you could not search it, recall it,
or change your mind about it until the relay came back. Every copy is now a row the moment
you make it, on both clients, and the relay is not involved in the change, so there is no
order to update things in.

- **An offline copy is in your history straight away**, at the top, tinted the same amber
  as the count of things waiting. You can find it with Filter, read it in full, and recall
  it onto your clipboard — all of that now works on something the relay has never seen,
  because the copy is on the machine you made it on.
- **You can take a copy back.** Deleting one of these removes the entry *and* the upload
  that was going to publish it, with nothing in reach and no trace anywhere else — the
  queue survives a force-quit, so before this there was no way to stop a mistaken copy
  from reaching the relay eventually. Deleting an entry the relay already has is unchanged.
- **A refused copy says why, and offers to try again.** If your relay turns something down
  for what it is — one entry over its size limit — that row now tells you so in the
  relay's own words and carries a **Resend**. Nothing queues up behind it while it waits
  for you, which is the other half: one oversized copy used to be able to hold up
  everything made after it.
- **Being out of reach is never a refusal.** No copy is ever given up on for being
  unreachable, and none is dropped to keep the queue short. An earlier limit quietly
  discarded the oldest thing waiting once a thousand had piled up — deleting clipboard
  content that had reached nowhere else, and telling only the log about it.
- **Nothing jumps when the relay comes back.** What you saw before a flush is what you see
  after it: same rows, same order, same place in the list, and the entry you had selected
  stays selected. The relay stamps each copy exactly where your machine already showed it.
- **A long offline stretch is not truncated.** The hundred entries kept per device, and the
  thirty days, now measure only the entries your relay has ordered. A hundred and fifty
  copies made offline are a hundred and fifty rows, because throwing one away to keep a
  number tidy would be throwing away the only copy of it.

## 0.6.1

**Only the phone changes.** A patch over 0.6.0's Filter, which shipped as a boxed field
sitting on a bar rather than as the bar itself. Nothing on the desktop moves and the relay
is not involved, so if 0.6.0 is working for you this is a phone update and nothing else.

- **The field is the whole bar now**, edge to edge, and draws no frame around itself — not
  even while you are typing in it. The cursor was always the part that said where you were.
- **The `✕` sits over the Recall buttons**, in the column they make down the right-hand
  side, instead of a gutter's width inside it.
- **Back finishes the job.** Clearing a filter with the back gesture used to leave the
  cursor in the field: a bar reading `FILTER HISTORY` that was still quietly being typed
  into, and one more press before the app would close. The `✕` still leaves you in the
  field, because a thumb already there is usually about to type something else.

## 0.6.0

**Both clients, and your relay with them.** Your history is now ordered by when you last
*used* an entry rather than when it was captured, and both clients can narrow it to what
you type. **Update the relay before the clients** — half the ordering is its work, and a
relay still on the old schema leaves every recall changing nothing.

- **Recalling an entry moves it to the top, on every device.** A recall used to leave no
  trace at all: the text reached your clipboard and the list looked exactly as it had a
  second earlier, so the thing you reach for ten times a day sank underneath things you
  pasted once. It is the same entry that moves — same text, same origin, same capture
  time; nothing is copied and nothing new is made.
- **Copying something you already have counts as using it.** Both clients used to treat
  that as a refusal — this machine said `ALREADY HERE` — which was true about the storage
  and no use to you. It now moves the entry you already have to the top, and the phone
  says **Already saved** rather than reporting a failure.
- **Both retention limits measure from last use.** The hundred entries a relay keeps, and
  the thirty days it keeps them for, now count from when you last used an entry, so
  something in regular use is never the thing dropped to make room.
- **Type to narrow your history.** New on the phone: a field under the contact line that
  hides every row whose text does not contain what you typed, with a count of what is
  left and a `✕` to clear it. It never asks the relay, so it can only find what has
  already reached that phone — and it matches the whole entry rather than the one line
  you can see, so a word buried on an entry's third line still finds it. On this machine
  the box you already had is now called **Filter**, which is what it always did.
- **The phone's `RECALL LATEST` is now `RECALL FIRST`.** It hands over the first row of
  the list in front of you, which with a filter on need not be the newest entry — and
  that is the row already marked in the list, so the button and the list cannot disagree
  about which entry you are about to get. It takes what the phone already holds instead
  of fetching, so it can no longer hand you something you never saw. The recall on the
  notification is unchanged: still the last entry used, still fetched.
- **The phone shows a hundred entries instead of fifty**, which is everything it keeps.
- **Ages on this machine read from last use.** The age column runs down the list in order
  again instead of showing a three-week-old entry above a two-minute-old one, and an
  entry's detail pane says `USED` beside `CAPTURED` when the two differ.

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
