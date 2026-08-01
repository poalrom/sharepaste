package com.sharepaste.android.scan

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.MultiFormatReader
import com.google.zxing.NotFoundException
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.common.HybridBinarizer

/**
 * The two reasons a phone cannot scan, kept apart.
 *
 * They are not interchangeable and must not share a message: "turn camera
 * access on" is useless advice on a device with no camera, and "this phone has
 * no camera" is a lie when the person simply said no to a permission prompt.
 */
enum class CameraProblem {
    /** Someone declined the camera permission, or a policy declines it for them. */
    PermissionRefused,

    /** There is no camera on this device Sharepaste could use. */
    NoCamera,
}

/**
 * Which camera problem applies, or `null` when scanning can go ahead.
 *
 * Order matters. Absent hardware wins over a refused permission, because a
 * device with no camera also has no permission granted, and the *useful* thing
 * to say is the one the person can act on.
 */
fun cameraProblem(hasCamera: Boolean, permissionGranted: Boolean): CameraProblem? = when {
    !hasCamera -> CameraProblem.NoCamera
    !permissionGranted -> CameraProblem.PermissionRefused
    else -> null
}

/** Whether this device has any camera at all, front or back. */
fun deviceHasCamera(packageManager: PackageManager): Boolean =
    packageManager.hasSystemFeature(PackageManager.FEATURE_CAMERA_ANY)

/**
 * Whether this app may use the camera, right now.
 *
 * A function rather than a remembered flag on purpose: the answer changes while
 * the app is running — a grant from the platform's own dialog, or from Settings
 * with the app still in the back stack — and every caller here is somewhere that
 * has just been told to ask again.
 */
fun cameraPermissionGranted(context: Context): Boolean =
    context.checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED

/**
 * Reads the pairing code out of camera frames.
 *
 * ZXing rather than ML Kit, and that is a privacy decision rather than a
 * preference: ML Kit's unbundled variant downloads its models from Google on
 * first use, and ticket 13 has to be able to prove this app makes no request to
 * anything but the Relay. ZXing is one pure-Java jar that decodes in this
 * process and never opens a socket.
 *
 * The frame is read as luminance only — the Y plane of the YUV_420_888 image
 * CameraX delivers — which is all a QR code carries and a third of the bytes.
 * Rotation is not corrected: ZXing locates a code by its three finder patterns,
 * so a phone held sideways decodes the same code as a phone held upright.
 *
 * [onCode] is called on the analyser's executor, not the main thread, and may be
 * called repeatedly for the same code while it stays in frame. Deciding what to
 * do with the second one is the state holder's job, not this class's.
 */
class QrCodeAnalyser(private val onCode: (String) -> Unit) : ImageAnalysis.Analyzer {

    /**
     * One reader, reused.
     *
     * `setHints` once and `decodeWithState` per frame is ZXing's documented
     * continuous-scan path: the single-argument `decode` re-runs `setHints(null)`
     * and reallocates every reader on every frame. Confined to the analyser's
     * executor, because a `MultiFormatReader` is not thread-safe.
     */
    private val reader = MultiFormatReader().apply {
        setHints(mapOf(DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE)))
    }

    /**
     * The luminance plane, carried between frames rather than reallocated.
     *
     * At 30 frames a second a fresh array per frame is a megabyte a second of
     * garbage for no reason. Grown when a frame needs more and never shrunk.
     */
    private var luminance = ByteArray(0)

    override fun analyze(image: ImageProxy) {
        try {
            decode(image)?.let(onCode)
        } finally {
            // Without this the pipeline stalls after a handful of frames: CameraX
            // hands out a fixed number of buffers and waits for them back.
            image.close()
        }
    }

    private fun decode(image: ImageProxy): String? {
        val plane = image.planes[0]
        val buffer = plane.buffer
        buffer.rewind()
        val size = buffer.remaining()
        if (luminance.size < size) luminance = ByteArray(size)
        buffer.get(luminance, 0, size)

        val source = PlanarYUVLuminanceSource(
            luminance,
            // `rowStride`, deliberately, not `image.width`. The Y plane is padded
            // on plenty of real sensors, and passing the width there does not
            // fail — it skews every row by the padding and silently decodes
            // nothing, which is the worst kind of wrong.
            plane.rowStride,
            image.height,
            0,
            0,
            image.width,
            image.height,
            false,
        )
        return try {
            reader.decodeWithState(BinaryBitmap(HybridBinarizer(source))).text
        } catch (_: NotFoundException) {
            // No code in this frame, which is true of almost every frame. ZXing
            // returns a preallocated instance for exactly this reason; it is
            // control flow, not an error, and must not be logged.
            null
        }
    }
}
