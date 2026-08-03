package com.sharepaste.android

import android.content.Context
import com.sharepaste.android.platform.AndroidClipboard
import com.sharepaste.android.platform.AndroidKeychain
import com.sharepaste.android.platform.FlowEventSink
import com.sharepaste.core.AppException
import com.sharepaste.core.Clipboard
import com.sharepaste.core.ConnectionState
import com.sharepaste.core.CoreEvent
import com.sharepaste.core.Entry
import com.sharepaste.core.HistoryCursor
import com.sharepaste.core.OfferOutcome
import com.sharepaste.core.PairedDevice
import com.sharepaste.core.PairingSummary
import com.sharepaste.core.RecallSource
import com.sharepaste.core.Recalled
import com.sharepaste.core.Settings
import com.sharepaste.core.SettingsPatch
import com.sharepaste.core.Sharepaste
import com.sharepaste.core.ShortCode
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.onSubscription
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import java.io.File
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.locks.ReentrantReadWriteLock
import kotlin.concurrent.read
import kotlin.concurrent.write

/**
 * What became of an Offered Capture, including the one outcome the core has no
 * word for.
 *
 * The core is asked about a Pairing it has been given the id of, so "nothing is
 * paired on this device" is not a failure it can report. It is not a failure at
 * all — it is the ordinary state of a fresh install — and it has to be a value,
 * because the callers that need it least are the ones with nowhere to put an
 * exception: ticket 12's Standing Action runs in a transparent activity with no
 * composition to raise a screen from.
 */
sealed interface OfferAttempt {

    /** No Active Pairing, so there is nothing to offer to. */
    data object Unpaired : OfferAttempt

    /**
     * The core's own verdict on the text: queued, recognised as something this
     * device already held, or refused with a reason.
     */
    data class Settled(val userId: String, val outcome: OfferOutcome) : OfferAttempt
}

/** What became of a Recall Latest. See [OfferAttempt] for why `Unpaired` is a value. */
sealed interface RecallAttempt {

    /** No Active Pairing, so there is no History to take the newest Entry from. */
    data object Unpaired : RecallAttempt

    /**
     * The Entry is on this device's clipboard.
     *
     * [fromCache] is the field that has to reach the person. Recall Latest
     * always attempts the round trip; `RecallSource.CACHE` means it failed, so
     * what was just handed over is the newest Entry *this device already had*
     * and may be yesterday's link. A caller that drops this flag turns a
     * correct operation into a silently wrong one.
     *
     * Carries no plaintext on purpose. The core has already put it on the
     * clipboard, and a secret that nothing needs is a secret something logs.
     */
    data class Done(
        val userId: String,
        val entryId: Long,
        val createdAt: Long,
        val fromCache: Boolean,
    ) : RecallAttempt
}

/**
 * The only thing in this application that touches the core.
 *
 * The FFI boundary is blocking: every call runs the operation to completion on
 * the core's runtime and returns a plain value. So **no call may happen on the
 * main thread**, and the way that rule is kept is by having exactly one place
 * where calls happen at all. Every method below is `suspend` and every one of
 * them hops to [Dispatchers.IO]; a screen that reaches past this class for the
 * `Sharepaste` object has broken the rule, and `StrictMode` in a debug build
 * will say so.
 *
 * **Opening is a blocking call too** — it creates a SQLite connection, runs the
 * migrations and stands up a tokio runtime — so [open] returns immediately with
 * the facade still opening on [Dispatchers.IO], and every method awaits it. That
 * keeps the rule true of construction as well as of use, and it means a caller
 * on the main thread (an `Activity`, a `ViewModelProvider.Factory`) never has to
 * be a coroutine just to hold one of these.
 *
 * Events do not come back through here — they arrive on [events], because they
 * are raised by the core's own tasks rather than in reply to a call.
 */
class SharepasteRepository private constructor(
    private val scope: CoroutineScope,
    private val opening: Deferred<Sharepaste>,
    /**
     * The same clipboard the core was handed.
     *
     * One object, so an Offered Capture reads the clipboard through exactly the
     * rules a Recall writes it through — and so there is no second opinion here
     * about what counts as text on this platform.
     */
    private val clipboard: Clipboard,
    /** Everything the core raises, already off the thread that raised it. */
    override val events: SharedFlow<CoreEvent>,
) : PendingQueue {

    /**
     * Orders [close] against the calls already inside the FFI boundary.
     *
     * A read lock per call and the write lock to close: readers do not contend
     * with each other, so this costs an uncontended lock per operation and buys
     * the one guarantee the boundary cannot give itself — the Rust object is not
     * destroyed while a call is standing on it.
     */
    private val lifetime = ReentrantReadWriteLock()

    @Volatile
    private var closed = false

    /**
     * The Pairings this process is holding a session for.
     *
     * Not a second opinion about core state — a record of **this shell's own
     * acts**, which is a different fact and the one [drainPending] needs: a
     * Standing Action has to put down the session it brought up and has to
     * leave alone the one an open screen is using. Asking the core would not
     * answer it anyway. `connectionState` reads `Disconnected` for a Pairing no
     * session has ever run for, for one whose session was stopped, and for one
     * merely out of contact, and the map that does know (`sync_tasks`) is not on
     * the FFI.
     *
     * It cannot drift. A session ends only by being stopped — the core's two
     * tasks retry rather than exit — and every start and stop this application
     * makes goes through the three methods below, because this class is the only
     * thing that touches the core at all.
     *
     * Concurrent because [io] resumes its callers on whatever thread the FFI
     * call finished on.
     */
    private val heldSessions: MutableSet<String> = ConcurrentHashMap.newKeySet()

    // -- pairings and sessions ----------------------------------------------

    suspend fun listPairings(): List<PairingSummary> = io { it.listPairings() }

    suspend fun pairWithInvite(serverUrl: String, token: String, deviceLabel: String): PairedDevice =
        io { it.pairWithInvite(serverUrl, token, deviceLabel) }

    suspend fun pairStart(userId: String): ShortCode = io { it.pairStart(userId) }

    suspend fun pairWithCode(code: String, deviceLabel: String): PairedDevice =
        io { it.pairWithCode(code, deviceLabel) }

    suspend fun forgetPairing(userId: String) = io { it.forgetPairing(userId) }

    suspend fun setActivePairing(userId: String) = io { it.setActivePairing(userId) }

    suspend fun activePairing(): String? = io { it.activePairing() }

    suspend fun resumeActivePairing(): String? = io { it.resumeActivePairing() }

    override suspend fun startSession(userId: String) {
        io { it.startSession(userId) }
        // Recorded after the call, so a start that raised — no route to the
        // Relay, a Pairing that would not unlock — is not a session this process
        // believes it holds.
        heldSessions.add(userId)
    }

    override suspend fun stopSession(userId: String) {
        // Forgotten before the call rather than after it: the record says "this
        // process asked for a session and has not given it up", and asking is
        // exactly what it has just stopped doing.
        heldSessions.remove(userId)
        io { it.stopSession(userId) }
    }

    suspend fun stopAllSessions() {
        heldSessions.clear()
        io { it.stopAllSessions() }
    }

    override fun holdsSession(userId: String): Boolean = userId in heldSessions

    override suspend fun pendingOn(userId: String): Long? =
        io { core -> core.listPairings().firstOrNull { it.userId == userId }?.pending }

    suspend fun connectionState(userId: String): ConnectionState = io { it.connectionState(userId) }

    // -- history and clipboard ----------------------------------------------

    /**
     * One page of the History, last use first.
     *
     * [before] is the `lastUse` and `id` of the last row already shown, and not
     * an id on its own: id stopped being the order when the History started
     * following Last Use, so paging by it alone would skip and repeat rows.
     * Nothing on this phone pages — the screen shows the whole page — but the
     * boundary carries the shape the core has.
     *
     * [limit] is larger than the hundred `entries_cache` keeps, and has to be:
     * that hundred bounds the region the Relay has ordered, and the un-flushed
     * region is unbounded on purpose — an act this phone has not delivered is
     * undelivered clipboard content, and evicting one to protect a number is the
     * trade ADR 0014 refuses. A page of a hundred would hide exactly the oldest
     * offline Offers, which are the ones about to flush first. The core clamps to
     * its own ceiling either way. The Filter narrows this one page and never asks
     * the Relay, so what this call returns is the whole of what it can ever find.
     */
    suspend fun listHistory(
        userId: String,
        before: HistoryCursor? = null,
        limit: Long = 1_000,
    ): List<Entry> = io { it.listHistory(userId, before, limit) }

    suspend fun readEntry(userId: String, entryId: Long): String? = io { it.readEntry(userId, entryId) }

    suspend fun recall(userId: String, entryId: Long) = io { it.recall(userId, entryId) }

    suspend fun recallLatest(userId: String): Recalled = io { it.recallLatest(userId) }

    suspend fun offer(userId: String, text: String): OfferOutcome = io { it.offer(userId, text) }

    /**
     * Put a refused act back in the queue.
     *
     * Local work only: the core clears the refusal and puts the act at the back, and
     * the Relay hears about it on the next flush. Not a **Use** — the Relay
     * never took the act, so there is no record of one to move.
     */
    suspend fun resend(userId: String, entryId: Long) = io { it.resend(userId, entryId) }

    suspend fun deleteEntry(userId: String, entryId: Long) = io { it.deleteEntry(userId, entryId) }

    suspend fun clearHistory(userId: String) = io { it.clearHistory(userId) }

    suspend fun settings(): Settings = io { it.getSettings() }

    suspend fun updateSettings(patch: SettingsPatch): Settings = io { it.updateSettings(patch) }

    suspend fun writeClipboard(text: String) = io { it.writeClipboard(text) }

    // -- the Standing Actions, which assume no screen is open -----------------

    /**
     * Offered Capture of whatever is on this device's clipboard.
     *
     * The whole operation behind one call on the Active Pairing, because that is
     * the granularity every caller wants and the only granularity some of them
     * can express: ticket 12's Standing Action runs in a transparent activity
     * that renders nothing and holds no state, so anything it had to look up
     * first would be a second copy of this method.
     *
     * A clipboard with nothing text-like on it is offered as the **empty
     * string**, which the core's one capture filter answers with
     * `SkipReason.NonText`. Deciding "there is no text here" a second time up
     * here would be a second filter to keep in step with the first, and the
     * first is the one with the tests.
     */
    suspend fun offerClipboard(): OfferAttempt = io { it.offerOnTheActivePairing(clipboard.readText() ?: "") }

    /**
     * Offered Capture of text that arrived from somewhere other than the
     * clipboard.
     *
     * The share target's entry point: an `ACTION_SEND` carries its content
     * inside the `Intent`, so there is nothing to read and no window focus to
     * wait for — but the Active Pairing still has to be resolved, and the caller
     * still has nowhere to put an exception. Same shape as [offerClipboard] for
     * the same reason, and the two share their body so they cannot drift.
     *
     * What counts as offerable text is **not** decided here. Whether the sending
     * app marked its content sensitive is the share target's question, because
     * only it can see the `Intent`; everything after that is the core's one
     * capture filter, which is where `NonText` and `TooLarge` are decided — and
     * where text this device already holds becomes a Use of the Entry holding
     * it rather than an Offer at all.
     */
    suspend fun offerText(text: String): OfferAttempt = io { it.offerOnTheActivePairing(text) }

    /**
     * Resolve the Active Pairing and offer [text] to it.
     *
     * Takes the core rather than being a `suspend` method of its own, so both
     * callers do their work inside one [io] block: the Pairing lookup and the
     * Offer are then a single trip across the boundary under a single read of
     * the facade's lifetime lock.
     */
    private fun Sharepaste.offerOnTheActivePairing(text: String): OfferAttempt {
        val userId = theActivePairing() ?: return OfferAttempt.Unpaired
        return OfferAttempt.Settled(userId, offer(userId, text))
    }

    /**
     * The Active Pairing, in a process that may never have resumed one.
     *
     * **`activePairing()` is an in-memory read, and a cold process has nothing
     * in memory.** The core latches the Active Pairing when
     * `resumeActivePairing()` loads it from storage, which is what
     * `MainActivity.onStart` does — so on a screen there is always one. A
     * Standing Action has no `onStart`: its process was created by an `Intent`,
     * it opens the facade and asks immediately, and `activePairing()` answers
     * `null` for a phone that is perfectly well paired.
     *
     * Found on the emulator, and it is the whole difference between "Offer
     * without opening the app" working and reporting *"This phone is not paired
     * yet"* on a phone with a Pairing, an Active one and a History. It belongs
     * here rather than in the two activities: every screenless caller has the
     * problem, and one that forgot would fail in the way hardest to attribute.
     *
     * A failed resume is not an error to raise. It means there is no Pairing to
     * resume, or the keychain would not open — and `Unpaired` is the honest
     * answer to both from a surface with nowhere to put an exception.
     */
    private fun Sharepaste.theActivePairing(): String? =
        activePairing() ?: runCatching { resumeActivePairing() }.getOrNull()

    /**
     * Bring a session up, wait for the pending queue to drain, and put it back
     * down.
     *
     * **What this is for.** `offer` enqueues and nudges the uploader, and the
     * uploader lives on a session. A screen open in front of somebody always has
     * one; a Standing Action does not — the process was started by an `Intent`
     * and holds nothing but one invisible window. Without this, "Offer without
     * opening the app" would produce an Entry that sits in a queue until the app
     * *is* opened, which is not the feature: a person offers something on their
     * phone in order to use it on their laptop a moment later.
     *
     * **Why this is not the background work ADR 0007 forbids.** The rule is that
     * nothing runs while nobody is looking. This runs because somebody pressed a
     * control; [timeoutMs] bounds the wait; and the session is brought back down
     * before the caller's window closes **even if the caller is cancelled** —
     * which is the one part of that sentence a `finally` alone does not buy. The
     * teardown carries a bound of its own, [TEARDOWN_MS], rather than living
     * inside [timeoutMs]'s: it deliberately runs where cancellation cannot reach
     * it, so it needs a limit that is not the caller's to revoke. The worst case
     * one press can cost is therefore the two added together.
     *
     * **A session an open screen is already holding is left alone**, neither
     * started nor stopped. Stopping it is what this used to do, on an argument
     * that measured false: the claim was that a Standing Action's window taking
     * focus is what puts any open screen through `onStop`, and `onStop` is what
     * stops sessions. `Theme.Sharepaste.Invisible` is translucent, and an
     * activity left drawn is not stopped. Measured on `spike35` — an `am start`
     * of `StandingActionActivity` over an open `MainActivity` logs
     * `wm_on_paused_called` for it and never `wm_on_stop_called`, and the way
     * back is `wm_on_resume_called` with no `wm_on_start_called` either. So
     * `onLeaveForeground` never ran, the visible History screen still held its
     * session, this stopped it, and no `onStart` was coming to put it back: the
     * screen went silently deaf to new Entries. See [drainPending], where the
     * ordering lives and where JVM tests pin all three parts.
     *
     * Answers whether the queue emptied. Not emptying is the ordinary offline
     * outcome and not an error — the Entry is kept, the pending count is on the
     * History screen, and the next foreground sends it.
     */
    suspend fun sendPending(userId: String, timeoutMs: Long = SEND_TIMEOUT_MS): Boolean =
        drainPending(userId, timeoutMs)

    /**
     * Recall Latest, on the Active Pairing.
     *
     * It **always** fetches — see the facade, which never short-circuits to the
     * cache — and the fetch failing is not a failure of the operation: the
     * newest cached Entry is still the best answer available. Which one it was
     * comes back in [RecallAttempt.Done.fromCache], and saying so is the
     * caller's obligation, not an option.
     */
    suspend fun recallLatestOnActivePairing(): RecallAttempt = io { core ->
        val userId = core.theActivePairing() ?: return@io RecallAttempt.Unpaired
        val recalled = core.recallLatest(userId)
        RecallAttempt.Done(
            userId = userId,
            entryId = recalled.entryId,
            createdAt = recalled.createdAt,
            fromCache = recalled.source == RecallSource.CACHE,
        )
    }

    /**
     * The Preview of one Entry, for a Recall that has to say what it handed over.
     *
     * [RecallAttempt.Done] carries no plaintext and must not start: the core has
     * already put the text on the clipboard, and a secret nothing needs is a
     * secret something logs. A Preview is a different thing — the facade's own
     * one-line rendering, already normalised and capped, and the same string a
     * History row shows — so a Recall Receipt reads it back by id instead.
     *
     * **Answers `null` rather than throwing, and that is the contract.** The
     * Entry is on the clipboard by the time anyone asks; a failed read is a
     * Receipt with less to say, never a Recall reported as a failure. Both
     * callers used to make that judgement separately, which is one judgement
     * more than there is.
     */
    suspend fun previewOf(userId: String, entryId: Long): String? = try {
        listHistory(userId).firstOrNull { it.id == entryId }?.preview
    } catch (e: AppException) {
        null
    }

    /**
     * Release the facade and its runtime.
     *
     * The shipped app never calls this: the process dying *is* the teardown, and
     * a repository that could be closed while a screen still held it would be a
     * crash waiting for a race. It exists so an instrumented test can open a
     * facade of its own and not leave a tokio runtime and a SQLite handle behind
     * for the next test in the same process.
     *
     * **Closing waits for the calls already inside the boundary, and refuses the
     * ones that come after.** Without that ordering, a `close` landing between a
     * caller entering [io] and the FFI call happening throws
     * `IllegalStateException: Sharepaste object has already been destroyed` out
     * of whatever coroutine was unlucky — and that is not the caller's mistake to
     * catch, because the state holder answers core events by calling back in and
     * `forgetPairing` raises exactly those events on its way out.
     * [CancellationException] is what a call made after the close gets: it means
     * "this work no longer has a point", which is precisely true, and coroutines
     * already know to treat it as an ending rather than a failure.
     */
    suspend fun close() {
        val core = withContext(Dispatchers.IO) { runCatching { opening.await() }.getOrNull() }
        withContext(Dispatchers.IO) {
            // Nothing suspends inside the locked region: a read/write lock is
            // owned by a thread, and a coroutine that resumed elsewhere would try
            // to release a lock it does not hold.
            lifetime.write {
                if (!closed) {
                    closed = true
                    core?.let { runCatching { it.close() } }
                }
            }
        }
        scope.cancel()
    }

    private suspend inline fun <T> io(crossinline call: (Sharepaste) -> T): T {
        val core = opening.await()
        return withContext(Dispatchers.IO) {
            lifetime.read {
                if (closed) throw CancellationException("the facade has been closed")
                call(core)
            }
        }
    }

    companion object {
        /** The database file, inside app-private storage. */
        const val DATABASE_NAME = "sharepaste.db"

        /**
         * How long [sendPending] holds a session open waiting for the upload.
         *
         * Long enough for a session to come up and one small POST to complete
         * over a working connection; short enough that a phone with no route
         * does not hold a Standing Action's invisible window while somebody
         * waits for nothing. Exceeding it is the ordinary offline outcome.
         */
        const val SEND_TIMEOUT_MS = 10_000L

        /**
         * Open the core over this application's private storage.
         *
         * The path is handed *in*: the core never asks the OS where data lives,
         * because on Android the framework is what decides. `filesDir` is
         * app-private, which is what puts the cache behind file-based encryption
         * without a plaintext-at-rest toggle of our own.
         *
         * [requireHttps] is the transport policy, and it is a parameter rather
         * than a constant because the answer belongs to whoever is shipping. The
         * app passes `BuildConfig.REQUIRE_HTTPS`, which is `true`; an
         * instrumented test reaching the cleartext test relay passes `false` and
         * says so at the call. Android's network security configuration does not
         * constrain the core's Rust HTTP client — ticket 08 proved that on the
         * emulator — so this flag is the real enforcement.
         *
         * [databaseName] is a parameter for the same reason: a test that needs
         * its own database gets one without a test-only constructor sitting in
         * production code.
         */
        fun open(
            context: Context,
            requireHttps: Boolean,
            databaseName: String = DATABASE_NAME,
        ): SharepasteRepository {
            val app = context.applicationContext
            val sink = FlowEventSink()
            // One clipboard, handed both to the core and to `offerClipboard`:
            // a Recall writes through the same object an Offer reads through, so
            // there is one set of rules about what this platform calls text.
            val clipboard = AndroidClipboard(app)
            // A supervisor so a failed open does not cancel anything else that
            // happens to share a scope; `await` rethrows the `AppException` at
            // the first caller instead, which is where it can be reported.
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
            val opening = scope.async {
                Sharepaste.open(
                    dbPath = File(app.filesDir, databaseName).absolutePath,
                    keychain = AndroidKeychain(app),
                    clipboard = clipboard,
                    events = sink,
                    requireHttps = requireHttps,
                )
            }
            return SharepasteRepository(scope, opening, clipboard, sink.events)
        }
    }
}

/**
 * The queue of **Pending** Entries, the session that empties it, and the events
 * it reports through: the whole of what [drainPending] needs of the core.
 *
 * Every member is one [SharepasteRepository] already had, so this adds no
 * behaviour — it exists to be **faked**. What [drainPending] has to get right is
 * three orderings no device can be relied on to exercise on demand: the session
 * comes down when the coroutine draining it is cancelled, it comes down even
 * when the teardown itself never returns, and a session this drain did not start
 * is never touched. A fake makes those JVM tests in `PendingDrainTest`, which
 * runs in CI, rather than instrumented ones, which do not.
 */
internal interface PendingQueue {

    /** Everything the core raises. See [SharepasteRepository.events]. */
    val events: SharedFlow<CoreEvent>

    suspend fun startSession(userId: String)

    suspend fun stopSession(userId: String)

    /**
     * Whether this process is already holding a session for [userId].
     *
     * Not `suspend`, and that is the whole point of it: this is a read of the
     * shell's record of what it has itself asked the core for, so it crosses no
     * boundary and cannot fail. `SharepasteRepository.heldSessions` is where it
     * lives and where the reason the core is not the one asked is written down.
     */
    fun holdsSession(userId: String): Boolean

    /**
     * How many Entries are waiting to upload on [userId], or `null` when this
     * device holds no such Pairing.
     *
     * Asked **once** per drain, and only to close a gap the events cannot. See
     * [drainPending].
     */
    suspend fun pendingOn(userId: String): Long?
}

/**
 * How long the teardown gets, once it is somewhere cancellation cannot reach.
 *
 * A bound of its own rather than the drain's, because it runs inside
 * [NonCancellable]: nothing outside can end it, so it has to be able to end
 * itself. Stopping a session is local work — cancel two tasks, drop an upload
 * trigger — with no network in it, so the only thing that can make it slow is a
 * facade still opening behind it, and two seconds is more than that needs.
 * Exceeding it releases the window with the session possibly still up, which is
 * the lesser of the two failures: a window nothing can close is a window nobody
 * can report.
 */
internal const val TEARDOWN_MS = 2_000L

/**
 * Bring [userId]'s session up unless something already holds one, wait for its
 * queue to drain, and put down whatever this call brought up — whatever happens
 * to the caller in between.
 *
 * **The session comes down even if this coroutine is cancelled.** `stopSession`
 * suspends, and a suspending call in a `finally` reached *by cancellation*
 * throws `CancellationException` at its first suspension point — before the FFI,
 * so the session is never touched, and a `runCatching` around it swallows the
 * evidence. `StandingActionActivity.onDestroy` cancels its scope, so an activity
 * destroyed inside the drain used to leave a session running with no window on
 * screen: unattended sync, which ADR 0007 forbids, arrived at silently.
 * [NonCancellable] is what makes the teardown happen anyway.
 *
 * **And the teardown is bounded, because [NonCancellable] took away the
 * caller's last lever.** `stopSession` awaits the facade before it calls
 * anything, so a facade that never finishes opening — the same stall that ends
 * the drain body by timeout — left this `finally` suspended forever in a scope
 * nothing could cancel: work outliving the press that authorised it, which is
 * the thing [NonCancellable] was added to prevent. [TEARDOWN_MS] is the bound,
 * and the shape is the narrow one — non-cancellable, so a destroyed window still
 * tears the session down; bounded, so it cannot hold the process instead. What a
 * bound cannot do is cut short a blocking FFI call already in progress; nothing
 * on this platform can, and it is the suspending half that hangs.
 *
 * **Nothing is started or stopped for a Pairing this process already holds a
 * session for**, and that is not an optimisation. A Standing Action's window is
 * translucent, so an open screen behind it is paused and *not* stopped:
 * `onLeaveForeground` never runs and that screen still has its session. Both
 * halves would be wrong on it — `startSession` cancels whichever session it
 * replaces, and the teardown would then stop the replacement, leaving a visible
 * History screen receiving no Entries with no `onStart` coming to restore it.
 * Measured on `spike35`; the logcat is in [SharepasteRepository.sendPending].
 *
 * **It awaits an event rather than polling for a number.** The core already
 * emits [CoreEvent.PendingCount] whenever the queue moves — the state holder
 * consumes it and the History screen shows it. Asking `listPairings()` every
 * 200ms for the same number rebuilt *every* Pairing up to fifty times per Offer,
 * to hold a second opinion about queue depth in a shell ADR 0006 wants thin.
 *
 * **The one thing an event cannot say is that nothing is coming.** A session an
 * open screen still holds can empty the queue before this collector exists, and
 * the count that would have ended the wait was then emitted with nobody
 * listening. So the subscription is established first and the session is brought
 * up inside [onSubscription], after which one read of [PendingQueue.pendingOn]
 * closes the gap: anything emptied before that read is in the read, and anything
 * emptied after it arrives as an event. One call across the boundary, and no
 * window on it.
 *
 * **It raises nothing.** Both crossings inside the subscription can throw
 * [AppException] — `startSession` where there is no route to the Relay, and
 * `pendingOn` through the same facade await — and every caller is a
 * `try`/`finally` under a bare `MainScope` with no handler, reached *after* the
 * person has been shown "Offered.". A queue that did not empty is a `false`.
 *
 * Answers whether the queue emptied. [timeoutMs] bounds the drain — the session
 * start and the wait — and not the teardown, which has [TEARDOWN_MS] instead, so
 * one press costs at most the two together. Not emptying in time is the ordinary
 * offline outcome and not an error.
 */
internal suspend fun PendingQueue.drainPending(userId: String, timeoutMs: Long): Boolean {
    // Ours to bring up and ours to put down, or somebody else's to leave alone.
    // Read before anything is started, because starting is what would make it
    // ours.
    val ours = !holdsSession(userId)
    return try {
        withTimeoutOrNull(timeoutMs) {
            events
                .onSubscription {
                    if (ours) {
                        try {
                            startSession(userId)
                        } catch (e: AppException) {
                            // No route to the Relay. Queued is the right answer
                            // and the History already says so; there is nothing
                            // to bring down, and the teardown below is tolerant
                            // of that.
                        }
                    }
                    val depth = try {
                        pendingOn(userId)
                    } catch (e: AppException) {
                        // The gap-closing read is the one thing here that can be
                        // lost cheaply: a session may well be up and uploading,
                        // so wait for the event and let the bound above end it.
                        // `null` is also what an unknown Pairing answers, and the
                        // drain does the same thing with both.
                        null
                    }
                    // Already empty, so no further count is coming. Emitted into
                    // this collector rather than returned around it, so the drain
                    // has one ending instead of two.
                    if (depth == 0L) emit(CoreEvent.PendingCount(userId, 0L))
                }
                .first { it is CoreEvent.PendingCount && it.userId == userId && it.count == 0L }
        } != null
    } finally {
        if (ours) {
            // Reached by a return, by the timeout, or by the caller being
            // cancelled. The third is why this is [NonCancellable], and being
            // [NonCancellable] is why it is bounded.
            withContext(NonCancellable) {
                withTimeoutOrNull(TEARDOWN_MS) { runCatching { stopSession(userId) } }
            }
        }
    }
}
