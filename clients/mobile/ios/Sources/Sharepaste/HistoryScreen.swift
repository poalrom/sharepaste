import SharepasteCore
import SwiftUI

/// The Entries of the Viewed Pairing, and the two verbs that need no row.
///
/// The list is the whole point of the client: something copied on the computer
/// shows up as an Entry, and a Recall puts it back on this phone's pasteboard.
///
/// **Everything above the list is chrome and cannot scroll away.** That is worth
/// stating because the arrangement it replaced looked identical until you
/// scrolled: the Contact readout and the foreground-only note were the first two
/// items *in* the list, so the two facts a puzzled person most needs were the
/// first two to leave the screen. Now identity, Contact and the background
/// policy are fixed bands, and the eleventh row peeks under the verb bar
/// instead. The third of them is the only one a person can be rid of, and only
/// by acknowledging it — see ``ForegroundOnlyNote``.
///
/// Contact is permanent rather than degraded-only, inverting the desktop's rule
/// (ADR 0002) rather than copying it: a phone is out of contact almost always,
/// so a band that appeared only when disconnected would be a band that was
/// always there and always looked like bad news. ``ContactReadout`` holds that.
///
/// Takes a whole ``UiState`` and an ``AppActions`` rather than a widening list
/// of pieces: nothing here sees the state holder, the repository or the core,
/// which is what lets every sentence on this screen be drawn with no facade
/// behind it.
///
/// **No `NavigationStack`, and no back gesture.** Spec row 29; the argument is
/// in ``SharepasteRoot``. This screen is the root of the app anyway — the `◎` in
/// the identity band leads out of it, and the `◂` on the screen it opened is the
/// only thing that leads back.
@MainActor
struct HistoryScreen: View {

    let state: UiState
    let actions: AppActions

    /// The Entry that just arrived, or `nil`.
    ///
    /// Written by exactly one arm of the state holder — `entryAdded` — and that
    /// narrowness is the whole feature. See ``newestStaysInView(_:scroll:)``.
    let arrived: Int64?

    var body: some View {
        VStack(spacing: 0) {
            IdentityBand(state: state, actions: actions)
            // Permanent, in every phase. A revoked token is the one that grows a
            // sentence and a way out of it.
            ContactReadout(phase: state.session, onPairAgain: actions.openAddPairing)
            // The one band here a person can be rid of, and only by
            // acknowledging it. Gone entirely rather than drawn empty, and
            // nothing is lost with it: the sentence keeps its full length on the
            // Settings Screen, so what leaves this screen is the reminder and
            // not the disclosure.
            if !state.foregroundNoteDismissed {
                ForegroundOnlyNote(onDismiss: actions.dismissForegroundNote)
            }
            // What just happened is not an Entry, and a notice that scrolled
            // away with the rows would be a notice the person never read.
            if let notice = state.notice {
                NoticeBand(notice: notice, onDismiss: actions.dismissNotice)
            }
            // The same reasoning, more urgently. These rows belong to a Pairing
            // this phone is not syncing, so nothing here is being kept up to
            // date — and a frozen list looks exactly like a current one.
            if state.diverged {
                DivergenceBand(
                    viewedName: state.nameOf(state.viewedPairing),
                    activeName: state.nameOf(state.activeUserId),
                    onUseViewed: { state.viewedPairing.map(actions.activatePairing) }
                )
            }
            if state.pending > 0 { PendingBand(count: state.pending) }

            // The two erasures a person can ask for both live on the Settings
            // Screen, beside the Pairing each is about, and both are answered
            // inside that card rather than in a dialog so the scope stays on
            // screen while the choice is made. Nothing here erases more than one
            // row, which is why this screen carries no confirmation strip — if
            // one ever arrives, it goes in the card and never in an alert.
            rows.frame(maxWidth: .infinity, maxHeight: .infinity)

            VerbBar(actions: actions)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Fui.panel)
        .fuiBackdrop()
    }

    @ViewBuilder
    private var rows: some View {
        if state.entries.isEmpty {
            VStack(spacing: 0) {
                EmptyHistory()
                Spacer(minLength: 0)
            }
        } else {
            ScrollViewReader { scroll in
                List {
                    // Newest first, as the facade hands them over. The order is
                    // not re-derived here: an order computed twice is an order
                    // that can disagree with itself.
                    ForEach(state.entries, id: \.id) { entry in
                        row(entry)
                            .listRowInsets(EdgeInsets())
                            .listRowSeparator(.hidden)
                            .listRowBackground(Color.clear)
                    }
                }
                // A `List` rather than a `LazyVStack`, and the reason is the
                // swipe: `.swipeActions` exists only inside a `List`. The four
                // modifiers below undo what `List` draws by default — SwiftUI's
                // own insets, separators, row floor and grouped background would
                // each land on top of a palette that has already decided them.
                .listStyle(.plain)
                .scrollContentBackground(.hidden)
                .environment(\.defaultMinListRowHeight, 0)
                .onChange(of: arrived) { newest in
                    newestStaysInView(newest, scroll: scroll)
                }
            }
        }
    }

    /// The newest Entry is where it can be seen the moment it arrives, and at no
    /// other moment.
    ///
    /// Entries are prepended, so the newest is the head and the destination
    /// never changes; what changes is the distance to it. Everything above the
    /// list is chrome and three of those bands come and go — a notice, a
    /// divergence and a pending queue can each be there or not, and each one
    /// pushes the row `RECALL LATEST` will hand over further under the top of
    /// the viewport.
    ///
    /// **The trigger is ``arrived`` and deliberately not the head of the list.**
    /// An arrival is a new head with the old head still under it, which is a
    /// narrower question than "the head changed" and is the only one that means
    /// what this is for. Deleting the newest row changes the head too, and so
    /// does switching the Viewed Pairing; dragging somebody to the top because
    /// they removed a row, or because they are now looking at a list they have
    /// not read a word of, is not a courtesy.
    ///
    /// The state holder already answers the narrow question —
    /// ``SharepasteViewModel/arrived`` is written by the `entryAdded` arm and by
    /// nothing else — so this view compares nothing and remembers nothing.
    /// Android needs a `rememberSaveable` seen-value to tell the cases apart,
    /// because a `LazyListState` cannot say why its head moved; here the
    /// distinction arrives already made, which is the whole benefit of the state
    /// holder owning it.
    private func newestStaysInView(_ newest: Int64?, scroll: ScrollViewProxy) {
        guard let newest else { return }
        withAnimation { scroll.scrollTo(newest, anchor: .top) }
    }

    /// One Entry, in whichever of its two shapes.
    ///
    /// **Delete is a swipe, and only the readable rows have one.** Android chose
    /// the gesture to avoid twenty targets a screen and to keep an undoable
    /// destructive action away from a safe one, and logged the discoverability
    /// cost as a standing risk. On iOS the risk is smaller — `.swipeActions` is
    /// an idiom people expect — but the *outcome* is the same and so is the
    /// design. A visible delete button is not added here merely because iOS
    /// could afford one.
    ///
    /// **Android's review then had to make its revealed `✕ DELETE` a real
    /// button, and that fix has nothing to port.** `SwipeToDismissBox` composes
    /// the panel under every row on every frame, and an opaque colour is not a
    /// pointer target — so a panel that were pressable at rest would have put an
    /// undoable Delete under most of the screen, and one that were not would
    /// have shown a control that answers a press with nothing. `.swipeActions`
    /// is armed-then-tapped already and UIKit owns both halves. The note is here
    /// so nobody reads Android's fix as a behaviour iOS lacks.
    @ViewBuilder
    private func row(_ entry: Entry) -> some View {
        if entry.undecryptable {
            // No swipe on this one, matching Android, and the asymmetry is the
            // point: Delete is the only thing left to do with the row, so it is
            // not put behind a gesture somebody would have to discover. Exactly
            // one Delete per row either way.
            UndecryptableRow(entry: entry, onDelete: actions.deleteEntry)
        } else {
            EntryRow(
                entry: entry,
                ownDeviceId: state.ownDeviceId,
                newest: entry.id == state.entries.first?.id,
                onRecall: actions.recall
            )
            .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                Button(role: .destructive) {
                    actions.deleteEntry(entry)
                } label: {
                    Text(Strings.entryDelete)
                }
                // The palette's own red rather than the system's. Everything
                // destructive in this app is `alert500`, and a swipe panel in
                // Apple's red would be the one place it is not.
                .tint(Fui.alert500)
            }
        }
    }
}

// ── The rows ─────────────────────────────────────────────────────────────────

/// One readable Entry: a single tap target, and a Delete that has to be dragged
/// for.
///
/// Three things the row has to get right, and each is a mistake the desktop made
/// first:
///
///  * The **Preview** is rendered as it arrives. `Entry.preview` is the Preview
///    on every path the core produces an Entry on — one line, control characters
///    already spaces, trimmed and capped — so this row neither re-derives it nor
///    reads `Entry.plaintext`, which is the raw text and would render an
///    indented Entry as a blank line.
///  * **Undecryptable** comes from `Entry.undecryptable` and from nowhere else.
///    Never from an empty Preview: an Entry whose plaintext is genuinely empty
///    is indistinguishable from ciphertext this device has no key for to
///    anything guessing, and the desktop guessed in four places.
///  * **Origin appears only on Entries from another Device.** It is "the device
///    an Entry was captured on, as distinct from the device viewing it"
///    (`CONTEXT.md`), so a row this phone offered has nothing to distinguish and
///    stays a single line.
///
/// The newest row wears the emitter's treatment — a rule, a tint, and the Recall
/// named rather than drawn as a glyph — because it is the row `RECALL LATEST`
/// will hand over, and a person should be able to see which one that is before
/// they press it.
@MainActor
private struct EntryRow: View {

    let entry: Entry
    let ownDeviceId: String?
    let newest: Bool
    let onRecall: (Entry) -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                // The emitter's own rule, on the one row Recall Latest acts on.
                // Every other row is flush with the gutter.
                if newest {
                    Rectangle()
                        .fill(Fui.cyan400)
                        .frame(width: 2)
                        .frame(maxHeight: .infinity)
                }
                VStack(alignment: .leading, spacing: 5) {
                    Text(entry.preview)
                        .fuiText(Fui.data, color: newest ? Fui.textPrimary : Fui.textBody)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    if entry.deviceId != ownDeviceId {
                        // Resolved by the core: the Device Label, or a slice of
                        // the Device id when the mirror has none.
                        Text(Strings.entryOrigin(entry.originLabel))
                            .fuiText(Fui.micro, color: Fui.textMuted)
                            .lineLimit(1)
                            .truncationMode(.tail)
                    }
                }
                .padding(.leading, newest ? 12 : Fui.gutter)
                .frame(maxWidth: .infinity, alignment: .leading)

                if newest {
                    // Named, filled and wider, because this is the row the verb
                    // bar acts on.
                    FuiButton(
                        text: Strings.entryRecall,
                        action: { onRecall(entry) },
                        solid: true,
                        fillsWidth: true
                    )
                    .frame(width: 96)
                } else {
                    GlyphButton(
                        glyph: Glyphs.recall,
                        action: { onRecall(entry) },
                        accessibilityLabel: Strings.entryRecall
                    )
                }
            }
            .padding(.trailing, 8)
            .frame(height: Fui.rowHeight)
            // Two layers, and the reason is not Android's. There a translucent
            // row lets the swipe panel through and paints the newest one red;
            // here it would let the backdrop's grid through instead. Either way
            // the emitter's 12% tint has to sit over the panel rather than over
            // the atmosphere.
            .background(newest ? Fui.active : Color.clear)
            .background(Fui.panel)
            Hairline(color: Fui.cyanA08)
        }
    }
}

/// Ciphertext this phone holds no key for: named, not blanked, and deletable.
///
/// **Recall is disabled rather than hidden**, following the desktop's detail
/// pane and not its row — the control someone is looking for has to still be
/// where they are looking, saying no, with the marker beside it as the reason. A
/// row that simply lost a control would read as broken rather than as sealed.
///
/// Two lines and not one: ``Strings/entryUndecryptableMarker`` names the state
/// in the product's own word and the sentence under it says what that means. A
/// blank Preview would say neither.
@MainActor
private struct UndecryptableRow: View {

    let entry: Entry
    let onDelete: (Entry) -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 6) {
                Rectangle()
                    .fill(Fui.alert400)
                    .frame(width: 2)
                    .frame(maxHeight: .infinity)
                VStack(alignment: .leading, spacing: 5) {
                    Text(Strings.entryUndecryptableMarker)
                        .fuiText(Fui.label, color: Fui.alert400)
                        .lineLimit(1)
                    Text(Strings.entryUndecryptable)
                        .fuiText(Fui.micro, color: Fui.textBody)
                        .lineLimit(2)
                }
                .padding(.leading, 12)
                .frame(maxWidth: .infinity, alignment: .leading)

                GlyphButton(
                    glyph: Glyphs.recall,
                    action: {},
                    accessibilityLabel: Strings.entryRecall,
                    enabled: false
                )
                GlyphButton(
                    glyph: Glyphs.delete,
                    action: { onDelete(entry) },
                    accessibilityLabel: Strings.entryDelete,
                    accent: .alert
                )
            }
            .padding(.trailing, 8)
            .frame(height: Fui.rowHeight)
            .background(Fui.alertA16)
            .background(Fui.panel)
            Hairline(color: Fui.cyanA08)
        }
    }
}

// ── The verbs ────────────────────────────────────────────────────────────────

/// Offer and Recall Latest, the two verbs that need no row selected.
///
/// Deliberately **not** called Standing Actions: those are the verbs a device
/// performs without its own surface being opened, and these are controls on an
/// open screen. They call the same two repository entry points the Standing
/// Actions do, which is the point — neither of those entry points assumes a view
/// exists.
///
/// **Recall Latest is the solid one and comes first.** Recall is why a phone is
/// opened at all — the laptop copied something and the phone has to paste it —
/// and it is the one verb that fetches rather than trusting the cache, so it is
/// the one that must never hand over something stale. Two equal outlines were
/// truthful about their symmetry in the code and mute about which one a person
/// reaches for. Offer keeps a full-height target beside it, in the order
/// Shortcuts lists them.
///
/// Offer is this phone's only capture, and it is Offered Capture by
/// construction: it never watches a pasteboard — no mobile OS permits it (ADR
/// 0007) — and reads one only at the moment it is pressed. Expect iOS to raise
/// its paste banner; that is the platform telling the truth about what just
/// happened, and it must not be engineered around.
@MainActor
private struct VerbBar: View {

    let actions: AppActions

    private static let gap: CGFloat = 8
    /// Compose's `Modifier.weight`, as the two numbers it actually is.
    private static let weights: (recall: CGFloat, offer: CGFloat) = (1.6, 1)

    var body: some View {
        VStack(spacing: 0) {
            Hairline(color: Fui.frame)
            // SwiftUI has no `Modifier.weight`, and `1.6 : 1` is a ratio rather
            // than two sizes — two `.frame(maxWidth: .infinity)` slots would
            // give `1 : 1` and lose the ranking that is the whole point of this
            // bar. A `GeometryReader` divides the row the way Compose does;
            // hard-coding a pair of widths would be a guess about a screen size.
            GeometryReader { geometry in
                let unit = max(0, geometry.size.width - Self.gap)
                    / (Self.weights.recall + Self.weights.offer)
                HStack(spacing: Self.gap) {
                    FuiButton(
                        text: Strings.recallLatestBar,
                        action: actions.recallLatest,
                        solid: true,
                        fillsWidth: true
                    )
                    .frame(width: unit * Self.weights.recall)
                    FuiButton(
                        text: Strings.offerBar,
                        action: actions.offerPasteboard,
                        fillsWidth: true
                    )
                    .frame(width: unit * Self.weights.offer)
                }
            }
            .frame(height: Fui.target)
            .padding(Fui.gutter)
            .background(Fui.band)
        }
    }
}

/// Nothing here yet, said as a state rather than as an absence.
///
/// A phone that has just paired has an empty History and no way to tell whether
/// that is correct. The heading names the state and the body says what fills it,
/// which is the same disclosure ``ForegroundOnlyNote`` makes, arrived at from
/// the other direction.
@MainActor
private struct EmptyHistory: View {

    var body: some View {
        VStack(spacing: 10) {
            Text(Strings.historyEmptyHeading)
                .fuiText(Fui.heading, color: Fui.textPrimary)
            Text(Strings.historyEmptyBody)
                .fuiText(Fui.prose, color: Fui.textBody)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity)
        .padding(24)
    }
}

// ── Gallery ──────────────────────────────────────────────────────────────────

#if DEBUG

/// Sample state, and nothing that could ever be mistaken for real content.
///
/// Every Preview here is an `example.invalid` address or plain words. A fixture
/// that looked like somebody's clipboard is a fixture somebody eventually
/// screenshots.
private enum Sample {

    static let ownDevice = "device-this-phone"
    static let otherDevice = "device-the-laptop"
    static let user = "user-01"

    static func entry(
        _ id: Int64,
        _ preview: String,
        from device: String = ownDevice,
        origin: String = "Laptop",
        undecryptable: Bool = false
    ) -> Entry {
        Entry(
            id: id,
            userId: user,
            preview: preview,
            plaintext: undecryptable ? nil : preview,
            createdAt: 1_760_000_000 + id,
            deviceId: device,
            deviceLabel: origin,
            originLabel: origin,
            undecryptable: undecryptable
        )
    }

    static let pairing = PairingSummary(
        userId: user,
        deviceId: ownDevice,
        label: "iPhone in my pocket",
        username: "ada",
        serverUrl: "https://relay.example.invalid",
        relayHost: "relay.example.invalid",
        status: .online,
        pending: 0,
        isActive: true
    )

    /// A History with every row shape in it: the newest carrying the emitter's
    /// treatment, one from another Device wearing an Origin, one this phone
    /// offered wearing none, and one Undecryptable.
    static var populated: UiState {
        var state = UiState()
        state.screen = .history
        state.foreground = true
        state.session = .inContact(userId: user)
        state.activeUserId = user
        state.ownDeviceId = ownDevice
        state.pairings = [pairing]
        state.foregroundNoteDismissed = true
        state.entries = [
            entry(4, "https://example.invalid/the-newest-thing", from: otherDevice),
            entry(3, "a note this phone offered itself"),
            entry(2, "", from: otherDevice, undecryptable: true),
            entry(1, "https://example.invalid/older", from: otherDevice),
        ]
        return state
    }

    static var empty: UiState {
        var state = UiState()
        state.screen = .history
        state.foreground = true
        state.session = .outOfContact(userId: user)
        state.activeUserId = user
        state.ownDeviceId = ownDevice
        state.pairings = [pairing]
        return state
    }
}

/// The six Notices, one under another.
///
/// Eight bands for six variants: ``Notice/offerRefused(reason:)`` and
/// ``Notice/pairingForgotten(pairing:promoted:)`` each read differently
/// depending on what they carry, and a gallery that drew one arm of each would
/// miss the two places the words change. ``Notice/recalledFromCache`` is the one
/// that has to be looked at *beside* the others, because the whole argument for
/// it being a Notice is that it reads like a confirmation — seeing it in caution
/// amber above five bands in the emitter's voice is the check that the argument
/// survived the drawing.
struct NoticeGallery: View {

    private static let notices: [(String, Notice)] = [
        ("OFFER REFUSED · NOTHING TO SEND", .offerRefused(reason: .nonText)),
        ("OFFER REFUSED · ALREADY HERE", .offerRefused(reason: .duplicate)),
        ("RECALLED FROM CACHE", .recalledFromCache),
        ("UNPAIRED", .unpaired),
        ("HISTORY CLEARED", .historyCleared(pairing: "ada")),
        ("PAIRING FORGOTTEN", .pairingForgotten(pairing: "ada", promoted: "grace")),
        ("PAIRING FORGOTTEN · LAST ONE", .pairingForgotten(pairing: "ada", promoted: nil)),
        (
            "FAILED",
            .failed(
                sentence: Strings.deleteFailed,
                detail: "relay.example.invalid: connection refused"
            )
        ),
    ]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                ForEach(Self.notices, id: \.0) { name, notice in
                    SectionHeading(name).padding(.horizontal, Fui.gutter)
                    NoticeBand(notice: notice, onDismiss: {})
                }
            }
            .padding(.vertical, Fui.gutter)
        }
        .background(Fui.panel)
        .fuiBackdrop()
    }
}

/// `PreviewProvider` rather than the `#Preview` macro — see
/// `ReceiptGallery_Previews` for why the macro cannot be used on this build
/// path.
struct HistoryScreen_Previews: PreviewProvider {
    static var previews: some View {
        Group {
            HistoryScreen(state: Sample.empty, actions: .inert, arrived: nil)
                .previewDisplayName("History · empty")

            HistoryScreen(state: Sample.populated, actions: .inert, arrived: nil)
                .previewDisplayName("History · populated")

            HistoryScreen(state: Sample.everyBand, actions: .inert, arrived: nil)
                .previewDisplayName("History · every band")

            NoticeGallery()
                .previewDisplayName("Notices")
        }
        .preferredColorScheme(.dark)
    }
}

private extension Sample {
    /// Every band at once, which is the arrangement that pushes the newest row
    /// furthest down and is therefore the one worth looking at.
    static var everyBand: UiState {
        var state = populated
        state.foregroundNoteDismissed = false
        state.pending = 3
        state.notice = .recalledFromCache
        return state
    }
}

#endif
