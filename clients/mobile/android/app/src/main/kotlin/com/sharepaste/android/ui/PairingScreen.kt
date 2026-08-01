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
import androidx.compose.foundation.layout.aspectRatio
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
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
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
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.sharepaste.android.R
import com.sharepaste.android.scan.CameraProblem
import com.sharepaste.android.scan.QrCodeAnalyser
import com.sharepaste.android.scan.cameraPermissionGranted
import com.sharepaste.android.scan.cameraProblem
import com.sharepaste.android.scan.deviceHasCamera
import kotlinx.coroutines.delay
import java.util.concurrent.Executors

/**
 * Pairing this phone to a User that already exists.
 *
 * Two ways of filling one field, and then one button. Scanning the square on the
 * computer's pairing pane is the practical one; typing the code printed
 * underneath it is the fallback, and it is load-bearing rather than decorative —
 * it is the only way in when the camera is refused or absent, which is why each of
 * those has its own message rather than a shared shrug, and why the field is on
 * screen either way rather than behind a camera failure.
 *
 * **A scan fills that field. It does not pair.** Somebody who opens this screen
 * points it at the square before reading a word, because the square is the only
 * thing here that looks like an instruction — and the name the Pairing has to
 * carry comes after. So the viewfinder hands its code down to the field and stands
 * down, which leaves one thing left to do and a button that says what it is. See
 * [PairingState.scanned].
 *
 * **The name is the person's, and pairing waits for it.** The field starts empty
 * and the button is dead until it is not. The desktop's flow hard-codes a default;
 * a machine's guess at what someone calls their own phone is not a default, it is
 * a thing they have to notice and correct in a list they read later.
 *
 * The camera arrives through [scanner] rather than being reached for here, and the
 * permission it needs is watched further out still — see [rememberCameraAccess].
 * The screen's job is layout and wording, and it must be renderable from a
 * [PairingState] alone: a screen that binds a camera as a side effect of being
 * composed cannot be asserted about without one, which is precisely the case where
 * the failure messages matter.
 */
@Composable
fun PairingScreen(
    state: PairingState,
    onLabelChange: (String) -> Unit,
    onCodeChange: (String) -> Unit,
    onPair: () -> Unit,
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
    /**
     * Read the camera permission again, now, because somebody asked.
     *
     * The flow already notices a grant on its own. This is the control beside the
     * refusal for the person who has just come back from Settings and would rather
     * press something than trust that it noticed.
     */
    onRecheckCamera: () -> Unit = {},
    scanner: @Composable () -> Unit = {},
) {
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
                        ViewfinderNote(R.string.camera_absent, TAG_CAMERA_ABSENT, Fui.TextMuted)

                    CameraProblem.PermissionRefused -> ViewfinderNote(
                        message = R.string.camera_permission_refused,
                        tag = TAG_CAMERA_REFUSED,
                        mark = Fui.Amber400,
                    ) {
                        FuiButton(
                            text = stringResource(R.string.camera_recheck),
                            onClick = onRecheckCamera,
                            height = Fui.TargetSmall,
                            modifier = Modifier.testTag(TAG_CAMERA_RECHECK),
                        )
                    }

                    // A code already read is the viewfinder's whole job done. The
                    // note is what tells a camera that has stood down apart from a
                    // camera that has failed, and says where the code went.
                    null -> if (state.scanned) {
                        ViewfinderNote(R.string.pair_code_scanned, TAG_CODE_SCANNED, Fui.Cyan400, glyph = "✓")
                    } else {
                        Viewfinder(scanner)
                    }
                }
            }

            Hairline()

            Step(
                step = null,
                heading = stringResource(R.string.pair_typed_heading),
                explainer = stringResource(R.string.pair_typed_explainer),
            ) {
                FuiTextField(
                    value = state.code,
                    onValueChange = onCodeChange,
                    label = stringResource(R.string.pair_typed_field),
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Go),
                    modifier = Modifier.testTag(TAG_CODE_FIELD),
                )
                FuiButton(
                    text = stringResource(
                        if (state.attempt is PairAttempt.Working) R.string.pair_working else R.string.pair_button,
                    ),
                    onClick = onPair,
                    solid = true,
                    enabled = state.canPair,
                    modifier = Modifier.fillMaxWidth().testTag(TAG_PAIR_BUTTON),
                )
            }

            (state.attempt as? PairAttempt.Failed)?.let { failed ->
                Failure(failed, onDismissFailure)
            }
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
 * The caption sits inside the frame, over the preview it describes, so the panel
 * is the viewfinder and nothing else. How long a code lives is not stated here:
 * this phone is the claimer and reads a shortcode carrying no timestamp, so it
 * could only assert the rule, never count it down — and [PairAttempt.Failed]
 * already says it in the one place it is actionable.
 */
@Composable
private fun Viewfinder(scanner: @Composable () -> Unit) {
    FuiPanel(
        title = stringResource(R.string.pair_viewfinder_title),
        code = stringResource(R.string.pair_viewfinder_code),
        modifier = Modifier.testTag(TAG_VIEWFINDER),
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                // The sensor's shape, not a height in dp: a ratio against the
                // width lands the same frame on any phone, and it is the box the
                // preview is cropped *into* — so it has to be a shape somebody can
                // aim a square at. `clipToBounds` is the belt to the TextureView's
                // braces; [CameraPreview] says what happens without either.
                .aspectRatio(4f / 3f)
                .clipToBounds()
                .background(Fui.Void1000),
        ) {
            scanner()
            Text(
                text = stringResource(R.string.pair_viewfinder_hint),
                style = Fui.Micro,
                color = Fui.TextMuted,
                textAlign = TextAlign.Center,
                modifier = Modifier.align(Alignment.Center).padding(horizontal = 24.dp),
            )
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
 * The camera permission, watched from outside the thing it decides.
 *
 * **This shape is the fix for a bug worth naming**, because the one it replaces is
 * easy to write twice. The permission used to be remembered by a composable that
 * existed only while the permission was granted: it reported "refused", the screen
 * swapped the viewfinder for the refusal, the reporter left the composition — and
 * the grant it had just asked for arrived at a `remember` nothing was reading any
 * more. Granting camera access left the refusal on screen until the app was closed
 * and reopened. So the holder lives *beside* the screen instead of inside the
 * branch it steers, and it is composed for as long as the pairing flow is.
 *
 * Three things move it, in order of how much they are relied on. The request's own
 * result, for the dialog this puts up on first sight. `ON_RESUME`, for the person
 * who went to Settings — every route to granting a permission ends with this app
 * coming back to the front, so this is the one that has to work. And a one-second
 * poll while the answer is still no: a belt for those braces, bounded to the
 * refusal it is watching, and it stops the moment there is nothing left to watch.
 *
 * Returns the manual re-check that the refusal offers as a button.
 */
@Composable
fun rememberCameraAccess(onProblem: (CameraProblem?) -> Unit): () -> Unit {
    val context = LocalContext.current
    val hasCamera = remember { deviceHasCamera(context.packageManager) }
    val granted = remember { mutableStateOf(cameraPermissionGranted(context)) }
    // Read through the latest lambda rather than keyed on it: an action bag
    // rebuilt during recomposition would otherwise re-run the ask below.
    val report by rememberUpdatedState(onProblem)

    LaunchedEffect(hasCamera, granted.value) { report(cameraProblem(hasCamera, granted.value)) }

    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        granted.value = it
    }
    // Once per visit to this screen, and only for hardware that exists. Asking on
    // every recomposition is how an app ends up permanently denied by the
    // platform's "don't ask again".
    LaunchedEffect(hasCamera) {
        if (hasCamera && !granted.value) ask.launch(Manifest.permission.CAMERA)
    }

    val lifecycleOwner = LocalLifecycleOwner.current
    DisposableEffect(lifecycleOwner) {
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_RESUME) granted.value = cameraPermissionGranted(context)
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose { lifecycleOwner.lifecycle.removeObserver(observer) }
    }

    LaunchedEffect(hasCamera, granted.value) {
        // A read that agrees with the state leaves it untouched, so this loop runs
        // until the answer changes and is then cancelled by its own key.
        while (hasCamera && !granted.value) {
            delay(CAMERA_POLL_MS)
            granted.value = cameraPermissionGranted(context)
        }
    }

    return remember(hasCamera) {
        {
            granted.value = cameraPermissionGranted(context)
            // A second dialog, if the platform will still show one. On a
            // permission refused twice this returns denied without showing
            // anything, which is why the sentence above the button names Settings.
            if (hasCamera && !granted.value) ask.launch(Manifest.permission.CAMERA)
        }
    }
}

/** How often a refused camera permission is read again. */
private const val CAMERA_POLL_MS = 1_000L

/**
 * A note in the viewfinder's place, in a slot that is dashed rather than framed.
 *
 * Dashed because the viewfinder is *absent* rather than broken — the field it
 * feeds is already on screen underneath and works just as well, so a solid alert
 * frame would overstate every one of the three things this says. Two of them are
 * camera failures and the third is a scan that succeeded, which is why the glyph
 * and its colour are the caller's to choose.
 */
@Composable
private fun ViewfinderNote(
    @StringRes message: Int,
    tag: String,
    mark: Color,
    glyph: String = "⊘",
    action: (@Composable () -> Unit)? = null,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .dashedBorder(Fui.Inert)
            .padding(12.dp)
            .testTag(tag),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(glyph, style = Fui.Glyph, color = mark)
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(stringResource(message), style = Fui.Prose, color = Fui.TextBody)
            action?.invoke()
        }
    }
}

/**
 * A live preview with the analyser bound to it.
 *
 * `PreviewView` through `AndroidView` rather than `camera-compose`: one fewer
 * artifact whose version has to be kept in lockstep with `camera-core`, and the
 * viewfinder is not the interesting part of this screen.
 *
 * **`COMPATIBLE`, not the default.** `PERFORMANCE` gives `PreviewView` a
 * `SurfaceView`, which the system composites rather than the view hierarchy: it
 * honours neither the bounds Compose measured for it nor the order Compose drew
 * in, and `FILL_CENTER` scales its child past those bounds by design. On this
 * screen that painted the camera across the step above it and the code field
 * below. `COMPATIBLE` is a `TextureView` — one texture copy per frame, drawn in
 * place and clipped like any other view.
 *
 * The analyser runs on its own single thread. `KEEP_ONLY_LATEST` means a slow
 * decode drops frames instead of queueing them, which is what keeps the preview
 * from lagging behind the phone.
 *
 * The unbind on disposal is load-bearing rather than tidy: the camera is bound to
 * the *activity's* lifecycle, which outlives this composable, and a scan takes the
 * preview off a screen that stays. Without it the sensor would keep running behind
 * a viewfinder that had already stood down.
 */
@Composable
fun CameraPreview(onCode: (String) -> Unit) {
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    val executor = remember { Executors.newSingleThreadExecutor() }
    val providerFuture = remember { ProcessCameraProvider.getInstance(context) }
    DisposableEffect(Unit) {
        onDispose {
            if (providerFuture.isDone) providerFuture.get().unbindAll()
            executor.shutdown()
        }
    }

    Box(
        modifier = Modifier.fillMaxSize().testTag(TAG_CAMERA_PREVIEW),
        contentAlignment = Alignment.Center,
    ) {
        CircularProgressIndicator(color = Fui.Cyan400)
        AndroidView(
            modifier = Modifier.fillMaxSize(),
            factory = { viewContext ->
                PreviewView(viewContext).also { view ->
                    view.implementationMode = PreviewView.ImplementationMode.COMPATIBLE
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
const val TAG_PAIRING_BACK = "pairing-back"
const val TAG_LABEL_FIELD = "pairing-label"
const val TAG_CODE_FIELD = "pairing-code"
const val TAG_CODE_SCANNED = "pairing-code-scanned"
const val TAG_PAIR_BUTTON = "pairing-button"
const val TAG_FAILURE = "pairing-failure"
const val TAG_FAILURE_DETAIL = "pairing-failure-detail"
const val TAG_VIEWFINDER = "pairing-viewfinder"
const val TAG_CAMERA_PREVIEW = "camera-preview"
const val TAG_CAMERA_ABSENT = "camera-absent"
const val TAG_CAMERA_REFUSED = "camera-refused"
const val TAG_CAMERA_RECHECK = "camera-recheck"
