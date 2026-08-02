import Foundation

/// Every word the app says.
///
/// The vocabulary is the project's own and is not negotiable per-screen: Entry,
/// Preview, Pairing, Active Pairing, Offered Capture, Recall, Relay, Contact,
/// Origin. See `CONTEXT.md`.
///
/// Two registers, and which one a string is in is a design decision rather than
/// a typing habit. **Chrome, controls and telemetry are written in capitals** —
/// they are HUD readouts, scanned rather than read, and writing them in capitals
/// here rather than upper-casing them at render time keeps what is on screen
/// greppable and keeps a locale's casing rules out of it. **Whole sentences stay
/// sentence case**, at full length, wherever the app has something to explain:
/// the foreground-only note, the stale-Recall warning, the two erasures, the two
/// absent settings and what the one switch this phone has actually decides are
/// the load-bearing ones, and none of them is shortened to fit a band.
///
/// Plain `String` constants rather than a `.strings` catalogue, because there is
/// one locale and a catalogue would buy a build step and a lookup that can fail
/// at runtime. The Android side needs `R.string` because a resource id is how a
/// `Notice` travels without carrying its own words; here a `Notice` can carry
/// the `String` itself, and does.
enum Strings {

    // -- Pairing ---------------------------------------------------------------

    static let pairTitle = "PAIR THIS PHONE"
    /// The step numbers are drawn beside the headings, not written into them.
    static let pairStepName = "01"
    static let pairStepScan = "02"
    static let pairLabelHeading = "NAME THIS PHONE"
    /// The one load-bearing sentence that moved out of the deleted settings note
    /// in `0.5.0`. It lands here, at the naming step, because here it is
    /// actionable; the Settings Screen deliberately does not repeat it.
    static let pairLabelExplainer =
        "Shown beside every Entry from this phone. It cannot be changed later."
    static let pairLabelField = "This phone\u{2019}s name"
    static let pairLabelPlaceholder = "iPhone in my pocket"
    static let pairScanHeading = "SCAN THE CODE ON YOUR COMPUTER"
    static let pairScanExplainer =
        "On your computer, open Sharepaste and choose Pair a device."
    static let pairNeedsAName =
        "Name this phone first — the code is only good for two minutes, so we would rather ask "
        + "now than lose it."
    static let pairTypedHeading = "OR TYPE THE CODE"
    static let pairTypedExplainer = "Case and dashes do not matter."
    static let pairTypedField = "Pairing code"
    static let pairButton = "PAIR THIS PHONE"
    static let pairWorking = "PAIRING\u{2026}"

    // The viewfinder's own chrome.
    static let pairViewfinderTitle = "VIEWFINDER"
    static let pairViewfinderCode = "BACK CAMERA"
    static let pairViewfinderHint = "POINT AT THE SQUARE ON THE PAIR A DEVICE SCREEN"
    /// What the viewfinder says once it has done its job. A scan fills the code
    /// field rather than pairing, so this has to point at the field, say what is
    /// still missing — the name — and say how to get the camera back.
    static let pairCodeScanned =
        "Code read. It is in the field below — name this phone, then Pair. Clear the field to "
        + "point the camera at another code."

    // The three camera and code failures, each said in its own words.
    static let cameraPermissionRefused =
        "Sharepaste cannot use the camera, so it cannot read the square code. You can turn "
        + "camera access on in Settings, or type the code underneath the square instead — that "
        + "works just as well."
    /// Beside the refusal, because the app notices a grant on its own: it
    /// re-reads the permission whenever it comes back to the front and while this
    /// sentence is on screen. The control is for the person who would rather not
    /// trust that.
    static let cameraRecheck = "CHECK AGAIN"
    /// iOS has no Settings deep link that lands anywhere but this app's own page,
    /// which is exactly where the person needs to be, so unlike Android this one
    /// can offer to take them there.
    static let cameraOpenSettings = "OPEN SETTINGS"
    static let cameraAbsent =
        "This phone has no camera Sharepaste can use, so there is nothing to point at the "
        + "square code. Type the code printed underneath it instead."
    static let pairCodeExpired =
        "That code has already expired. A pairing code is only good for two minutes, on purpose "
        + "— anyone who reads it over your shoulder can pair with it. Ask your computer for a "
        + "fresh one and scan again."
    static let pairNotACode =
        "That is not a Sharepaste pairing code. Check you scanned the square on the Pair a "
        + "device screen, or retype the code underneath it."
    static let pairRefused =
        "Your computer\u{2019}s Relay turned this phone away. The code may already have been "
        + "used by another device — ask for a fresh one."
    static let pairUnreachable =
        "Sharepaste could not reach the Relay. Check this phone has a working connection, then "
        + "try the code again — you have two minutes from when your computer showed it."
    /// The one failure that shows the core's own words underneath it. The core
    /// names the relay's address and why cleartext is refused, and no wording
    /// here could be as specific.
    static let pairInsecureRelay =
        "That code points at a Relay served over plain http://. Sharepaste will not send this "
        + "phone\u{2019}s access token over a connection anyone on the network can read, so it "
        + "refused before connecting."
    static let pairFailed =
        "Pairing did not go through. Ask your computer for a fresh code and try again."
    /// Whatever this is called, it resets the *whole* attempt — the spent code,
    /// the scan latch and the failure — and leaves the Device Label alone.
    static let pairDismiss = "TRY AGAIN"
    static let pairFailedBadge = "DID NOT PAIR"

    // -- History ---------------------------------------------------------------

    static let historyTitle = "SHAREPASTE"
    /// The User slot while the Relay has not yet said who this phone is. Never a
    /// raw `user_id`: a 36-character uuid on a cold start is chrome lying about
    /// what it knows.
    static let historyIdentityUnknown = "\u{2026}"
    static let historyEmptyHeading = "NOTHING HERE YET"
    static let historyEmptyBody =
        "Anything you copy on a paired computer shows up here as an Entry. Nothing has been "
        + "offered yet."

    /// `%@ @ %@` — the User, then the Relay host. Which Pairing is on screen.
    static func historyIdentity(user: String, host: String) -> String { "\(user) @ \(host)" }

    // A row. The Preview itself is the facade's and is never re-worded here.
    // Two parts, because the row draws two: the marker names the state in the
    // product's own word, and the sentence underneath says what it means.
    static let entryUndecryptableMarker = "\u{2298} UNDECRYPTABLE"
    static let entryUndecryptable = "SEALED WITH A KEY THIS PHONE DOES NOT HAVE"
    static let entryRecall = "RECALL"
    static let entryDelete = "DELETE"

    /// Shown only for an Entry captured on another Device. The Origin is a name a
    /// person chose and is never re-cased.
    static func entryOrigin(_ origin: String) -> String { "FROM \(origin)" }

    // The two verbs, on the open screen where the screen is the context.
    static let offerBar = "OFFER"
    static let recallLatestBar = "RECALL LATEST"
    static let noticeDismiss = "DISMISS"

    // The long titles, for Shortcuts, where there is no surrounding screen to
    // say what the verb acts on.
    static let offerIntentTitle = "Offer what I copied"
    static let recallIntentTitle = "Recall the latest Entry"

    // Every outcome is labelled with what it reports, in one or two words,
    // before the sentence that explains it.
    static let noticeOffered = "OFFERED"
    static let noticeRecalled = "RECALLED"
    static let noticeNotPaired = "NOT PAIRED"
    static let noticeCleared = "CLEARED"
    static let noticeForgotten = "FORGOTTEN"
    static let noticeFailed = "DID NOT WORK"
    // One per reachable refusal, naming what to do about it.
    static let noticeNothingToSend = "NOTHING TO SEND"
    static let noticeTooBig = "TOO BIG · 64 KB CAP"
    static let noticeAlreadyHere = "ALREADY HERE"
    static let noticeRefused = "REFUSED"
    static let noticeNothingToRecall = "NOTHING TO RECALL"

    static let offerQueued = "Offered. Sharepaste sends it to the Relay while this app is open."
    static let offerFailed = "Sharepaste could not offer that."
    static let actionUnpaired =
        "This phone is not paired yet, so there is nothing to offer to and nothing to recall. "
        + "Pair it with your computer first."

    // The three refusals an Offer can actually receive, each needing a different
    // thing done about it. A rejection with no reason is a button that does
    // nothing, which is the one outcome a person cannot act on.
    static let offerRefusedNonText =
        "There is nothing on the clipboard Sharepaste can send. It syncs text — copy a link, a "
        + "note or a code and offer that."
    static let offerRefusedTooLarge =
        "That is too big to send. An Entry can be up to 64 KB; put the long version somewhere "
        + "else and offer a link to it."
    static let offerRefusedDuplicate =
        "That is the same text you just offered, so there is nothing new to send. It is already "
        + "in your History."
    /// Unreachable by construction: the four remaining reasons describe Watched
    /// Capture, which a phone never performs, and the facade passes their inputs
    /// in inert. One sentence rather than four that can never be read, worded as
    /// the surprise it would be.
    static let offerRefusedUnreachable =
        "Sharepaste refused that for a reason that cannot apply to a phone, and nothing was "
        + "sent. This is worth reporting."

    static let recallDone = "On the clipboard. Paste it wherever you were going."
    /// The same outcome with the Preview named, for when Sharepaste is asked to
    /// say what arrived. The Preview is the facade's and is never re-worded here.
    static func receiptRecalled(preview: String) -> String { "On the clipboard: \(preview)" }
    /// The one sentence that may never be missing. Recall Latest always tries the
    /// Relay; when the try fails, the newest Entry this phone already had is
    /// still the best answer available — but it may be yesterday's link, and the
    /// person is the only one who can tell.
    static let recallFromCache =
        "Sharepaste could not reach the Relay, so this is the newest Entry it already had. "
        + "Anything you copied on your computer since then is not here yet — open Sharepaste "
        + "again once you have a connection."
    /// Its badge. The notice above is about what is *now on the clipboard*,
    /// rather than about what the app just did, which is why it is the one notice
    /// that wears a colour at all.
    static let recallFromCacheBadge = "MAY BE STALE"
    static let recallNothingToRecall =
        "There is nothing to recall. Either this History is empty, or that Entry is "
        + "Undecryptable and this phone holds no plaintext for it."
    static let recallFailed = "Sharepaste could not recall that."
    static let deleteFailed =
        "Sharepaste could not delete that Entry. It is still on the Relay, so it will come back "
        + "— try again once you have a connection."

    // Pending, surfaced. Sync is foreground-only, so an Offer made with no
    // connection waits for the next time the app is opened; a count nobody can
    // see is a count nobody comes back for. The number is drawn beside the
    // sentence as a readout rather than written into it.
    static func pendingCount(_ count: Int64) -> String {
        count == 1
            ? "Entry is waiting for the Relay. Sharepaste sends it while this app is open."
            : "Entries are waiting for the Relay. Sharepaste sends them while this app is open."
    }

    /// The same queue on a Pairing card, where there is no room for a readout and
    /// it has to carry its own count. The only surface that shows a queue
    /// belonging to a Pairing the phone has switched away from.
    static func pairingsPending(_ count: Int64) -> String {
        count == 1
            ? "1 ENTRY WAITING FOR THE RELAY · SENT WHILE THIS APP IS OPEN"
            : "\(count) ENTRIES WAITING FOR THE RELAY · SENT WHILE THIS APP IS OPEN"
    }

    // -- The foreground-only disclosure ----------------------------------------
    //
    // The one honest consequence of ADR 0007, in the app's own voice and on the
    // screen rather than in a release note. It is the single most surprising
    // thing about how this app works, and a person who does not know it will
    // read it as a bug. On iOS it is forced as well as chosen: Background Modes
    // is entitlement-gated alongside push, and a free team has neither.
    //
    // Pinned chrome, not a list item: clipped to one line until asked, verbatim
    // once opened. A fact that scrolls away is a fact the puzzled person never
    // reaches, and this is the one they are puzzled about.

    static let foregroundOnlyPinned = "NOTHING ARRIVES WHILE THIS IS CLOSED"
    static let foregroundOnlyWhy = "WHY \u{25B8}"
    static let foregroundOnlyClose = "\u{25B4} CLOSE"
    /// What the band's tap does, for a screen reader. The chip says `WHY ▸`,
    /// which is an affordance rather than a sentence, and the whole band is the
    /// target.
    static let foregroundOnlyWhyAction = "Show why nothing arrives while this is closed"
    static let foregroundOnlyNote =
        "Sharepaste only looks for new Entries while this app is open. It does no work in the "
        + "background — that is deliberate, and it is why nothing here drains your battery or "
        + "reads your clipboard behind your back. Something you copied on the computer an hour "
        + "ago arrives the moment you open Sharepaste, not before."
    // The same fact as four things that are not happening.
    static let foregroundOnlyTagSync = "NO BACKGROUND SYNC"
    static let foregroundOnlyTagNotification = "NO NEW-ENTRY NOTIFICATION"
    static let foregroundOnlyTagWatching = "NO CLIPBOARD WATCHING"
    static let foregroundOnlyTagCounterparty = "ONE COUNTERPARTY: YOUR RELAY"

    // -- Contact ---------------------------------------------------------------
    //
    // In the phone's terms, and permanently on screen rather than only when
    // degraded. The desktop's rule is the other way round (ADR 0002); a phone is
    // out of contact almost always, so hiding the nominal reading would leave the
    // band meaning "something is wrong" every time it appeared.

    static let contactLooking = "CHECKING FOR NEW ENTRIES\u{2026}"
    static let contactOnline = "IN CONTACT WITH THE RELAY"
    static let contactOffline = "NOT IN CONTACT · NOMINAL"
    static let contactResting = "RESTING · NOT LOOKING WHILE CLOSED"
    /// A Pairing that is not the Active one. Distinct from ``contactResting``,
    /// which is the *phone* put down: this one is idle on a phone that is wide
    /// awake, and it is nominal.
    static let contactNotActive = "RESTING · NOT SYNCING THIS PAIRING"
    /// The one true fault, and the only readout that is a whole sentence: no
    /// amount of waiting fixes a revoked token, so this one says what to do about
    /// it rather than reporting a state.
    static let contactRefusedShort = "RELAY REFUSED THIS PHONE"
    static let contactRefused =
        "The Relay no longer recognises this phone. It has probably been unpaired from your "
        + "computer; pair it again to start over."
    static let contactPairAgain = "PAIR THIS PHONE AGAIN"

    // -- The Settings Screen ----------------------------------------------------
    //
    // Exactly one Pairing is Active — that is what this phone syncs, and it
    // survives a restart. Any may be Viewed, which changes nothing and is
    // forgotten when the app is put down (CONTEXT.md).
    //
    // The screen is titled SETTINGS and Pairings is a section of it. Android kept
    // `Screen.Pairings` as its enum case because renaming a symbol nobody reads
    // is churn; iOS has nothing to keep, so ``Screen/settings`` names the
    // destination for what it is.

    static let settingsTitle = "SETTINGS"
    static let settingsOpen = "Settings"
    static let settingsBack = "Back"
    static let pairingsActiveBadge = "SYNCING"
    static let pairingsViewedBadge = "SHOWING"
    static let pairingsView = "SHOW ITS ENTRIES"
    static let pairingsUse = "SYNC THIS ONE"
    static let pairingsClearHistory = "CLEAR HISTORY"
    static let pairingsForget = "FORGET"
    static let pairingsCancel = "KEEP IT"

    /// The Device Label this phone chose when it paired.
    static func pairingsThisPhone(_ label: String) -> String { "This phone here: \(label)" }

    /// The divergence, admitted. Without this the History shows one Pairing's
    /// Entries while the phone syncs another, and a list that is quietly frozen
    /// looks exactly like a list that is up to date.
    static func pairingDiverged(viewed: String, active: String) -> String {
        "You are looking at \(viewed), but this phone is syncing \(active). Nothing here is "
        + "being kept up to date, and anything you offer goes to \(active)."
    }
    static let pairingDivergedUse = "SYNC THIS ONE INSTEAD"

    // The two erasures name the User and the Relay rather than the heading: two
    // Pairings can share a name, and neither of these can be undone. Both are
    // answered inside the card they belong to rather than in a dialog, so the
    // scope stays on screen while the choice is being made.
    static let pairingsConfirmBadge = "CANNOT BE UNDONE"
    static func pairingsClearConfirm(_ pairing: String) -> String {
        "Erase every Entry of \(pairing), on the Relay and on every device paired to it?"
    }
    static func pairingsForgetConfirm(_ pairing: String) -> String {
        "Forget \(pairing)? This phone erases its Entries, its key and its access token. The "
        + "Relay itself is untouched, and pairing again would start this phone over from empty."
    }

    static func historyCleared(_ pairing: String) -> String {
        "Cleared every Entry of \(pairing)."
    }
    static func pairingForgottenPromoted(_ pairing: String, _ promoted: String) -> String {
        "Forgot \(pairing). This phone now syncs \(promoted)."
    }
    static func pairingForgottenLast(_ pairing: String) -> String {
        "Forgot \(pairing). This phone is not paired to anything now."
    }

    static let pairingUseFailed = "Sharepaste could not switch to that Pairing."
    static let clearHistoryFailed =
        "Sharepaste could not clear that History. Nothing was erased — try again once you have "
        + "a connection."
    static let forgetFailed = "Sharepaste could not finish forgetting that Pairing."

    static let pairingsAddHeading = "ADD ANOTHER PAIRING"
    static let pairingsAddBody =
        "A phone can hold several. Only the one marked Syncing is the one this phone sends to "
        + "and receives from."
    static let pairingsAddButton = "PAIR WITH A CODE"

    /// One line per Pairing card, and the only cipher this product names
    /// anywhere: `clients/core/src/crypto.rs` seals with XChaCha20-Poly1305.
    ///
    /// ADR 0002 asked for the disclosure beside pairing, where the choice to
    /// trust a Relay is being made. On this phone it is not there — the pairing
    /// flow's footer band was never built, for the reason Android deleted its own
    /// — so what survives is this card, which is read after the fact. A weaker
    /// placement than the ADR asked for, and recorded as such in ADR 0002.
    static let cipherDisclosure = "XCHACHA20-POLY1305"

    // Settings, which are thinner here than on a computer. The reason is stated
    // rather than left as a gap: someone who knows the desktop will come looking
    // for the capture switch and the deny-list, and finding nothing is
    // indistinguishable from finding a half-built screen.
    static let settingsHeading = "ABOUT THIS PHONE"
    static let settingsThisPhoneHeading = "THIS PHONE"
    /// The only switch on this screen. A Recall reaches the pasteboard whichever
    /// way this is set; all it decides is whether the Receipt names what arrived,
    /// which is the part a person may not want on screen in company.
    static let settingsShowRecalled = "SHOW WHAT WAS RECALLED"
    static let settingsShowRecalledNote =
        "A Recall puts the Entry on the clipboard either way. This only decides whether "
        + "Sharepaste tells you what it was."
    static let settingsAbsentNote =
        "Your computer has two switches this phone does not. Sharepaste never watches this "
        + "phone\u{2019}s clipboard — it only ever sends what you hand it — so there is nothing "
        + "to turn on, and no list of apps to keep it away from."
    // The same two, plus the update check, as chips. A chip that says N/A makes a
    // missing switch legible as a decision; nothing at all makes it look like a
    // screen somebody stopped building. `UPDATE CHECK · NONE` is ADR 0008: the
    // phone carries no update code, so unlike a desktop it never asks an update
    // source anything and the Relay is its only counterparty.
    static let settingsTagWatchedCapture = "WATCHED CAPTURE · N/A"
    static let settingsTagDenyList = "DENY-LIST · N/A"
    static let settingsTagUpdateCheck = "UPDATE CHECK · NONE"

    // -- Standing Actions, which on this phone are Shortcuts --------------------
    //
    // iOS's only addition to Android's four sections, and it sits where it does
    // deliberately: after the switch this phone has, before the notes about what
    // it is.
    //
    // Deliberately **not** a chrome callout. Android's blocked-notification note
    // warns about a state that went wrong — notifications switched off. "You have
    // not written a shortcut yet" is not that; it is the normal condition of a
    // fresh install, and a permanent banner for it would nag rather than report.

    static let shortcutsHeading = "STANDING ACTIONS"
    static let shortcutsBody =
        "Sharepaste offers two actions to the Shortcuts app, and they do nothing until you "
        + "build a shortcut around them. Neither one touches the clipboard itself — Shortcuts "
        + "does, which is why the app can never read or write it behind your back."
    static let shortcutsOfferRecipe = "GET CLIPBOARD \u{2192} OFFER"
    static let shortcutsOfferNote =
        "Hands whatever you copied to Sharepaste. Build it once and it works from the Action "
        + "Button, Back Tap, Control Centre or the Lock Screen."
    static let shortcutsRecallRecipe = "RECALL LATEST \u{2192} COPY TO CLIPBOARD"
    static let shortcutsRecallNote =
        "Fetches the newest Entry and hands it back for Shortcuts to copy. It will tell you if "
        + "all it could reach was this phone\u{2019}s own cache."

    /// What an App Intent says when the facade did not answer in time.
    ///
    /// Named rather than left to the system, which kills an over-budget intent
    /// with nothing said. Every FFI call is blocking and serialised, so the
    /// honest reading is "something else had the phone", not "it failed".
    static let standingActionTimedOut =
        "Sharepaste took too long to answer, so the shortcut stopped rather than hanging. "
        + "Nothing was lost — try it again in a moment."
}
