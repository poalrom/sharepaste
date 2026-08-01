package com.sharepaste.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.platform.UiPreferences
import com.sharepaste.android.ui.Screen
import com.sharepaste.android.ui.SessionPhase
import com.sharepaste.android.ui.SharepasteViewModel
import com.sharepaste.android.ui.Tone
import com.sharepaste.android.ui.toneOf
import com.sharepaste.core.ConnectionState
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Backgrounding tears the session down; resuming re-opens it and backfills.
 *
 * That pair of sentences is the *entire* sync model (ADR 0007), so it is worth a
 * test against a live relay rather than a stub: no WorkManager, no JobScheduler,
 * no foreground service, no push. The two edges are driven on the exact class
 * `MainActivity.onStart`/`onStop` delegate to, so this is not a parallel
 * implementation of the lifecycle — it is the lifecycle, with a facade of its own
 * so it can reach the cleartext test relay.
 *
 * The backfill half is the one that matters most. An Entry offered by the other
 * device *while this phone is backgrounded* has to be there when the phone comes
 * back — and nowhere else, because nothing was listening.
 */
@RunWith(AndroidJUnit4::class)
class SessionLifecycleTest {

    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = instrumentation.targetContext

    private lateinit var repo: SharepasteRepository
    private lateinit var model: SharepasteViewModel
    private var pairedUserId: String? = null

    @Before
    fun open() {
        listOf("", "-wal", "-shm").forEach {
            java.io.File(context.filesDir, DATABASE + it).delete()
        }
        repo = SharepasteRepository.open(
            context,
            // The test relay is plain HTTP. `TransportPolicyTest` proves the app
            // itself passes `true`.
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
        runBlocking {
            pairedUserId?.let { runCatching { repo.forgetPairing(it) } }
            repo.close()
        }
    }

    @Test
    fun backgrounding_tears_the_session_down_and_resuming_re_opens_it_and_backfills() {
        val other = Inviter.shared()
        val code = other.freshCompactCode()

        // Pair, which is what a fresh install does before any of this applies.
        val paired = runBlocking { repo.pairWithCode(code, "lifecycle test phone") }
        pairedUserId = paired.userId
        runBlocking { repo.setActivePairing(paired.userId) }
        Evidence.log("lifecycle     = paired user=${paired.userId}")

        // --- onStart -------------------------------------------------------
        instrumentation.runOnMainSync { model.onEnterForeground() }
        awaitPhase("in contact after the first onStart") { it is SessionPhase.InContact }
        assertEquals(Screen.History, model.state.value.screen)
        assertTrue("the app must know it is in front", model.state.value.foreground)
        Evidence.log("onStart       = ${model.state.value.session}")

        // --- onStop --------------------------------------------------------
        instrumentation.runOnMainSync { model.onLeaveForeground() }
        awaitPhase("resting after onStop") { it is SessionPhase.Resting }
        assertTrue(
            "onStop must not leave the app thinking it is in front",
            !model.state.value.foreground,
        )
        // The core reads `DISCONNECTED` from here on: `stop_all_sessions` walks
        // every session it stopped to that state before it returns, because a
        // Pairing row rendering `Online` for a session that no longer exists was
        // its own bug. What survives the teardown is **Contact** — the last
        // moment this device had a live connection, flushed to the account row on
        // the way out — so the next `onStart` has something to render before the
        // Relay answers. `stop_all_sessions_ends_the_streams_but_forgets_nothing`
        // pins that half.
        //
        // The screen still must not read `connectionState` for this, and does not:
        // `onLeaveForeground` states `Resting` itself. `Disconnected` is also what
        // a Pairing no session has ever run for reads, and what one that is merely
        // out of contact reads, so it cannot tell a phone that was put down from a
        // phone that cannot reach the Relay. `SharepasteRepository.heldSessions`
        // makes the same argument for the same reason.
        assertEquals(
            "the resting phase must not be derived from a connection state at all",
            SessionPhase.Resting(paired.userId),
            model.state.value.session,
        )
        assertEquals(
            "resting is nominal, never a fault",
            Tone.Nominal,
            toneOf(model.state.value.session),
        )
        Evidence.log(
            "onStop        = ${model.state.value.session}; " +
                "core reads ${runBlocking { repo.connectionState(paired.userId) }} " +
                "(Contact is the reading that survives a teardown, not this one)",
        )

        // --- something happens while the phone is not listening -------------
        //
        // This is the real proof the stream came down, and it is stronger than any
        // status enum: the other device puts an Entry on the Relay and nothing
        // reaches this phone, because nothing here is listening.
        val offered = "offered-while-backgrounded-${System.currentTimeMillis()}"
        other.offer(offered)
        Evidence.log("while resting = the other device offered an Entry")
        Thread.sleep(BACKGROUND_WINDOW_MS)
        assertTrue(
            "the Entry offered while the session was down arrived anyway; the teardown did not " +
                "happen. That is the whole of ADR 0007 — nothing syncs while the app is closed.",
            // Named rather than asserted as an empty History: ticket 10 added tests
            // that pair into this same inviting User and leave Entries of their own
            // on the Relay, so "nothing at all is here" is a fact about test order
            // and not about this teardown. What this test is about is *this* Entry.
            model.state.value.entries.none { it.preview == offered },
        )

        // --- onStart again -------------------------------------------------
        instrumentation.runOnMainSync { model.onEnterForeground() }
        awaitPhase("in contact after the second onStart") { it is SessionPhase.InContact }
        Evidence.log("onStart again = ${model.state.value.session}")

        val backfilled = awaitEntry("the Entry offered while backgrounded must be backfilled") {
            it.preview == offered
        }
        Evidence.log("backfilled    = Entry id=${backfilled.id} preview=${backfilled.preview}")
        assertEquals(
            "the backfilled Entry is not the one that was offered",
            offered,
            runBlocking { repo.readEntry(paired.userId, backfilled.id) },
        )
    }

    private fun awaitPhase(what: String, predicate: (SessionPhase) -> Boolean) {
        val deadline = System.nanoTime() + TIMEOUT_SECONDS * 1_000_000_000L
        while (System.nanoTime() < deadline) {
            if (predicate(model.state.value.session)) return
            Thread.sleep(100)
        }
        throw AssertionError("$what: never happened; phase is ${model.state.value.session}")
    }


    private fun awaitEntry(
        what: String,
        predicate: (com.sharepaste.core.Entry) -> Boolean,
    ): com.sharepaste.core.Entry {
        val deadline = System.nanoTime() + TIMEOUT_SECONDS * 1_000_000_000L
        while (System.nanoTime() < deadline) {
            model.state.value.entries.firstOrNull(predicate)?.let { return it }
            Thread.sleep(100)
        }
        throw AssertionError("$what: it never arrived after ${TIMEOUT_SECONDS}s")
    }

    private companion object {
        const val DATABASE = "lifecycle-proof.db"
        const val TIMEOUT_SECONDS = 60L

        /**
         * How long the phone stays "backgrounded" while the other device offers.
         *
         * Long enough for the other device's uploader to get the Entry onto the
         * Relay — otherwise the "nothing arrived" assertion would pass because
         * nothing had been sent yet, which proves nothing at all.
         */
        const val BACKGROUND_WINDOW_MS = 5_000L
    }
}
