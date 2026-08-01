package com.sharepaste.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.platform.UiPreferences
import com.sharepaste.android.ui.PairAttempt
import com.sharepaste.android.ui.PairingState
import com.sharepaste.android.ui.SharepasteViewModel
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * `TRY AGAIN` takes the failure back, and the spent code with it.
 *
 * **The fault this exists for.** [SharepasteViewModel.dismissPairFailure] used to
 * clear [PairingState.attempt] and nothing else, so three things outlived a
 * dismissal that reads as "start over", and each one broke something:
 *
 * - the **code** stayed in the field, so a person who pressed `TRY AGAIN` was
 *   looking at the dead code they had just been told was no good;
 * - [PairingState.canPair] therefore went true again, so **PAIR came back to
 *   life** over that same dead code and the next press bought the same failure —
 *   the one assertion here that is the bug itself;
 * - [PairingState.scanned] stayed **latched**, so the viewfinder stayed stood
 *   down and there was no way to read a fresh code, because
 *   [SharepasteViewModel.setPairingCode] only unlatches on a blank field and
 *   the field was not blank.
 *
 * The Device Label is the one thing that must survive: it names the phone, not
 * the attempt, and retyping it after every failed code is busywork.
 *
 * It runs against the real [SharepasteRepository] and the real state holder,
 * because the fault lived in the state holder and a hand-made [PairingState]
 * would only re-state the fix. `PairingMessagesTest` — which the spec named — is
 * the wrong home for exactly that reason: it renders `PairingScreen` from a
 * [PairingState] built in the test and has no state holder to dismiss anything on.
 *
 * **No Relay is involved and none is needed.** The failure is reached offline, on
 * the first line of the core's `pair_with_code`: `decode` rejects anything that is
 * not a short code before a socket is opened, which arrives here as
 * `AppException.BadInput` and renders as `pair_not_a_code`. Asserting *which*
 * sentence came back is what proves this test never touched the network — a
 * `pair_unreachable` would mean it had.
 */
@RunWith(AndroidJUnit4::class)
class TryAgainTest {

    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = instrumentation.targetContext

    private lateinit var repo: SharepasteRepository
    private lateinit var model: SharepasteViewModel

    @Before
    fun open() {
        listOf("", "-wal", "-shm").forEach { File(context.filesDir, DATABASE + it).delete() }
        repo = SharepasteRepository.open(
            context,
            // No relay is contacted at all here; the policy is the permissive one
            // the rest of the instrumented suite uses so it cannot be the reason
            // anything fails. `TransportPolicyTest` proves the app passes `true`.
            requireHttps = false,
            databaseName = DATABASE,
        )
        // A `ViewModel` wants a main looper for `viewModelScope`, so it is built
        // and driven from the main thread exactly as the activity builds it.
        instrumentation.runOnMainSync {
            model = SharepasteViewModel(repo, UiPreferences(context))
        }
    }

    @After
    fun close() {
        // Nothing paired and no preference was written, so the database file is
        // the only thing this test owns.
        runBlocking { repo.close() }
    }

    @Test
    fun dismissing_a_pairing_failure_keeps_the_name_and_clears_the_code_the_scan_and_the_button() {
        instrumentation.runOnMainSync {
            model.setDeviceLabel(LABEL)
            // Through `codeScanned` rather than `setPairingCode`, because the
            // scan latch is one of the three things under test and only the
            // camera path arms it.
            model.codeScanned(NOT_A_CODE)
        }

        // If the setup above ever stopped arming the latch or filling the field,
        // the assertions after the dismissal would pass on a state that was
        // already empty and prove nothing. These two say it started full.
        assertTrue("the scan latch must be armed before there is anything to clear", pairing.scanned)
        assertTrue(
            "the field must hold a code before there is anything to clear",
            pairing.code.isNotBlank(),
        )

        instrumentation.runOnMainSync { model.pairWithCode() }
        val failure = awaitFailure()
        assertEquals(
            "the failure must be the offline decode refusal, not a network one",
            R.string.pair_not_a_code,
            failure.message,
        )
        // The state the person is looking at when `TRY AGAIN` appears: a dead
        // code in the field, and PAIR still live over it.
        assertTrue("PAIR is pressable over the dead code, which is the fault", pairing.canPair)
        Evidence.log("pair failed   = pair_not_a_code; the dead code is still in the field")

        instrumentation.runOnMainSync { model.dismissPairFailure() }

        assertEquals(
            "the Device Label names the phone, not the attempt",
            LABEL,
            pairing.deviceLabel,
        )
        assertTrue("the surviving label must be a real one", pairing.deviceLabel.isNotBlank())
        assertEquals("the spent code must not still be in the field", "", pairing.code)
        assertFalse("a latched scan leaves the viewfinder stood down for good", pairing.scanned)
        assertEquals("the failure is what was dismissed", PairAttempt.Idle, pairing.attempt)
        // The bug, stated as the thing a person can do: PAIR must not be
        // pressable again with the code that just failed.
        assertFalse("PAIR must not come back to life over a dead code", pairing.canPair)
        Evidence.log(
            "try again     = label='${pairing.deviceLabel}' kept; " +
                "code, scan and attempt cleared; canPair=false",
        )
    }

    private val pairing: PairingState
        get() = model.state.value.pairing

    private fun awaitFailure(): PairAttempt.Failed {
        val deadline = System.nanoTime() + TIMEOUT_SECONDS * 1_000_000_000L
        while (System.nanoTime() < deadline) {
            (pairing.attempt as? PairAttempt.Failed)?.let { return it }
            Thread.sleep(50)
        }
        throw AssertionError("the attempt never failed; it is ${pairing.attempt}")
    }

    private companion object {
        const val DATABASE = "try-again-proof.db"

        const val LABEL = "the phone in my pocket"

        /**
         * A code that cannot pair, and cannot even be looked up.
         *
         * `0`, `1` and `8` are not in the base32 alphabet the core decodes with,
         * so `decode` refuses this before it has a relay address to talk to. That
         * is the point: the failure has to be reachable with no relay running and
         * no clock to wait out, or this test would be about the network rather
         * than about what `TRY AGAIN` resets. `ExpiredCodeTest` owns the real
         * 120-second expiry.
         */
        const val NOT_A_CODE = "0000-1111-8888"

        /** Nothing here leaves the process; this is a hang detector, not a wait. */
        const val TIMEOUT_SECONDS = 10L
    }
}
