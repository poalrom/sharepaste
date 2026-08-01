package com.sharepaste.android

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.sharepaste.android.scan.QrCodeAnalyser
import com.sharepaste.android.ui.PairAttempt
import com.sharepaste.android.ui.appActions
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * A scanned code goes into the code field, and nothing else happens.
 *
 * **The fault this exists for.** A scan used to pair. On a fresh install the square
 * is the first thing anybody points the phone at — before reading the step above it
 * — so the scan arrived with the device name still empty, pairing refused it, and a
 * code with a two-minute life had been spent on the sentence "name this phone
 * first". The camera now hands its code down to the field and stops there, which
 * leaves the name as the only thing outstanding and the code still good.
 *
 * The chain is the production one from the frame inwards: the real
 * [QrCodeAnalyser], called the way CameraX calls it, wired through the real
 * `appActions(model)` — the activity's own bag, not a lambda assembled here — into
 * the real state holder. Only the optics are missing, and they are missing for the
 * reason [QrPairingTest] gives: an emulated camera cannot be pointed at a laptop
 * screen.
 */
@RunWith(AndroidJUnit4::class)
class ScannedCodeReachesTheFieldTest {

    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private lateinit var phone: PhoneUnderTest

    @Before
    fun open() {
        phone = PhoneUnderTest.open(compose, DATABASE)
    }

    @After
    fun close() = phone.close()

    @Test
    fun the_first_code_read_fills_the_field_and_the_frames_after_it_do_not() {
        val actions = appActions(phone.model)
        val scanner = QrCodeAnalyser(actions.codeScanned)

        scanner.analyze(QrImageProxy.of(FIRST))
        assertEquals("the field must hold the code that was scanned", FIRST, phone.state.pairing.code)
        assertTrue(
            "a code already read leaves nothing to point a camera at, so the viewfinder goes",
            phone.state.pairing.scanned,
        )

        // Nothing was sent, and — the whole point — nothing was refused. A scan
        // that paired would be sitting on `pair_needs_a_name` right here.
        assertEquals(
            "a scan must not pair, and must not fail either",
            PairAttempt.Idle,
            phone.state.pairing.attempt,
        )
        Evidence.log("scan -> field = $FIRST, viewfinder stood down, nothing attempted")

        // The analyser fires on every frame a code stays in view, and a hand holding
        // a phone drifts onto whatever else is on the desk. The first code wins:
        // this is the gate that turns a stream of decodes into one field.
        scanner.analyze(QrImageProxy.of(SECOND))
        assertEquals(
            "a later frame must not overwrite the code already read",
            FIRST,
            phone.state.pairing.code,
        )

        // And emptying the field is the way back to the camera — the only way, and
        // the reason there is no control for it beside the field it would duplicate.
        actions.setPairingCode("")
        assertFalse(
            "an empty field is how somebody asks for the viewfinder back",
            phone.state.pairing.scanned,
        )
        Evidence.log("cleared field = viewfinder returns")
    }

    private companion object {
        const val DATABASE = "scanned-code-field.db"

        /**
         * Two codes that are not each other, in the compact form a QR carries.
         *
         * Neither is a code any Relay is holding a slot for, and neither needs to
         * be: what is under test ends at the field. Pairing with a code is
         * [QrPairingTest]'s, against a real 120-second slot.
         */
        const val FIRST = "K7QF3M2XA9BZTY6WD4NS8HJC"
        const val SECOND = "P4RTUV5XY7ZA2BC3DE6FG8HJ"
    }
}
