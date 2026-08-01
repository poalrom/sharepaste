package com.sharepaste.android.ui

import android.Manifest
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.annotation.StringRes
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
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
 * a shared shrug, and why the typed field is on screen either way rather than
 * behind a camera failure.
 *
 * **The name is the person's, and it comes first.** The field starts empty and
 * pairing is blocked until it is not. The desktop's flow hard-codes a default; a
 * machine's guess at what someone calls their own phone is not a default, it is
 * a thing they have to notice and correct in a list they read later. Name before
 * code, because the code expires after two minutes and the name does not.
 *
 * The footer states the two facts a phone cannot discover for itself: the cipher
 * — ADR 0002 puts disclosure beside pairing, where the choice to trust a Relay
 * is being made — and this build's refusal of a cleartext Relay.
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
     * control would lead nowhere. Adding a Pairing to a phone that has one has
     * its own path in, and a person who changes their mind halfway has to be
     * able to leave without force-quitting.
     */
    onBack: (() -> Unit)? = null,
    scanner: @Composable () -> Unit = {},
) {
    var typedCode by remember { mutableStateOf("") }

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(Fui.Panel)
            .fuiBackdrop()
            .testTag(TAG_PAIRING_SCREEN),
    ) {
        TitleBand(
            title = stringResource(R.string.pair_title),
            onBack = onBack,
            backDescription = stringResource(R.string.pairings_back),
            backTag = TAG_PAIRING_BACK,
        )

        Column(
            modifier = Modifier
                .weight(1f)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = Fui.Gutter, vertical = 18.dp),
            verticalArrangement = Arrangement.spacedBy(18.dp),
        ) {
            // 01 — the name, first, because the code expires and the name does not.
            Step(
                step = stringResource(R.string.pair_step_name),
                heading = stringResource(R.string.pair_label_heading),
                explainer = stringResource(R.string.pair_label_explainer),
            ) {
                FuiTextField(
                    value = state.deviceLabel,
                    onValueChange = onLabelChange,
                    label = stringResource(R.string.pair_label_field),
                    placeholder = stringResource(R.string.pair_label_placeholder),
                    modifier = Modifier.testTag(TAG_LABEL_FIELD),
                )
            }

            Hairline()

            // 02 — the camera, or the reason there isn't one.
            Step(
                step = stringResource(R.string.pair_step_scan),
                heading = stringResource(R.string.pair_scan_heading),
                explainer = stringResource(R.string.pair_scan_explainer),
            ) {
                when (state.camera) {
                    // Caution for the permission, because there is something to
                    // turn on; muted for the hardware, because there is not.
                    CameraProblem.NoCamera ->
                        Explanation(R.string.camera_absent, TAG_CAMERA_ABSENT, Fui.TextMuted)

                    CameraProblem.PermissionRefused ->
                        Explanation(R.string.camera_permission_refused, TAG_CAMERA_REFUSED, Fui.Amber400)

                    null -> Viewfinder(scanner)
                }
            }

            Hairline()

            Step(
                step = null,
                heading = stringResource(R.string.pair_typed_heading),
                explainer = stringResource(R.string.pair_typed_explainer),
            ) {
                FuiTextField(
                    value = typedCode,
                    onValueChange = { typedCode = it },
                    label = stringResource(R.string.pair_typed_field),
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Go),
                    modifier = Modifier.testTag(TAG_CODE_FIELD),
                )
                FuiButton(
                    text = stringResource(
                        if (state.attempt is PairAttempt.Working) R.string.pair_working else R.string.pair_button,
                    ),
                    onClick = { onCode(typedCode) },
                    solid = true,
                    enabled = state.canPair && typedCode.isNotBlank(),
                    modifier = Modifier.fillMaxWidth().testTag(TAG_PAIR_BUTTON),
                )
            }

            (state.attempt as? PairAttempt.Failed)?.let { failed ->
                Failure(failed, onDismissFailure)
            }
        }

        Hairline()
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(34.dp)
                .background(Fui.Recess)
                .padding(horizontal = Fui.Gutter),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(stringResource(R.string.cipher_disclosure), style = Fui.Micro, color = Fui.TextMuted)
            Text(stringResource(R.string.relay_must_be_https), style = Fui.Micro, color = Fui.Amber400)
        }
    }
}

/**
 * A screen's own header: its name, and the way back if there is one.
 *
 * Shared by this screen and the Pairings so that two screens with a back arrow
 * cannot end up with two different arrows in two different places.
 */
@Composable
fun TitleBand(
    title: String,
    backDescription: String,
    backTag: String,
    modifier: Modifier = Modifier,
    onBack: (() -> Unit)? = null,
) {
    Column(modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(52.dp)
                .background(Fui.CyanA08)
                .padding(horizontal = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (onBack != null) {
                Box(
                    modifier = Modifier
                        .size(Fui.Target)
                        .testTag(backTag)
                        // A name, not just a click verb: the glyph is a picture
                        // of the door and "◂" is not what a screen reader should
                        // read out.
                        .semantics { contentDescription = backDescription }
                        .clickable(onClick = onBack, role = Role.Button),
                    contentAlignment = Alignment.Center,
                ) {
                    Text("◂", style = Fui.Glyph, color = Fui.Cyan300, modifier = Modifier.clearAndSetSemantics {})
                }
            }
            Text(
                text = title,
                style = Fui.Heading,
                color = Fui.TextPrimary,
                modifier = Modifier.padding(start = if (onBack != null) 4.dp else 10.dp),
            )
        }
        Hairline()
    }
}

/**
 * One numbered step, or an unnumbered one.
 *
 * The number is drawn beside the heading rather than written into the string,
 * so a step that moves does not need its words re-edited — and so the numeral
 * can carry the emitter colour while the heading stays a heading.
 */
@Composable
private fun Step(
    step: String?,
    heading: String,
    explainer: String,
    content: @Composable () -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (step != null) Text(step, style = Fui.Label, color = Fui.Cyan400)
            Text(heading, style = Fui.Subheading, color = Fui.TextPrimary)
        }
        Text(explainer, style = Fui.Prose, color = Fui.TextBody)
        content()
    }
}

/**
 * The camera, framed and captioned.
 *
 * The caption says what to point it at, and the strip underneath states the
 * Relay's 120-second pairing slot as a rule rather than as a countdown — nothing
 * on this phone knows when the computer printed the code, so a running clock
 * here would be invented.
 */
@Composable
private fun Viewfinder(scanner: @Composable () -> Unit) {
    FuiPanel(
        title = stringResource(R.string.pair_viewfinder_title),
        code = stringResource(R.string.pair_viewfinder_code),
    ) {
        Box(Modifier.fillMaxWidth().height(220.dp).background(Fui.Void1000)) {
            scanner()
            Text(
                text = stringResource(R.string.pair_viewfinder_hint),
                style = Fui.Micro,
                color = Fui.TextMuted,
                textAlign = TextAlign.Center,
                modifier = Modifier.align(Alignment.Center).padding(horizontal = 24.dp),
            )
        }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .background(Fui.Recess)
                .padding(vertical = 6.dp),
            horizontalArrangement = Arrangement.Center,
        ) {
            Text(stringResource(R.string.pair_code_ttl), style = Fui.Micro, color = Fui.Amber400)
        }
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
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(Fui.AlertA16)
            .padding(12.dp)
            .testTag(TAG_FAILURE),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        FuiBadge(stringResource(R.string.pair_failed_badge), Accent.Alert, solid = true)
        Text(stringResource(failed.message), style = Fui.Prose, color = Fui.TextPrimary)
        failed.detail?.let {
            Text(
                text = it,
                style = Fui.Data,
                color = Fui.TextBody,
                modifier = Modifier.testTag(TAG_FAILURE_DETAIL),
            )
        }
        FuiButton(
            text = stringResource(R.string.pair_dismiss),
            onClick = onDismiss,
            accent = Accent.Alert,
            height = Fui.TargetSmall,
        )
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

/**
 * A camera failure, in a slot that is dashed rather than framed.
 *
 * Dashed because the viewfinder is *missing*, not broken: the typed path is
 * already on screen underneath and works just as well, so a solid alert frame
 * would overstate what has gone wrong.
 */
@Composable
private fun Explanation(@StringRes message: Int, tag: String, mark: Color) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .dashedBorder(Fui.Inert)
            .padding(12.dp)
            .testTag(tag),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text("⊘", style = Fui.Glyph, color = mark)
        Text(stringResource(message), style = Fui.Prose, color = Fui.TextBody)
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
        modifier = Modifier.fillMaxSize().testTag(TAG_CAMERA_PREVIEW),
        contentAlignment = Alignment.Center,
    ) {
        CircularProgressIndicator(color = Fui.Cyan400)
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

/**
 * A text field in the console's own voice.
 *
 * Material's outlined field underneath, because a hand-rolled one would owe the
 * platform a cursor, a selection handle, an IME contract and an accessibility
 * tree. Only its colours, shape and type are ours.
 */
@Composable
private fun FuiTextField(
    value: String,
    onValueChange: (String) -> Unit,
    label: String,
    modifier: Modifier = Modifier,
    placeholder: String? = null,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        label = { Text(label, style = Fui.Micro) },
        placeholder = placeholder?.let { { Text(it, style = Fui.Data, color = Fui.TextDim) } },
        singleLine = true,
        shape = RectangleShape,
        textStyle = Fui.Data,
        keyboardOptions = keyboardOptions,
        colors = OutlinedTextFieldDefaults.colors(
            focusedTextColor = Fui.TextPrimary,
            unfocusedTextColor = Fui.TextBody,
            focusedBorderColor = Fui.Cyan400,
            unfocusedBorderColor = Fui.Frame,
            focusedLabelColor = Fui.TextEmitter,
            unfocusedLabelColor = Fui.TextMuted,
            cursorColor = Fui.Cyan400,
            focusedContainerColor = Fui.Recess,
            unfocusedContainerColor = Fui.Recess,
        ),
        modifier = modifier.fillMaxWidth(),
    )
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
