package com.sharepaste.android.ui

import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.sharepaste.android.scan.CameraProblem
import com.sharepaste.core.Entry

/**
 * Everything a screen can ask the app to do.
 *
 * A bag of lambdas rather than the state holder itself, so no composable can
 * reach past it into the repository or the core, and so a screen can be rendered
 * in a test without a facade behind it. Ticket 12 adds members here — the
 * Standing Actions' own entry points — rather than handing screens a
 * `SharepasteViewModel`.
 *
 * [offerClipboard] and [recallLatest] are the two that matter beyond this file.
 * They are one call each on `SharepasteRepository`, and neither takes a screen
 * for granted: ticket 12's Standing Actions invoke the same repository methods
 * from a transparent activity with no composition at all. Both act on the
 * **Active** Pairing and not on the Viewed one, which is the whole content of
 * "switching the Viewed Pairing changes nothing about syncing or capture".
 */
class AppActions(
    val setDeviceLabel: (String) -> Unit,
    val pairWithCode: (String) -> Unit,
    val setCameraProblem: (CameraProblem?) -> Unit,
    val dismissPairFailure: () -> Unit,
    val offerClipboard: () -> Unit,
    val recallLatest: () -> Unit,
    val recall: (Entry) -> Unit,
    val deleteEntry: (Entry) -> Unit,
    val dismissNotice: () -> Unit,
    // -- the Pairings screen ------------------------------------------------
    val openPairings: () -> Unit,
    val openHistory: () -> Unit,
    val openAddPairing: () -> Unit,
    /** Look at this Pairing's History. Transient; changes no syncing. */
    val viewPairing: (String) -> Unit,
    /** Sync this Pairing from now on. Persistent; changes what the phone does. */
    val activatePairing: (String) -> Unit,
    /** Ask for a destructive action, or take the question back. */
    val confirm: (Confirmation?) -> Unit,
    val clearHistory: (String) -> Unit,
    val forgetPairing: (String) -> Unit,
    // -- the Standing Actions -----------------------------------------------
    /**
     * Ask the platform for the notification back.
     *
     * The one member the state holder cannot supply, and the reason is worth
     * knowing: a runtime permission request needs an `Activity` registered for
     * a result before it is started, so this is wired by `MainActivity` and
     * defaults to doing nothing. A screen rendered in a test therefore shows the
     * blocked note and its control without a permission dialog anywhere near it.
     */
    val enableStandingActions: () -> Unit = {},
)

/**
 * The app's own wiring, in one place.
 *
 * `MainActivity` builds its bag from here, and so does every instrumented test
 * that drives a real screen against a real facade. That is the point: a test
 * that assembled its own lambdas could pass while the activity was wired to
 * something else entirely, and the wiring is exactly the part nobody re-reads.
 */
fun appActions(
    model: SharepasteViewModel,
    enableStandingActions: () -> Unit = {},
) = AppActions(
    setDeviceLabel = model::setDeviceLabel,
    pairWithCode = model::pairWithCode,
    setCameraProblem = model::setCameraProblem,
    dismissPairFailure = model::dismissPairFailure,
    offerClipboard = model::offerClipboard,
    recallLatest = model::recallLatest,
    recall = model::recall,
    deleteEntry = model::deleteEntry,
    dismissNotice = model::dismissNotice,
    openPairings = model::openPairings,
    openHistory = model::openHistory,
    openAddPairing = model::openAddPairing,
    viewPairing = model::viewPairing,
    activatePairing = model::activatePairing,
    confirm = model::confirm,
    clearHistory = model::clearHistory,
    forgetPairing = model::forgetPairing,
    // The one member the state holder cannot supply. `MainActivity` passes a
    // permission request; every other caller leaves it doing nothing, which is
    // the right answer for a screen with no activity result behind it.
    enableStandingActions = enableStandingActions,
)

/**
 * The whole interface, from one [UiState].
 *
 * Three destinations and the choice between them is a fact about the data: a
 * phone with no Pairing has nothing to show and one screen it can usefully be on.
 * That is a `when`, not a navigation graph — and it stays a `when` at three, so
 * adding a `Screen` is a compile error here rather than a route nobody registered.
 */
@Composable
fun SharepasteApp(state: UiState, actions: AppActions, modifier: Modifier = Modifier) {
    SharepasteTheme {
        Surface(modifier = modifier) {
            when (state.screen) {
                Screen.Pairing -> PairingScreen(
                    state = state.pairing,
                    onLabelChange = actions.setDeviceLabel,
                    onCode = actions.pairWithCode,
                    onDismissFailure = actions.dismissPairFailure,
                    // A way out only when there is somewhere to go. On a fresh
                    // install this screen is the whole app, and a back control
                    // that led to an empty History would be a dead end wearing a
                    // door's clothes.
                    onBack = if (state.activeUserId == null) null else actions.openPairings,
                    scanner = {
                        CameraScanner(
                            onProblem = actions.setCameraProblem,
                            onCode = actions.pairWithCode,
                        )
                    },
                )

                Screen.History -> HistoryScreen(state = state, actions = actions)

                Screen.Pairings -> PairingsScreen(state = state, actions = actions)
            }
        }
    }
}
