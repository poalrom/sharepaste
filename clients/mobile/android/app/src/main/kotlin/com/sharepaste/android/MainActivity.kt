package com.sharepaste.android

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.runtime.getValue
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import com.sharepaste.android.standing.StandingActions
import com.sharepaste.android.ui.SharepasteApp
import com.sharepaste.android.ui.SharepasteViewModel
import com.sharepaste.android.ui.appActions
import com.sharepaste.android.ui.showReceipt
import kotlinx.coroutines.launch

/**
 * The one activity, and the two edges that are the entire sync model.
 *
 * `onStart` resumes the Active Pairing and brings its session up; `onStop` takes
 * every session down. There is nothing else: no WorkManager, no JobScheduler, no
 * foreground service and no push (ADR 0007). That is a decision, not an omission
 * — a clipboard tool that runs unattended is a clipboard tool that reads your
 * clipboard unattended — and its one honest consequence is on screen in
 * `foreground_only_note`, not hidden in a release note.
 *
 * The edges are driven from here rather than from a `LifecycleObserver` inside the
 * state holder, because `onStart`/`onStop` *are* the contract: they are what a
 * test moves an `ActivityScenario` across, and a session that came up from a
 * composition entering the tree instead would be a session that came up on a
 * rotation too.
 */
class MainActivity : ComponentActivity() {

    private val model: SharepasteViewModel by viewModels { SharepasteViewModel.Factory }

    /**
     * The state holder's current snapshot.
     *
     * The wiring between `onStart`/`onStop` and the sync model is the whole of
     * ADR 0007 on this platform, and an instrumented test has to be able to see
     * that the edges arrived. Read-only and a snapshot, so nothing outside can
     * drive the activity through it.
     */
    @get:androidx.annotation.VisibleForTesting
    val uiStateForTests: com.sharepaste.android.ui.UiState get() = model.state.value

    /**
     * The notification permission request, registered before the activity is
     * ever started, because `registerForActivityResult` requires it.
     *
     * It is **never** fired from a lifecycle callback. A permission dialog takes
     * window focus, and an app that asks on launch has spent the person's one
     * chance to say yes on a moment when they have no idea what they are being
     * asked for — the notification is worth explaining first. So the request
     * comes from the note on the History screen, which is the sentence that
     * explains it, and nothing else can start it.
     *
     * Below API 33 there is no such permission, so the note's control sends the
     * person to the app's notification settings instead: on those versions the
     * only way notifications are off is that somebody turned them off there.
     */
    private val askForNotifications =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            if (granted) StandingActions.post(this)
            model.onStandingActionsChecked(blocked = !StandingActions.enabled(this))
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val actions = appActions(model, enableStandingActions = ::enableStandingActions)
        // A Receipt is a Toast rather than something in the Compose tree, so it
        // is collected here and not composed. `STARTED` is the window: a
        // confirmation for a verb pressed on this screen belongs on this screen,
        // and one that arrived while the app was away has been superseded by
        // whatever the person did since.
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                model.receipts.collect { showReceipt(this@MainActivity, it) }
            }
        }
        setContent {
            val state by model.state.collectAsStateWithLifecycle()
            SharepasteApp(state, actions, headMoves = model.headMoves)
        }
    }

    override fun onStart() {
        super.onStart()
        model.onEnterForeground()
        // Asked every time rather than once, because a person can turn
        // notifications off in Settings while the app is away and the platform
        // offers no callback for it. Posting is idempotent, so this is also
        // what puts the notification back after a fresh grant.
        val posted = StandingActions.post(this)
        model.onStandingActionsChecked(blocked = !posted)
    }

    override fun onStop() {
        super.onStop()
        model.onLeaveForeground()
    }

    /**
     * Ask for the Standing Actions back, by whichever route this version has.
     *
     * Two variants, and both are real. From API 33 the grant is a runtime
     * permission and the system dialog is the only way to ask for it — but it
     * is one-shot per install, so a second refusal makes the dialog silently
     * never appear, which would leave the control doing nothing. The app's own
     * notification settings always work and are where a sub-33 device's switch
     * lives anyway, so that is the fallback for both cases.
     */
    private fun enableStandingActions() {
        val needsRuntimeGrant = Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        if (needsRuntimeGrant && !askedOnce) {
            askedOnce = true
            askForNotifications.launch(Manifest.permission.POST_NOTIFICATIONS)
            return
        }
        startActivity(
            Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                .putExtra(Settings.EXTRA_APP_PACKAGE, packageName),
        )
    }

    /**
     * Whether the runtime dialog has been offered in this activity's life.
     *
     * The platform shows it at most twice per install and then never again,
     * silently. Without this the control would launch a request that does
     * nothing at all and the person would press it forever.
     */
    private var askedOnce = false
}
