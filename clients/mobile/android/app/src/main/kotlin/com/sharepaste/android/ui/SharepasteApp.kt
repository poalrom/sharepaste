package com.sharepaste.android.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalFocusManager
import com.sharepaste.android.scan.CameraProblem
import com.sharepaste.core.Entry
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow

/**
 * Everything a screen can ask the app to do.
 *
 * A bag of lambdas rather than the state holder itself, so no composable can
 * reach past it into the repository or the core, and so a screen can be rendered
 * in a test without a facade behind it. Ticket 12 adds members here — the
 * Standing Actions' own entry points — rather than handing screens a
 * `SharepasteViewModel`.
 *
 * [offerClipboard] is the one that matters beyond this file. It is a single
 * call on `SharepasteRepository` and takes no screen for granted: the Standing
 * Actions invoke the same repository method from a transparent activity with no
 * composition at all, and it acts on the **Active** Pairing and not on the
 * Viewed one — which is the whole content of "switching the Viewed Pairing
 * changes nothing about syncing or capture".
 *
 * The verb bar's other half no longer joins it. `RECALL FIRST` calls [recall]
 * with the first displayed row, on the **Viewed** Pairing, and fetches nothing;
 * the notification's `RECALL LATEST` still calls `recallLatestOnActivePairing`
 * and still performs the round trip. The two select the same Entry whenever
 * nothing is filtered and the two Pairings agree, and differ by the fetch
 * (ADR 0010).
 */
class AppActions(
    val setDeviceLabel: (String) -> Unit,
    /** The pairing code field, as somebody types in it. */
    val setPairingCode: (String) -> Unit,
    /**
     * A code the camera read.
     *
     * Separate from [setPairingCode] because it means more: it also stands the
     * viewfinder down, and it is the one write to that field that is not a
     * keystroke. See [SharepasteViewModel.codeScanned] for why it stops at the
     * field instead of pairing.
     */
    val codeScanned: (String) -> Unit,
    /** Pair with the code the field holds, whichever of the two put it there. */
    val pairWithCode: () -> Unit,
    val setCameraProblem: (CameraProblem?) -> Unit,
    val dismissPairFailure: () -> Unit,
    /**
     * The Filter, as somebody types in it.
     *
     * Narrows the rows on screen and asks the Relay nothing, so unlike the two
     * verbs below it acts on the **Viewed** Pairing: it can only hide what that
     * History already holds.
     */
    val setFilter: (String) -> Unit,
    val offerClipboard: () -> Unit,
    val recall: (Entry) -> Unit,
    val deleteEntry: (Entry) -> Unit,
    /**
     * Put a refused act back in the queue. Only a refused row may call it.
     *
     * On the **Viewed** Pairing, like [recall] and for the same reason: the
     * queue this acts on is the one the row is a row of.
     */
    val resend: (Entry) -> Unit,
    /**
     * A hand moved the list.
     *
     * The one member that reports a fact rather than asking for something. It
     * spends the open's jump (ADR 0019): somebody who has scrolled has a
     * **Place**, and the Catch-Up that lands after them must not cost them it.
     *
     * It says *that* the list moved and never where to. A Place is one surface's
     * own and is recorded nowhere (CONTEXT.md), so the `LazyListState` stays
     * inside [HistoryScreen] and this carries no position across the seam.
     */
    val handOnTheList: () -> Unit,
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
    // -- what this phone remembers about its own chrome ---------------------
    /** Whether a Recall says what it put on the clipboard. See ADR 0009. */
    val setShowRecalled: (Boolean) -> Unit,
    /** Whether a taken Offer says so. See ADR 0018. */
    val setConfirmOffers: (Boolean) -> Unit,
    /** Close the foreground-only band for good. Only `▴ CLOSE` may call it. */
    val dismissForegroundNote: () -> Unit,
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
    setPairingCode = model::setPairingCode,
    codeScanned = model::codeScanned,
    pairWithCode = model::pairWithCode,
    setCameraProblem = model::setCameraProblem,
    dismissPairFailure = model::dismissPairFailure,
    setFilter = model::setFilter,
    offerClipboard = model::offerClipboard,
    recall = model::recall,
    deleteEntry = model::deleteEntry,
    resend = model::resend,
    handOnTheList = model::handOnTheList,
    dismissNotice = model::dismissNotice,
    openPairings = model::openPairings,
    openHistory = model::openHistory,
    openAddPairing = model::openAddPairing,
    viewPairing = model::viewPairing,
    activatePairing = model::activatePairing,
    confirm = model::confirm,
    clearHistory = model::clearHistory,
    forgetPairing = model::forgetPairing,
    setShowRecalled = model::setShowRecalled,
    setConfirmOffers = model::setConfirmOffers,
    dismissForegroundNote = model::dismissForegroundNote,
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
 *
 * System back is answered here too, per branch, and it is still not a back
 * stack: each destination replies to the gesture with the action its own `◂`
 * fires, so the two ways out of a screen cannot drift apart. On the History
 * that action is `✕`, which clears the Filter — and only when there is one to
 * clear, because a History with no needle in it is the root and back belongs to
 * the platform there. The pairing flow on a phone that has never paired has an
 * empty History behind it, which is not somewhere to be sent, so it is
 * `enabled = false` for the same reason.
 *
 * The system-bar inset is applied here, once, and not by the three screens.
 * Android 15 draws every app edge to edge whether it asked to or not, and each
 * of these screens is a stack of fixed chrome bands with a list between them —
 * an identity band under the status bar and a verb bar under the gesture pill
 * are the exact two failures edge-to-edge invites. The window itself is already
 * the same void (`themes.xml`), so the inset costs no visible seam.
 *
 * [headMoves] is the state holder's one event stream that is not a [Receipt]:
 * how the list should get to the head of the History when something has put a
 * row there — a jump at an open, a follow for a **Use** this phone made. It is
 * passed rather than read, for the reason everything else here is — no
 * composable sees the state holder — and it defaults to nothing, so a screen
 * rendered to be read composes without one.
 */
@Composable
fun SharepasteApp(
    state: UiState,
    actions: AppActions,
    modifier: Modifier = Modifier,
    headMoves: Flow<HeadMove> = emptyFlow(),
) {
    SharepasteTheme {
        Surface(modifier = modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
            when (state.screen) {
                Screen.Pairing -> {
                    // Derived, not remembered, and asked once: a flow reached
                    // from the Pairings has a screen to go back to, and the
                    // launch screen of a phone that has never paired has nothing
                    // behind it at all. The gesture and the `◂` read the same
                    // value, because two predicates one screen apart is how they
                    // start answering differently.
                    val somewhereBack = state.pairings.isNotEmpty()
                    BackHandler(enabled = somewhereBack, onBack = actions.openPairings)
                    // Owned here, outside the branch that renders the refusal.
                    // A holder that lived inside the viewfinder was torn down by
                    // its own first report, so the grant it had just asked for
                    // arrived nowhere — see [rememberCameraAccess].
                    val recheckCamera = rememberCameraAccess(onProblem = actions.setCameraProblem)
                    PairingScreen(
                        state = state.pairing,
                        onLabelChange = actions.setDeviceLabel,
                        onCodeChange = actions.setPairingCode,
                        onPair = actions.pairWithCode,
                        onDismissFailure = actions.dismissPairFailure,
                        // A way out only when there is somewhere to go. On a fresh
                        // install this screen is the whole app, and a back control
                        // that led to an empty History would be a dead end wearing
                        // a door's clothes.
                        onBack = if (somewhereBack) actions.openPairings else null,
                        onRecheckCamera = recheckCamera,
                        scanner = { CameraPreview(onCode = actions.codeScanned) },
                    )
                }

                Screen.History -> {
                    // Not quite the `✕` in the Filter band, and only while that
                    // control is on screen. With no Filter this is the root and
                    // back exits, which is the platform's answer.
                    //
                    // **Back leaves the field, the `✕` stays in it.** Both empty
                    // the needle, but they are different intentions: the `✕` is
                    // a hand already in the band clearing a query to type the
                    // next one, while back is somebody done with filtering. A
                    // field still holding a caret and the focus after the second
                    // one reads as a Filter that only looks cleared, and it takes
                    // a third press to leave a screen that appears to be at rest.
                    //
                    // Three presses do still leave the app while the keyboard is
                    // up: the IME eats the first, this takes the second. That is
                    // the cost of a Filter that does not vanish on a stray
                    // gesture — but the second press now finishes the job, so
                    // the third leaves an app that is visibly ready to be left.
                    val focus = LocalFocusManager.current
                    BackHandler(enabled = state.filter.isNotEmpty()) {
                        focus.clearFocus()
                        actions.setFilter("")
                    }
                    HistoryScreen(state = state, actions = actions, headMoves = headMoves)
                }

                Screen.Pairings -> {
                    // The same action as the `◂` in this screen's own band.
                    BackHandler(onBack = actions.openHistory)
                    PairingsScreen(state = state, actions = actions)
                }
            }
        }
    }
}
