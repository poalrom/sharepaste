package com.sharepaste.android

import androidx.activity.ComponentActivity
import androidx.compose.runtime.getValue
import androidx.compose.ui.test.junit4.AndroidComposeTestRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.hasTestTag
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.platform.UiPreferences
import com.sharepaste.android.ui.HeadMove
import com.sharepaste.android.ui.HistoryScreen
import com.sharepaste.android.ui.PairingsScreen
import com.sharepaste.android.ui.Receipt
import com.sharepaste.android.ui.Screen
import com.sharepaste.android.ui.SharepasteTheme
import com.sharepaste.android.ui.SharepasteViewModel
import com.sharepaste.android.ui.TAG_HISTORY_LIST
import com.sharepaste.android.ui.TAG_PAIRINGS_LIST
import com.sharepaste.android.ui.UiState
import com.sharepaste.android.ui.appActions
import com.sharepaste.core.Entry
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.job
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import java.io.File

/**
 * The phone under test: the app's own repository, its own state holder, and its
 * real screens, over a database this test owns.
 *
 * Assembled from production objects and wired with the production
 * `appActions(model)`, because the wiring is precisely the part a hand-rolled
 * harness gets subtly different — a criterion proven against lambdas the activity
 * does not use is a criterion not proven at all.
 *
 * The composition is the app's own `when` over [Screen] with one branch bent, and
 * that is not a shortcut. `Screen.Pairing` is the camera flow, whose scanner asks
 * for the camera; a system permission dialog takes window focus and leaves the
 * activity `PAUSED` (ticket 09, deviation 4) — and with no focus, every clipboard
 * *read* is denied, which would break the one thing these tests are here to
 * prove. So the pairing branch renders the History instead. Which screen the real
 * `when` picks is ticket 09's business and already has a test.
 */
class PhoneUnderTest private constructor(
    private val compose: AndroidComposeTestRule<*, ComponentActivity>,
    val repo: SharepasteRepository,
    val model: SharepasteViewModel,
    /**
     * The same store the state holder was built over, for a test that drives one
     * of the two switches.
     *
     * There is one DataStore per file in a process, so this is not a second view
     * of the preferences — it is the one the app is using. [close] puts it back to
     * the defaults, which is what keeps a switch a test turned off from silencing
     * a confirmation in every test that runs after it.
     */
    val preferences: UiPreferences,
) {

    private val instrumentation = InstrumentationRegistry.getInstrumentation()

    val clip = Clip(instrumentation.targetContext)

    /** Every Pairing this phone joined here, in the order it joined them. */
    val pairedUserIds = mutableListOf<String>()

    /** The Pairing this phone joined most recently, once it has one. */
    val userId: String? get() = pairedUserIds.lastOrNull()

    val state: UiState get() = model.state.value

    /**
     * Every Receipt this phone has shown, oldest first.
     *
     * Receipts are one-shot events rather than part of [UiState] — they are
     * Toasts, and the activity collects them — so a test cannot read the last
     * one off a snapshot the way it reads a [com.sharepaste.android.ui.Notice].
     * The collector is attached in [open], before anything can press a verb, and
     * it keeps them all: a test that asserted only on the latest could not tell
     * "the Offer said nothing" from "the Offer was overtaken".
     */
    val receipts: List<Receipt> get() = synchronized(seen) { seen.toList() }

    private val seen = mutableListOf<Receipt>()

    /**
     * Every motion the state holder asked the list to make, oldest first.
     *
     * The [Receipt]s' neighbour, collected for the same reason: a `SharedFlow`
     * replays nothing, so a motion raised before anything subscribed is gone. It
     * is what makes the *join* assertable — `OpenJumpTest` pins the gate's
     * sequence and `HistoryListTest` pins what the screen does with a
     * [HeadMove], and neither can say that a real open of a real phone raises
     * one.
     *
     * This is a second subscriber beside the screen's own, not a substitute for
     * it: [open] hands `model.headMoves` to the real [HistoryScreen] as
     * `MainActivity` does, and both collectors see every emission. The state
     * holder's `DROP_OLDEST` buffer of one is left as it is, so two motions
     * raised inside one animation would be dropped here exactly as they would be
     * on a phone — which no test here arranges, and which is the state holder's
     * decision to change if one ever needs to.
     */
    val headMoves: List<HeadMove> get() = synchronized(moved) { moved.toList() }

    private val moved = mutableListOf<HeadMove>()

    /** Pair with a short code the other device minted, exactly as a scan does. */
    fun pairWithCode(inviter: Inviter, label: String): String {
        val paired = runBlocking { repo.pairWithCode(inviter.freshCompactCode(), label) }
        return activate(paired.userId)
    }

    /**
     * Pair against one particular Relay address with an invite of this phone's
     * own.
     *
     * The only way to choose the address: a short code *carries* the inviting
     * device's `server_url` inside its payload, so a phone that pairs by code
     * talks to whatever that device talks to. A test that needs the Relay to be
     * somewhere it can take away has to claim its own invite, and pays a
     * single-use token for it.
     */
    fun pairWithInvite(serverUrl: String, label: String): String {
        val paired = runBlocking { repo.pairWithInvite(serverUrl, TestRelay.nextInvite(), label) }
        return activate(paired.userId)
    }

    /**
     * Make it the Active Pairing, then put the phone back down.
     *
     * Pairing brings a session up on its own — `pair_with_code` ends in
     * `activate_and_sync` and `set_active_pairing` calls `activate_session` — which
     * is right for a real phone and wrong for a test that has not reached its first
     * `onStart` yet. Without this, an Entry a test puts on the Relay "while the
     * phone is closed" arrives over a live stream instead, and the resume it was
     * meant to prove proves nothing. Every test here drives the two lifecycle edges
     * explicitly, so this is the state they all start from.
     */
    private fun activate(paired: String): String {
        pairedUserIds += paired
        runBlocking {
            repo.setActivePairing(paired)
            repo.stopAllSessions()
        }
        return paired
    }

    /**
     * Make one of the Pairings already held the Active one, with no session left
     * running.
     *
     * The set-up counterpart of the production `activatePairing`: a test that
     * pairs twice ends up Active on the second, and the one it wants Active is
     * usually the first.
     */
    fun makeActive(userId: String) {
        runBlocking {
            repo.setActivePairing(userId)
            repo.stopAllSessions()
        }
    }

    // -- the two lifecycle edges, driven where the activity drives them --------

    fun enterForeground() = instrumentation.runOnMainSync { model.onEnterForeground() }

    fun leaveForeground() = instrumentation.runOnMainSync { model.onLeaveForeground() }

    // -- waiting ---------------------------------------------------------------

    /**
     * Wait for the state holder to reach a state, or say what it was stuck on.
     *
     * Everything interesting here arrives from somewhere else — the SSE reader,
     * the uploader, a coroutine the action launched — so almost every assertion
     * has to be preceded by one of these. The message matters: a bare timeout in
     * a suite that talks to a real Relay is indistinguishable between "the phone
     * is wrong" and "the Relay was slow".
     */
    fun await(what: String, timeoutSeconds: Long = TIMEOUT_SECONDS, predicate: (UiState) -> Boolean) {
        try {
            compose.waitUntil(timeoutSeconds * 1_000) { predicate(state) }
        } catch (e: Throwable) {
            throw AssertionError("$what: never happened in ${timeoutSeconds}s. State: $state", e)
        }
    }

    /** Wait for an Entry matching [predicate] and hand it back. */
    fun awaitEntry(what: String, predicate: (Entry) -> Boolean): Entry {
        await(what) { it.entries.any(predicate) }
        return state.entries.first(predicate)
    }

    /**
     * Wait for a Receipt matching [predicate] and hand it back.
     *
     * The Receipt half of [await]. A verb that reports a Receipt writes nothing
     * to [UiState], so `await { it.notice is ... }` on one of those waits for
     * something that will never arrive.
     */
    fun awaitReceipt(
        what: String,
        timeoutSeconds: Long = TIMEOUT_SECONDS,
        predicate: (Receipt) -> Boolean,
    ): Receipt {
        try {
            compose.waitUntil(timeoutSeconds * 1_000) { receipts.any(predicate) }
        } catch (e: Throwable) {
            throw AssertionError(
                "$what: never happened in ${timeoutSeconds}s. Receipts: ${receipts}. " +
                    "State: $state",
                e,
            )
        }
        return receipts.first(predicate)
    }

    /**
     * Wait for the state holder to ask the list to move, and hand the motion back.
     *
     * The motion of an open arrives *after* the rows it is about — the state
     * holder emits once `refreshHistory` has written them — so
     * `await { entries.size == n }` can return a moment before it. Waiting for
     * the snapshot and then reading [headMoves] would read it too early.
     */
    fun awaitHeadMove(what: String, timeoutSeconds: Long = TIMEOUT_SECONDS): HeadMove {
        try {
            compose.waitUntil(timeoutSeconds * 1_000) { headMoves.isNotEmpty() }
        } catch (e: Throwable) {
            throw AssertionError("$what: nothing moved the list in ${timeoutSeconds}s", e)
        }
        return headMoves.last()
    }

    /**
     * Bring a row into view.
     *
     * A `LazyColumn` composes only what is on screen, and the Contact readout plus
     * the foreground-only note take most of a phone's height before the first row
     * — so a row that exists is not necessarily a row a test can press.
     */
    fun scrollTo(tag: String) {
        compose.onNodeWithTag(TAG_HISTORY_LIST).performScrollToNode(hasTestTag(tag))
    }

    /** The same, on the Pairings screen's own list. */
    fun scrollToPairing(tag: String) {
        compose.onNodeWithTag(TAG_PAIRINGS_LIST).performScrollToNode(hasTestTag(tag))
    }

    /**
     * Release the facade, and by default every Pairing this phone made.
     *
     * Forgetting is on by default so a run does not leave Users and Entries on
     * the shared Relay behind it. A test whose whole point is that a Pairing
     * survives the process passes `forgetPairings = false`.
     *
     * **The state holder is stopped first, and waited for.** It answers core
     * events by calling back into the facade — `ActivePairingChanged` re-reads
     * the Pairings — and `forgetPairing` raises exactly those events. Destroying
     * the facade with one of those reads in flight throws
     * `IllegalStateException: Sharepaste object has already been destroyed` out
     * of a coroutine nobody is catching, which fails whichever test happens to
     * be running when it lands. `cancelAndJoin` is the fix rather than a sleep:
     * a call already inside the blocking FFI boundary is not interruptible, so
     * the only safe thing is to wait for it. The shipped app never closes its
     * facade — the process dying is the teardown — so this is a test-lifecycle
     * obligation, and it is `ViewModelStore.clear()` by another name.
     *
     * **The preference store goes back to the defaults too**, in a `finally`. One
     * file serves the whole process, so a switch left off here would silence a
     * confirmation for every test that ran afterwards — and that failure would not
     * look like this class's fault, or like anybody's. The `finally` is the point:
     * the two lines above it are known to throw, which is why the `forgetPairing`
     * loop is wrapped in `runCatching`, and a teardown that gave up before the
     * reset would leave exactly the poisoning this exists to prevent.
     */
    fun close(forgetPairings: Boolean = true) {
        runBlocking {
            try {
                model.viewModelScope.coroutineContext.job.cancelAndJoin()
                if (forgetPairings) {
                    pairedUserIds.forEach { runCatching { repo.forgetPairing(it) } }
                }
                repo.close()
            } finally {
                preferences.resetToDefaults()
            }
        }
    }

    companion object {
        const val TIMEOUT_SECONDS = 60L

        /**
         * A phone with an empty database and its screens on screen.
         *
         * `requireHttps = false`, because the test Relay is plain HTTP and there
         * is no publicly trusted certificate to put in front of it from inside an
         * emulator. That concession is safe only while `TransportPolicyTest`
         * proves the app itself does not make it — leave that test alone.
         *
         * [fresh] deletes the database first, which is what every test that
         * starts from nothing wants. The half of a force-stop test that runs
         * *after* the restart passes `false`: the database surviving is the thing
         * being proven.
         */
        fun open(
            compose: AndroidComposeTestRule<*, ComponentActivity>,
            databaseName: String,
            fresh: Boolean = true,
        ): PhoneUnderTest {
            val context = InstrumentationRegistry.getInstrumentation().targetContext
            if (fresh) {
                listOf("", "-wal", "-shm").forEach {
                    File(context.filesDir, databaseName + it).delete()
                }
            }
            val repo = SharepasteRepository.open(
                context,
                requireHttps = false,
                databaseName = databaseName,
            )
            // A `ViewModel` wants a main looper for `viewModelScope`, so it is
            // built on the main thread exactly as the activity builds it.
            //
            // The preference store is the process's real one, held rather than
            // constructed inline so that a test which drives a switch drives the
            // store the app is actually reading — and so `close` can put all three
            // values back to what a fresh install has.
            val preferences = UiPreferences(context)
            lateinit var model: SharepasteViewModel
            InstrumentationRegistry.getInstrumentation().runOnMainSync {
                model = SharepasteViewModel(repo, preferences)
            }
            val phone = PhoneUnderTest(compose, repo, model, preferences)
            // `receipts` replays nothing, so the collector has to be running
            // before a verb is pressed. This only *schedules* it — the launch
            // dispatches onto the main looper rather than subscribing here — but
            // every verb is pressed through the composition below, and Compose
            // and this collector share that looper. The subscription is queued
            // ahead of anything a test can do, which is the guarantee; a
            // `first()` awaited from this thread would deadlock the looper it
            // needs.
            model.viewModelScope.launch {
                model.receipts.collect { synchronized(phone.seen) { phone.seen += it } }
            }
            // The same argument, and the same looper. A motion raised by the
            // Catch-Up of the first `enterForeground` a test drives would
            // otherwise be gone before anything could read it.
            model.viewModelScope.launch {
                model.headMoves.collect { synchronized(phone.moved) { phone.moved += it } }
            }
            compose.setContent {
                SharepasteTheme {
                    val state by model.state.collectAsStateWithLifecycle()
                    val actions = appActions(model)
                    when (state.screen) {
                        // See the class comment: the camera flow would take window
                        // focus and with it every clipboard read.
                        //
                        // `headMoves` is passed because `MainActivity` passes it.
                        // Left defaulted, this harness rendered the one screen
                        // whose whole behaviour under ADR 0019 is driven by it
                        // with an `emptyFlow()` — a wiring a test cannot see is
                        // exactly what this class exists to refuse.
                        Screen.Pairing, Screen.History ->
                            HistoryScreen(state, actions, headMoves = model.headMoves)
                        Screen.Pairings -> PairingsScreen(state, actions)
                    }
                }
            }
            return phone
        }
    }
}
