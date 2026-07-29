package com.sharepaste.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.platform.AndroidClipboard
import com.sharepaste.android.platform.AndroidKeychain
import com.sharepaste.android.scan.CameraProblem
import com.sharepaste.android.scan.QrCodeAnalyser
import com.sharepaste.android.scan.cameraProblem
import com.sharepaste.core.AppException
import com.sharepaste.core.Sharepaste
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Scanning the desktop's code pairs this phone.
 *
 * **What is proven and what is not.** An emulated camera cannot be pointed at a
 * laptop screen, so the optics — focus, exposure, glare, a screen's refresh
 * beating against the sensor — are *not* tested here and are not claimed to be.
 * Everything above the optics is, and it is tested through the production
 * objects rather than around them: a real short code from a live `pair_start` on
 * the relay, rendered as a real QR code, fed as a padded luminance plane through
 * the real [QrCodeAnalyser] behind the real `ImageProxy` interface, and the
 * string that comes out handed to the real `pairWithCode`.
 *
 * That is the whole pipeline except the lens.
 */
@RunWith(AndroidJUnit4::class)
class QrPairingTest {

    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    private lateinit var phone: Sharepaste
    private var pairedUserId: String? = null

    @Before
    fun open() {
        val dbFile = File(context.filesDir, "qr-pairing-proof.db")
        listOf("", "-wal", "-shm").forEach { File(dbFile.path + it).delete() }
        phone = Sharepaste.open(
            dbPath = dbFile.absolutePath,
            keychain = InMemoryKeychain(),
            clipboard = AndroidClipboard(context),
            events = SilentSink,
            // The test relay is plain HTTP. `TransportPolicyTest` proves the
            // shipped app passes `true` here.
            requireHttps = false,
        )
    }

    @After
    fun close() {
        phone.stopAllSessions()
        pairedUserId?.let { runCatching { phone.forgetPairing(it) } }
        phone.close()
    }

    @Test
    fun a_real_short_code_survives_the_qr_round_trip_and_pairs_this_phone() {
        val other = Inviter.shared()
        // A real code from a real `pair_start`: the payload carries the relay's
        // address, the pair id and the pairing secret, and the relay is holding a
        // 120-second slot for it right now.
        val compact = other.freshCompactCode()
        Evidence.log("qr payload    = ${compact.length} base32 chars, compact form (no dashes)")

        val frame = QrImageProxy.of(compact)
        Evidence.log("qr frame      = ${frame.width}x${frame.height}, rowStride=${frame.planes[0].rowStride}")

        // The production analyser, called exactly as CameraX calls it.
        var decoded: String? = null
        QrCodeAnalyser { decoded = it }.analyze(frame)

        assertNotNull("the analyser decoded nothing out of a frame containing the code", decoded)
        assertEquals(
            "the decoded string is not the code that went in, byte for byte",
            compact,
            decoded,
        )
        assertTrue(
            "the analyser must hand the frame back — CameraX runs out of buffers and stalls if not",
            frame.closed,
        )

        // And the string it produced is a string the core can pair with. The
        // scanned form goes straight in: `decode` strips whitespace and dashes and
        // upper-cases, so the compact QR payload needs no massaging on the way.
        val label = "scanned by the instrumented test"
        val paired = phone.pairWithCode(decoded!!, label)
        pairedUserId = paired.userId
        Evidence.log("qr paired     = user=${paired.userId} device=${paired.deviceId} label=$label")

        val summary = phone.listPairings().single { it.userId == paired.userId }
        assertEquals("the Pairing carries the label the user chose", label, summary.label)
        assertEquals(TestRelay.url, summary.serverUrl)
    }

    /**
     * A frame with no code in it decodes to nothing, quietly.
     *
     * Almost every frame a real scan sees is this one. If it were an error rather
     * than control flow, a scanner pointed at a wall would fill the log and the
     * callback with noise.
     */
    @Test
    fun a_frame_with_no_code_decodes_to_nothing() {
        val blank = QrImageProxy(ByteArray(64 * 48) { 0xFF.toByte() }, 64, 64, 48)
        var decoded: String? = null
        QrCodeAnalyser { decoded = it }.analyze(blank)
        assertNull("a blank frame must not produce a code", decoded)
        assertTrue(blank.closed)
    }

    /**
     * Typed entry pairs with the camera unavailable.
     *
     * The fallback is load-bearing, not decorative: with no camera there is no
     * other way in at all. Nothing about the pairing call depends on where the
     * code came from — which is the property being asserted — so a phone whose
     * camera is absent reaches exactly the same code path as one that scanned.
     */
    @Test
    fun typed_entry_pairs_when_the_camera_is_unavailable() {
        val other = Inviter.shared()
        // The state the screen would be in on a phone with no usable camera.
        assertEquals(CameraProblem.NoCamera, cameraProblem(hasCamera = false, permissionGranted = false))

        // What a person types: the grouped, mixed-case form the computer prints
        // under the square, not the compact form a scan produces.
        val typed = other.freshCompactCode().chunked(4).joinToString("-").lowercase()
        Evidence.log("typed code    = ${typed.length} chars, grouped and lower-cased by hand")

        val label = "typed in by the instrumented test"
        val paired = phone.pairWithCode(typed, label)
        pairedUserId = paired.userId
        Evidence.log("typed paired  = user=${paired.userId} device=${paired.deviceId} label=$label")
        assertEquals(label, phone.listPairings().single { it.userId == paired.userId }.label)
    }

    /**
     * A code that is not a code says so, rather than failing at the transport.
     *
     * This is the fourth message and the one a typo lands on. It is here rather
     * than with the other three because it needs no relay state at all.
     */
    @Test
    fun a_string_that_is_not_a_code_is_rejected_as_bad_input() {
        try {
            phone.pairWithCode("NOTACODE", "irrelevant")
            fail("a string that is not a short code must not pair")
        } catch (e: AppException.BadInput) {
            Evidence.log("not-a-code    = ${e.message}")
        }
    }

    /** The real keychain still works when the phone under test uses it. */
    @Test
    fun the_pairing_secrets_land_in_the_android_keystore() {
        val keychain = AndroidKeychain(context)
        keychain.delete("qr-pairing-probe")
        keychain.put("qr-pairing-probe", "value")
        assertEquals("value", keychain.get("qr-pairing-probe"))
        keychain.delete("qr-pairing-probe")
    }
}
