package com.sharepaste.android.ui

import android.Manifest
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.sharepaste.android.R
import com.sharepaste.android.scan.CameraProblem
import com.sharepaste.android.scan.QrCodeAnalyser
import com.sharepaste.android.scan.cameraProblem
import com.sharepaste.android.scan.deviceHasCamera
import java.util.concurrent.Executors

/**
 * Pairing this phone to a User that already exists.
 *
 * Two paths to the same call. Scanning the square on the computer's pairing pane
 * is the practical one; typing the code underneath it is the fallback, and it is
 * load-bearing rather than decorative — it is the only way in when the camera is
 * refused or absent, which is why each of those has its own message rather than
 * a shared shrug.
 *
 * **The name is the person's.** The field starts empty and pairing is blocked
 * until it is not. The desktop's flow hard-codes a default; a machine's guess at
 * what someone calls their own phone is not a default, it is a thing they have to
 * notice and correct in a list they read later.
 *
 * The camera arrives through [scanner] rather than being reached for here. The
 * screen's job is layout and wording, and it must be renderable from a
 * [PairingState] alone — a screen that binds a camera as a side effect of being
 * composed cannot be asserted about without one, which is precisely the case
 * where the three failure messages matter.
 */
@Composable
fun PairingScreen(
    state: PairingState,
    onLabelChange: (String) -> Unit,
    onCode: (String) -> Unit,
    onDismissFailure: () -> Unit,
    modifier: Modifier = Modifier,
    /**
     * The way back, on a phone that already holds a Pairing.
     *
     * `null` on a fresh install, where this screen is the entire app and a back
     * control would lead nowhere. Adding a Pairing to a phone that has one is
     * ticket 11's path in, and a person who changes their mind halfway has to be
     * able to leave without force-quitting.
     */
    onBack: (() -> Unit)? = null,
    scanner: @Composable () -> Unit = {},
) {
    var typedCode by remember { mutableStateOf("") }

    Column(
        modifier = modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(24.dp)
            .testTag(TAG_PAIRING_SCREEN),
        verticalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        onBack?.let {
            TextButton(onClick = it, modifier = Modifier.testTag(TAG_PAIRING_BACK)) {
                Text(stringResource(R.string.pairings_back))
            }
        }
        Text(
            text = stringResource(R.string.pair_title),
            style = MaterialTheme.typography.headlineSmall,
        )

        // 1 — the name, first, because the code expires and the name does not.
        Section(stringResource(R.string.pair_label_heading), stringResource(R.string.pair_label_explainer)) {
            OutlinedTextField(
                value = state.deviceLabel,
                onValueChange = onLabelChange,
                label = { Text(stringResource(R.string.pair_label_field)) },
                placeholder = { Text(stringResource(R.string.pair_label_placeholder)) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().testTag(TAG_LABEL_FIELD),
            )
        }

        // 2 — the camera, or the reason there isn't one.
        Section(stringResource(R.string.pair_scan_heading), stringResource(R.string.pair_scan_explainer)) {
            when (state.camera) {
                CameraProblem.NoCamera -> Explanation(R.string.camera_absent, TAG_CAMERA_ABSENT)
                CameraProblem.PermissionRefused ->
                    Explanation(R.string.camera_permission_refused, TAG_CAMERA_REFUSED)

                null -> scanner()
            }
        }

        // The fallback, always visible rather than hidden behind the failure. A
        // person whose camera cannot focus on a laptop screen is not in an error
        // state, and should not have to reach one to find the other way in.
        Section(stringResource(R.string.pair_typed_heading), stringResource(R.string.pair_typed_explainer)) {
            OutlinedTextField(
                value = typedCode,
                onValueChange = { typedCode = it },
                label = { Text(stringResource(R.string.pair_typed_field)) },
                singleLine = true,
                keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(imeAction = ImeAction.Go),
                modifier = Modifier.fillMaxWidth().testTag(TAG_CODE_FIELD),
            )
            Button(
                onClick = { onCode(typedCode) },
                enabled = state.canPair && typedCode.isNotBlank(),
                modifier = Modifier.testTag(TAG_PAIR_BUTTON),
            ) {
                Text(
                    stringResource(
                        if (state.attempt is PairAttempt.Working) R.string.pair_working else R.string.pair_button,
                    ),
                )
            }
        }

        (state.attempt as? PairAttempt.Failed)?.let { failed ->
            Failure(failed, onDismissFailure)
        }
    }
}

@Composable
private fun Section(heading: String, explainer: String, content: @Composable () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(heading, style = MaterialTheme.typography.titleMedium)
        Text(
            explainer,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        content()
    }
}

/**
 * A failed attempt, in the words that fit it.
 *
 * [PairAttempt.Failed.detail] is only ever set for a cleartext Relay, where the
 * core's own sentence names the address and the reason. Showing it under the
 * app's wording rather than instead of it keeps the specific fact without handing
 * over a protocol error as an explanation.
 */
@Composable
private fun Failure(failed: PairAttempt.Failed, onDismiss: () -> Unit) {
    Surface(
        color = MaterialTheme.colorScheme.errorContainer,
        shape = MaterialTheme.shapes.medium,
        modifier = Modifier.fillMaxWidth().testTag(TAG_FAILURE),
    ) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(
                text = stringResource(failed.message),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onErrorContainer,
            )
            failed.detail?.let {
                Text(
                    text = it,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onErrorContainer,
                    modifier = Modifier.testTag(TAG_FAILURE_DETAIL),
                )
            }
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.pair_dismiss)) }
        }
    }
}

/**
 * The camera itself: ask for it, then bind it.
 *
 * Separate from [PairingScreen] because it is the one part that touches hardware.
 * It reports what it found through [onProblem] and renders a preview only when
 * there is nothing to report; the screen above renders the two problems, so the
 * wording stays with the layout and the device stays here.
 *
 * The permission is asked for once, and only for hardware that exists — see
 * [com.sharepaste.android.scan.cameraProblem]. Re-asking on every recomposition is
 * how an app ends up permanently denied by the platform's "don't ask again".
 */
@Composable
fun CameraScanner(onProblem: (CameraProblem?) -> Unit, onCode: (String) -> Unit) {
    val context = LocalContext.current
    val hasCamera = remember { deviceHasCamera(context.packageManager) }
    var granted by remember {
        mutableStateOf(
            ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) ==
                android.content.pm.PackageManager.PERMISSION_GRANTED,
        )
    }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { allowed ->
        granted = allowed
        onProblem(cameraProblem(hasCamera, allowed))
    }

    LaunchedEffect(hasCamera, granted) {
        val current = cameraProblem(hasCamera, granted)
        onProblem(current)
        if (current == CameraProblem.PermissionRefused && hasCamera) {
            ask.launch(Manifest.permission.CAMERA)
        }
    }

    // Only ever the preview. The problems are the screen's to word.
    if (cameraProblem(hasCamera, granted) == null) CameraPreview(onCode)
}

@Composable
private fun Explanation(message: Int, tag: String) {
    Surface(
        color = MaterialTheme.colorScheme.surfaceVariant,
        shape = MaterialTheme.shapes.medium,
        modifier = Modifier.fillMaxWidth().testTag(tag),
    ) {
        Text(
            text = stringResource(message),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(16.dp),
        )
    }
}

/**
 * A live preview with the analyser bound to it.
 *
 * `PreviewView` through `AndroidView` rather than `camera-compose`: one fewer
 * artifact whose version has to be kept in lockstep with `camera-core`, and the
 * viewfinder is not the interesting part of this screen.
 *
 * The analyser runs on its own single thread. `KEEP_ONLY_LATEST` means a slow
 * decode drops frames instead of queueing them, which is what keeps the preview
 * from lagging behind the phone.
 */
@Composable
private fun CameraPreview(onCode: (String) -> Unit) {
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    val executor = remember { Executors.newSingleThreadExecutor() }
    DisposableEffect(Unit) { onDispose { executor.shutdown() } }

    Box(
        modifier = Modifier.fillMaxWidth().height(280.dp).testTag(TAG_CAMERA_PREVIEW),
        contentAlignment = Alignment.Center,
    ) {
        CircularProgressIndicator()
        AndroidView(
            modifier = Modifier.fillMaxSize(),
            factory = { viewContext ->
                PreviewView(viewContext).also { view ->
                    val providerFuture = ProcessCameraProvider.getInstance(viewContext)
                    providerFuture.addListener({
                        val provider = providerFuture.get()
                        val preview = Preview.Builder().build().also {
                            it.surfaceProvider = view.surfaceProvider
                        }
                        val analysis = ImageAnalysis.Builder()
                            .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                            .build()
                            .also { it.setAnalyzer(executor, QrCodeAnalyser(onCode)) }
                        provider.unbindAll()
                        provider.bindToLifecycle(
                            lifecycleOwner,
                            CameraSelector.DEFAULT_BACK_CAMERA,
                            preview,
                            analysis,
                        )
                    }, ContextCompat.getMainExecutor(viewContext))
                }
            },
        )
    }
}

const val TAG_PAIRING_SCREEN = "pairing-screen"
const val TAG_LABEL_FIELD = "pairing-label"
const val TAG_CODE_FIELD = "pairing-code"
const val TAG_PAIR_BUTTON = "pairing-button"
const val TAG_FAILURE = "pairing-failure"
const val TAG_FAILURE_DETAIL = "pairing-failure-detail"
const val TAG_CAMERA_PREVIEW = "camera-preview"
const val TAG_CAMERA_ABSENT = "camera-absent"
const val TAG_CAMERA_REFUSED = "camera-refused"
const val TAG_PAIRING_BACK = "pairing-back"
