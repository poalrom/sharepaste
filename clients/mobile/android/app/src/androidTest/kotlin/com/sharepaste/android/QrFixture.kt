package com.sharepaste.android

import android.graphics.Bitmap
import android.graphics.Rect
import android.media.Image
import androidx.camera.core.ImageInfo
import androidx.camera.core.ImageProxy
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.common.BitMatrix
import com.google.zxing.qrcode.QRCodeWriter
import java.nio.ByteBuffer

/**
 * A camera frame carrying a QR code, without a camera.
 *
 * An emulated camera cannot be pointed at a laptop screen, so the optics are the
 * one part of the scan path that cannot be tested here. **Everything above the
 * optics can**, and this is what makes that possible: a real QR code, rendered to
 * a real luminance plane, handed to the real
 * [com.sharepaste.android.scan.QrCodeAnalyser.analyze] through the real
 * [ImageProxy] interface CameraX calls it with.
 *
 * The plane is deliberately **padded** — `rowStride` is wider than `width` — which
 * is what a good many real sensors deliver and what the analyser has to handle.
 * Passing `width` where `rowStride` belongs does not throw; it skews every row by
 * the padding and decodes nothing at all, so an unpadded fixture would let that
 * bug through.
 */
class QrImageProxy(
    private val plane: ByteArray,
    private val rowStride: Int,
    private val imageWidth: Int,
    private val imageHeight: Int,
) : ImageProxy {

    /** Whether the analyser handed the frame back. CameraX stalls if it does not. */
    var closed: Boolean = false
        private set

    override fun close() {
        closed = true
    }

    override fun getCropRect(): Rect = Rect(0, 0, imageWidth, imageHeight)

    override fun setCropRect(rect: Rect?) = Unit

    override fun getFormat(): Int = android.graphics.ImageFormat.YUV_420_888

    override fun getHeight(): Int = imageHeight

    override fun getWidth(): Int = imageWidth

    override fun getPlanes(): Array<ImageProxy.PlaneProxy> = arrayOf(
        object : ImageProxy.PlaneProxy {
            // Qualified, and it has to be: an unqualified `rowStride` here binds
            // to *this* object's own synthetic property for the `getRowStride`
            // being declared, not to the outer class's constructor parameter, and
            // recurses until the stack runs out.
            override fun getRowStride(): Int = this@QrImageProxy.rowStride

            override fun getPixelStride(): Int = 1

            override fun getBuffer(): ByteBuffer = ByteBuffer.wrap(this@QrImageProxy.plane)
        },
    )

    override fun getImageInfo(): ImageInfo = object : ImageInfo {
        override fun getTimestamp(): Long = 0

        override fun getRotationDegrees(): Int = 0

        override fun getSensorToBufferTransformMatrix(): android.graphics.Matrix =
            android.graphics.Matrix()

        override fun getTagBundle(): androidx.camera.core.impl.TagBundle =
            androidx.camera.core.impl.TagBundle.emptyBundle()

        // No EXIF: there was no sensor, and the analyser does not read it.
        override fun populateExifData(builder: androidx.camera.core.impl.utils.ExifData.Builder) = Unit
    }

    override fun getImage(): Image? = null

    companion object {

        /**
         * Render `contents` as a QR code and wrap it as a camera frame.
         *
         * `padding` widens `rowStride` past the image width, so the fixture looks
         * like a padded sensor rather than a tidy one.
         */
        fun of(contents: String, side: Int = 480, padding: Int = 17): QrImageProxy {
            val matrix: BitMatrix = QRCodeWriter().encode(
                contents,
                BarcodeFormat.QR_CODE,
                side,
                side,
                // A quiet zone, because a code flush against the frame edge is
                // one a detector is entitled to miss — and a real screen has one.
                mapOf(EncodeHintType.MARGIN to 2),
            )
            val width = matrix.width
            val height = matrix.height
            val rowStride = width + padding
            val plane = ByteArray(rowStride * height)
            for (y in 0 until height) {
                val row = y * rowStride
                for (x in 0 until width) {
                    // Luminance, which is all a QR code is: black is 0, white is
                    // the top of the range. The padding stays 0 and must never be
                    // read — if it is, the decode fails, which is the point.
                    plane[row + x] = if (matrix.get(x, y)) 0 else 0xFF.toByte()
                }
            }
            return QrImageProxy(plane, rowStride, width, height)
        }
    }
}

/** The same code as a bitmap, for the record rather than for the decode. */
fun BitMatrix.toBitmap(): Bitmap {
    val pixels = IntArray(width * height)
    for (y in 0 until height) {
        for (x in 0 until width) {
            pixels[y * width + x] = if (get(x, y)) 0xFF000000.toInt() else 0xFFFFFFFF.toInt()
        }
    }
    return Bitmap.createBitmap(pixels, width, height, Bitmap.Config.ARGB_8888)
}
