import SharepasteKit
import SwiftUI

/// The process's one facade, and everything handed to it.
///
/// **One per process, not one per scene**, and that is the whole reason it is a
/// singleton rather than a value the scene owns. An App Intent runs in this same
/// process and may run when no scene has ever existed — a shortcut fired from
/// the Action Button on a locked phone. A second ``SharepasteRepository`` over
/// the same SQLite file would be a second connection, a second tokio runtime and
/// a second opinion about which Pairing is Active; the core is built to be
/// opened once.
///
/// Opening the facade is a blocking call — a SQLite connection, the migrations,
/// a runtime — which is why ``SharepasteRepository`` returns immediately and
/// opens behind itself. Nothing here has to be an `async` initialiser, and
/// touching ``shared`` from an intent costs no wait.
///
/// The state holder is *not* here. It is `@MainActor` and it is the screen's; an
/// intent has no composition and must not build one.
final class AppGraph: Sendable {
    static let shared = AppGraph()

    let repository: SharepasteRepository
    let preferences: UiPreferences

    private init() {
        // A failure here means the container itself is unusable, which is not a
        // state the app can report from: every screen it could draw the message
        // on needs the facade behind it, and an intent that could not open it has
        // nothing to say either. `applicationSupportDirectory` failing is a
        // device with no writable container.
        let directory = try! AppContainer.databaseDirectory()
        repository = SharepasteRepository(
            directory: directory,
            requireHttps: TransportPolicy.requireHttps
        )
        preferences = UiPreferences()
    }
}

@main
struct SharepasteApp: App {

    @Environment(\.scenePhase) private var scenePhase
    @StateObject private var model: SharepasteViewModel

    init() {
        let graph = AppGraph.shared
        _model = StateObject(
            wrappedValue: SharepasteViewModel(repo: graph.repository, preferences: graph.preferences)
        )
    }

    var body: some Scene {
        WindowGroup {
            SharepasteRoot(state: model.state, actions: appActions(model), model: model)
                // Forced rather than defaulted. `UIUserInterfaceStyle: Dark` in
                // the plist covers UIKit; this covers a SwiftUI hierarchy that
                // would otherwise still honour a `.light` override from anywhere
                // above it. Decision 12 of the Android redesign refuses a light
                // scheme, and every contrast ratio in `Fui.swift` is measured
                // against the void.
                .preferredColorScheme(.dark)
        }
        .onChange(of: scenePhase) { phase in
            // The whole sync model, in four lines. `.active` and `.background`
            // only: `.inactive` is the app being covered by a call banner or the
            // task switcher, and tearing a session down for that would make
            // every glance at Control Centre a reconnect.
            switch phase {
            case .active: model.onEnterForeground()
            case .background: model.onLeaveForeground()
            case .inactive: break
            @unknown default: break
            }
        }
    }
}

/// The whole interface, from one ``UiState``.
///
/// Three destinations and the choice between them is a fact about the data: a
/// phone with no Pairing has nothing to show and one screen it can usefully be
/// on. That is a `switch`, not a navigation graph — and it stays a `switch` at
/// three, so adding a ``Screen`` is a compile error here rather than a route
/// nobody registered.
///
/// **There is no `NavigationStack`, and the left-edge swipe therefore does
/// nothing.** That is spec row 29 and it is the one place iOS declines to follow
/// Android's `0.5.0` review, which gave every screen a `BackHandler` firing the
/// same action as its on-screen `◂`. The literal port is empty — iOS has no
/// system Back button — and the near-port is declined: a stack would put its own
/// path and this value in disagreement about which screen is on screen, on the
/// one client with no automated UI defence to catch the drift.
///
/// The comment is here rather than the absence being left to speak for itself,
/// because an iOS file that merely *lacks* Android's back handling reads as an
/// omission and invites someone to add a stack later. The `◂` still reaches
/// every destination Android's back does; what is lost is the gesture.
@MainActor
struct SharepasteRoot: View {
    let state: UiState
    let actions: AppActions
    @ObservedObject var model: SharepasteViewModel

    var body: some View {
        ZStack {
            Fui.panel.ignoresSafeArea()
            switch state.screen {
            case .pairing:
                // A way out only when there is somewhere to go. On a fresh
                // install this screen is the whole app, and a back control that
                // led to an empty History would be a dead end wearing a door's
                // clothes.
                PairingScreen(
                    state: state.pairing,
                    actions: actions,
                    onBack: state.pairings.isEmpty ? nil : actions.openSettings
                )
            case .history:
                HistoryScreen(state: state, actions: actions, arrived: model.arrived)
            case .settings:
                SettingsScreen(state: state, actions: actions)
            }
        }
        .receiptOverlay(model.receipt)
    }
}
