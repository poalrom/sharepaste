package com.sharepaste.android.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.CreationExtras
import com.sharepaste.android.OfferAttempt
import com.sharepaste.android.R
import com.sharepaste.android.RecallAttempt
import com.sharepaste.android.SharepasteApplication
import com.sharepaste.android.SharepasteRepository
import com.sharepaste.android.scan.CameraProblem
import com.sharepaste.core.AppException
import com.sharepaste.core.ConnectionState
import com.sharepaste.core.CoreEvent
import com.sharepaste.core.Entry
import com.sharepaste.core.OfferOutcome
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * The one state holder, and the whole of the sync model.
 *
 * Two responsibilities, and they are the same responsibility seen from two
 * sides: it turns what the person does into calls on [SharepasteRepository], and
 * it turns what the core raises into [UiState]. Nothing else in the app talks to
 * the repository, and no composable talks to the core.
 *
 * **Sync is foreground only** (ADR 0007). [onEnterForeground] resumes the Active
 * Pairing and brings its session up; [onLeaveForeground] takes every session
 * down. There is no WorkManager, no JobScheduler, no foreground service and no
 * push — not as an omission, but because a clipboard tool that runs unattended
 * is a clipboard tool that reads your clipboard unattended. The honest
 * consequence is on screen, in `foreground_only_note`, rather than buried here.
 *
 * A [ViewModel] rather than something the activity owns, so a rotation does not
 * tear a live session down and stand a new one up.
 */
class SharepasteViewModel(private val repo: SharepasteRepository) : ViewModel() {

    private val _state = MutableStateFlow(UiState())
    val state: StateFlow<UiState> = _state.asStateFlow()


    init {
        viewModelScope.launch {
            repo.events.collect(::onCoreEvent)
        }
    }

    // -- the two lifecycle edges, which are the entire sync model -------------

    /**
     * `Activity.onStart`: pick the Active Pairing back up and bring it online.
     *
     * A phone with no Pairing lands on the pairing flow, which is the only thing
     * it can usefully do. A failure to resume is *not* an error screen — being
     * out of contact is the nominal case, and a phone that cannot reach the
     * Relay right now is a phone that will try again next time it is opened.
     */
    fun onEnterForeground() {
        // Recorded before the coroutine, not inside it, for the same reason
        // `onLeaveForeground` does: the flag is what tells a disconnected session
        // apart from a resting one, and a window where the app is in front but
        // does not think so would read as "resting" on screen.
        _state.update { it.copy(foreground = true, session = SessionPhase.Looking) }
        viewModelScope.launch {
            val userId = try {
                repo.resumeActivePairing()
            } catch (e: AppException) {
                null
            }
            if (userId == null) {
                _state.update {
                    it.copy(
                        screen = Screen.Pairing,
                        session = SessionPhase.Unpaired,
                        activeUserId = null,
                    )
                }
                return@launch
            }
            _state.update { it.copy(screen = Screen.History, activeUserId = userId) }
            // Before the session, because it needs no network and everything it
            // reads is what the list is rendered *with*: this device's own Device
            // id, without which every row claims an Origin; the depth of the
            // pending queue, which is what an Offer made offline last time is
            // waiting behind; and the Pairings themselves, without which a
            // divergence band has no names to put in its sentence.
            refreshPairings()
            // The cached Entries too, and **before** the session rather than
            // after it. Reading afterwards is a race with the session's own
            // stream: `startSession` returns as soon as its tasks are on the
            // core's runtime, so an `EntryAdded` can arrive between this read
            // hitting the database and the state being written — and the write
            // then replaces a list that already had the new Entry in it with one
            // that never will. Nothing else arrives afterwards to correct it, so
            // the Entry is simply missing until the next foreground.
            //
            // Reading first also means a phone with no route to the Relay shows
            // its History instead of an empty screen: `startSession` throwing
            // used to skip this call.
            refreshHistory(userId)
            try {
                // Opens the SSE session and backfills every Entry that arrived
                // while this phone was closed. Returns as soon as the tasks are
                // on the core's runtime; the Relay is reported through events.
                //
                // This is also the only thing that flushes the pending queue: the
                // uploader lives on the session, so an Offer made with no
                // connection goes out on the next foreground and never in the
                // background (ADR 0007).
                repo.startSession(userId)
            } catch (e: AppException) {
                _state.update { it.copy(session = sessionPhaseFor(userId, e)) }
            }
        }
    }

    /**
     * `Activity.onStop`: take every session down.
     *
     * Launched rather than blocking, because `onStop` runs on the main thread and
     * every call into the core blocks. Android's grace period before it freezes
     * a backgrounded process is ample for a teardown that only cancels tokens;
     * and if the process is killed outright, a session that dies with it is the
     * same outcome by a shorter route.
     */
    fun onLeaveForeground() {
        // The Viewed Pairing goes with it. It is a transient view choice —
        // CONTEXT.md: "forgotten when the window closes" — and a phone's
        // equivalent of closing the window is being put down. Nothing persists
        // it, so forgetting is simply dropping the override.
        _state.update {
            it.copy(
                foreground = false,
                viewedUserId = null,
                confirming = null,
                // The rows belonged to the Pairing being stopped looking at, so
                // they would otherwise be read as the Active Pairing's on the way
                // back in, until the next refresh replaced them.
                entries = if (it.diverged) emptyList() else it.entries,
            )
        }
        viewModelScope.launch {
            repo.stopAllSessions()
            // [SessionPhase.Looking] carries no user id, because `onEnterForeground`
            // enters it before it knows which Pairing it is resuming. A phone put
            // down while it was still looking — reconnecting, or out of contact and
            // trying again — would otherwise stay on screen claiming to be checking
            // for new Entries with nothing left running to check. Resting needs a
            // Pairing to be resting *about*, and the Active Pairing is it.
            _state.update { current ->
                val resting = current.session.userIdOrNull() ?: current.activeUserId
                current.copy(
                    session = if (resting == null) current.session else SessionPhase.Resting(resting),
                )
            }
        }
    }

    // -- pairing --------------------------------------------------------------

    fun setDeviceLabel(label: String) {
        _state.update { it.copy(pairing = it.pairing.copy(deviceLabel = label)) }
    }

    fun setCameraProblem(problem: CameraProblem?) {
        _state.update { it.copy(pairing = it.pairing.copy(camera = problem)) }
    }

    /**
     * The code field, as somebody types in it.
     *
     * Emptying it clears [PairingState.scanned], which is what puts the
     * viewfinder back on screen. A field left holding a scanned code keeps the
     * camera stood down, including while it is being edited — resurrecting a
     * preview under a cursor because a character was deleted would be worse than
     * either state.
     */
    fun setPairingCode(code: String) {
        _state.update {
            it.copy(
                pairing = it.pairing.copy(
                    code = code,
                    scanned = it.pairing.scanned && code.isNotBlank(),
                ),
            )
        }
    }

    /**
     * A code the camera read. It fills the field and stands the viewfinder down.
     *
     * **It does not pair**, and that is the point rather than an omission. A scan
     * is the first thing a person does on this screen — the square is the only
     * part of it that looks like an instruction — and it arrives before the name
     * the Pairing has to carry. Pairing on it would spend a code with a two-minute
     * life on a message asking for the name.
     *
     * The analyser fires on every frame a code stays in view, so the first one
     * wins: [PairingState.scanned] is the gate that turns a stream of identical
     * decodes into one field. It also clears a failure, because the code that
     * failed is no longer the code in the field.
     */
    fun codeScanned(code: String) {
        _state.update {
            if (it.pairing.scanned) {
                it
            } else {
                it.copy(pairing = it.pairing.copy(code = code, scanned = true, attempt = PairAttempt.Idle))
            }
        }
    }

    fun dismissPairFailure() {
        _state.update { it.copy(pairing = it.pairing.copy(attempt = PairAttempt.Idle)) }
    }

    /**
     * Pair with the code in the field, however it got there.
     *
     * The code goes to the core exactly as it arrived: `decode` already strips
     * whitespace and dashes and upper-cases, so the desktop's compact QR payload
     * and a code someone typed in groups of four are both simply codes.
     *
     * Ignored while an attempt is already in flight. The two other refusals —
     * no code, no name — are what [PairingState.canPair] disables the button
     * over, and they are re-checked here because a state holder that trusts a
     * screen to have disabled something is a state holder with a hole in it.
     */
    fun pairWithCode() {
        val pairing = _state.value.pairing
        if (pairing.attempt is PairAttempt.Working) return
        if (pairing.code.isBlank()) return
        if (pairing.deviceLabel.isBlank()) {
            _state.update {
                it.copy(
                    pairing = it.pairing.copy(
                        attempt = PairAttempt.Failed(R.string.pair_needs_a_name),
                    ),
                )
            }
            return
        }
        _state.update { it.copy(pairing = it.pairing.copy(attempt = PairAttempt.Working)) }
        viewModelScope.launch {
            try {
                val paired = repo.pairWithCode(pairing.code, pairing.deviceLabel.trim())
                repo.setActivePairing(paired.userId)
                _state.update {
                    it.copy(
                        screen = Screen.History,
                        session = SessionPhase.Looking,
                        activeUserId = paired.userId,
                        // A Pairing just added is the one to look at, whatever was
                        // being looked at before.
                        viewedUserId = null,
                        // The flow is spent: nothing here should be able to offer
                        // this code a second time.
                        pairing = it.pairing.restarted(),
                    )
                }
                refreshPairings()
                // Before the session, for the reason `onEnterForeground` spells
                // out: a read that lands after it can overwrite an Entry the
                // stream has already delivered.
                refreshHistory(paired.userId)
                repo.startSession(paired.userId)
            } catch (e: AppException) {
                _state.update {
                    it.copy(pairing = it.pairing.copy(attempt = pairFailureFor(e)))
                }
            }
        }
    }

    // -- History, Offer and Recall --------------------------------------------

    /**
     * Offered Capture of whatever is on the clipboard.
     *
     * Every Entry a phone produces is an Offered Capture: the person hands the
     * content over, so the device never sees a clipboard it was not shown. It is
     * honoured whether or not capture is enabled — `capture_enabled` governs
     * Watched Capture, which a phone never performs, and refusing content
     * someone just chose to share would be indefensible.
     *
     * The whole operation is [SharepasteRepository.offerClipboard], which takes
     * no screen for granted. This method only turns its answer into a sentence.
     */
    fun offerClipboard() {
        viewModelScope.launch {
            val notice = try {
                when (val attempt = repo.offerClipboard()) {
                    OfferAttempt.Unpaired -> Notice.Unpaired
                    is OfferAttempt.Settled -> when (val outcome = attempt.outcome) {
                        is OfferOutcome.Queued -> Notice.Offered(outcome.pending)
                        is OfferOutcome.Rejected -> Notice.OfferRefused(outcome.reason)
                    }
                }
            } catch (e: AppException) {
                Notice.Failed(R.string.offer_failed, e.explain())
            }
            _state.update {
                // The Offer's own count is more current than any event: the
                // enqueue has already happened by the time it is returned, and
                // `PendingCount` is on its way through the sink behind it.
                it.copy(notice = notice, pending = (notice as? Notice.Offered)?.pending ?: it.pending)
            }
        }
    }

    /**
     * Recall Latest: the newest Entry onto this device's clipboard.
     *
     * It always fetches, and when the fetch fails the newest **cached** Entry is
     * handed over instead with [Notice.RecalledFromCache] to say so. That notice
     * is not decoration — a silent fallback hands over yesterday's link.
     */
    fun recallLatest() {
        viewModelScope.launch {
            val notice = try {
                when (val attempt = repo.recallLatestOnActivePairing()) {
                    RecallAttempt.Unpaired -> Notice.Unpaired
                    is RecallAttempt.Done ->
                        if (attempt.fromCache) Notice.RecalledFromCache else Notice.Recalled
                }
            } catch (e: AppException) {
                recallFailureFor(e)
            }
            _state.update { it.copy(notice = notice) }
        }
    }

    /**
     * Recall one chosen Entry.
     *
     * Takes the Entry's own `userId` rather than the Active Pairing's, because
     * ticket 11 introduces a Viewed Pairing that may not be the Active one and a
     * row must recall from the History it is a row of.
     */
    fun recall(entry: Entry) {
        viewModelScope.launch {
            val notice = try {
                repo.recall(entry.userId, entry.id)
                Notice.Recalled
            } catch (e: AppException) {
                recallFailureFor(e)
            }
            _state.update { it.copy(notice = notice) }
        }
    }

    /**
     * Delete one Entry, on the Relay and here.
     *
     * Offered for an Undecryptable Entry as much as for any other: ciphertext
     * this device holds no key for is exactly the thing a person most wants gone,
     * and it is the one row where deleting is all they can do with it.
     */
    fun deleteEntry(entry: Entry) {
        viewModelScope.launch {
            try {
                repo.deleteEntry(entry.userId, entry.id)
            } catch (e: AppException) {
                _state.update {
                    it.copy(notice = Notice.Failed(R.string.delete_failed, e.explain()))
                }
            }
        }
    }

    fun dismissNotice() {
        _state.update { it.copy(notice = null) }
    }

    // -- the Standing Actions -------------------------------------------------

    /**
     * Record whether the platform will show the Standing Actions notification.
     *
     * Reported *in* rather than asked for here, because the answer is a fact
     * about the process's `NotificationManager` and about a runtime grant, and
     * neither belongs behind a `ViewModel` that has no `Context`. `MainActivity`
     * asks on every `onStart` — the person may have changed their mind in
     * Settings while the app was away, and there is no callback for that.
     *
     * A denial changes exactly one thing: a sentence on the History screen. It
     * must not disable a control, empty a list, or turn into a fault — the two
     * verbs still work from the screen the person is already looking at, and the
     * only thing they have lost is reaching them without it.
     */
    fun onStandingActionsChecked(blocked: Boolean) {
        _state.update { it.copy(standingActionsBlocked = blocked) }
    }

    // -- Pairings: the Viewed one, the Active one, and the two erasures -------

    /** Show the Pairings, and read them back before they are rendered. */
    fun openPairings() {
        _state.update { it.copy(screen = Screen.Pairings, confirming = null) }
        viewModelScope.launch { refreshPairings() }
    }

    fun openHistory() {
        _state.update { it.copy(screen = Screen.History, confirming = null) }
    }

    /**
     * Show the pairing flow, from the top.
     *
     * The flow is restarted rather than resumed. Its state outlives the screen —
     * the code field is [PairingState]'s, so that a scan can fill it — and a
     * second visit that opened holding the code from an abandoned first one would
     * offer to send a code minted minutes ago for somebody else's slot.
     */
    fun openAddPairing() {
        _state.update {
            it.copy(screen = Screen.Pairing, confirming = null, pairing = it.pairing.restarted())
        }
    }

    /**
     * Look at another Pairing's History.
     *
     * **This changes nothing about syncing or capture.** No session is started or
     * stopped, no setting is written, nothing is persisted: the Viewed Pairing is
     * an override held in memory and dropped when the app is put down. Offer and
     * Recall Latest continue to act on the Active Pairing, which is what the
     * divergence band is on screen to admit.
     */
    fun viewPairing(userId: String) {
        _state.update { current ->
            current.copy(
                viewedUserId = userId,
                confirming = null,
                // Emptied rather than left: the rows on screen belong to the other
                // Pairing, and a list that changes ownership one repaint late is a
                // list that briefly attributes Entries to the wrong User.
                entries = emptyList(),
                ownDeviceId = current.pairings.firstOrNull { it.userId == userId }?.deviceId,
            )
        }
        viewModelScope.launch { refreshHistory(userId) }
    }

    /**
     * Sync this Pairing from now on.
     *
     * The persistent half of the pair of distinctions: the core writes the choice
     * to settings, so it survives a restart, and brings the new Pairing's session
     * up. The one it replaces is stopped **here** rather than left running —
     * `set_active_pairing` starts the new session but has no opinion about the
     * old one, and a phone quietly holding two live SSE streams is not what
     * "exactly one is active at a time" means (CONTEXT.md).
     */
    fun activatePairing(userId: String) {
        viewModelScope.launch {
            val previous = _state.value.activeUserId
            try {
                if (previous != null && previous != userId) repo.stopSession(previous)
                repo.setActivePairing(userId)
            } catch (e: AppException) {
                _state.update {
                    it.copy(notice = Notice.Failed(R.string.pairing_use_failed, e.explain()))
                }
                return@launch
            }
            _state.update { current ->
                current.copy(
                    activeUserId = userId,
                    // If the Pairing just made Active is the one being viewed, the
                    // two now agree and the override has nothing left to say.
                    viewedUserId = current.viewedUserId?.takeIf { it != userId },
                    confirming = null,
                )
            }
            refreshPairings()
            _state.value.viewedPairing?.let { refreshHistory(it) }
        }
    }

    /** Ask for a destructive action, or take the question back with `null`. */
    fun confirm(confirmation: Confirmation?) {
        _state.update { it.copy(confirming = confirmation) }
    }

    /**
     * Erase every Entry of one Pairing, on the Relay and on every device.
     *
     * Takes the Pairing explicitly rather than assuming the Active or the Viewed
     * one. A phone may hold several, the button is on a card, and the card is
     * what the person is looking at when they press it.
     */
    fun clearHistory(userId: String) {
        _state.update { it.copy(confirming = null) }
        viewModelScope.launch {
            val name = _state.value.nameOf(userId)
            try {
                repo.clearHistory(userId)
            } catch (e: AppException) {
                _state.update {
                    it.copy(notice = Notice.Failed(R.string.clear_history_failed, e.explain()))
                }
                return@launch
            }
            _state.update { current ->
                current.copy(
                    notice = Notice.HistoryCleared(name),
                    entries = if (userId == current.viewedPairing) emptyList() else current.entries,
                )
            }
            refreshPairings()
        }
    }

    /**
     * Forget a Pairing: its Entries, its key material and its token.
     *
     * The facade does all of it, including promoting another Pairing to Active
     * and bringing its session up. What is read back afterwards is the core's
     * answer rather than a guess made here — which Pairing was promoted is the
     * core's decision, and a shell that assumed one would be wrong the first time
     * the rule changed.
     */
    fun forgetPairing(userId: String) {
        _state.update { it.copy(confirming = null) }
        viewModelScope.launch {
            val name = _state.value.nameOf(userId)
            try {
                repo.forgetPairing(userId)
            } catch (e: AppException) {
                _state.update {
                    it.copy(notice = Notice.Failed(R.string.forget_failed, e.explain()))
                }
                return@launch
            }
            val active = try {
                repo.activePairing()
            } catch (e: AppException) {
                null
            }
            _state.update { current ->
                current.copy(
                    activeUserId = active,
                    viewedUserId = current.viewedUserId?.takeIf { it != userId },
                    pairings = current.pairings.filterNot { it.userId == userId },
                )
            }
            refreshPairings()
            val viewed = _state.value.viewedPairing
            _state.update { current ->
                current.copy(
                    notice = Notice.PairingForgotten(name, current.nameOf(active).ifEmpty { null }),
                    // Nothing left to be paired to: the pairing flow is the only
                    // screen a phone in that state can usefully be on.
                    screen = if (viewed == null) Screen.Pairing else current.screen,
                    session = if (viewed == null) SessionPhase.Unpaired else current.session,
                    entries = if (viewed == null) emptyList() else current.entries,
                    ownDeviceId = current.pairings.firstOrNull { it.userId == viewed }?.deviceId,
                )
            }
            if (viewed != null) refreshHistory(viewed)
        }
    }

    // -- the core's own events ------------------------------------------------

    private fun onCoreEvent(event: CoreEvent) {
        when (event) {
            is CoreEvent.ConnectionState -> _state.update { current ->
                current.copy(
                    session = phaseFor(event.userId, event.state, event.lastError),
                    // The card for that Pairing reads the same event. Only the
                    // Active Pairing has a session raising them, which is exactly
                    // why `pairingPhase` refuses to render a stale reading for any
                    // other card.
                    pairings = current.pairings.map {
                        if (it.userId == event.userId) it.copy(status = event.state) else it
                    },
                )
            }

            // Gated on the **Viewed** Pairing, not the Active one. A live session
            // belongs to the Active Pairing and goes on delivering while someone
            // is looking at another Pairing's History; ungated, its Entries would
            // appear in a list they are not part of and be attributed to the
            // wrong User.
            is CoreEvent.EntryAdded -> _state.update { current ->
                // Newest first, and de-duplicated by id: the backfill and the
                // live SSE stream can both deliver the same Entry across a
                // reconnect.
                if (event.userId != current.viewedPairing) {
                    current
                } else if (current.entries.any { it.id == event.entry.id }) {
                    current
                } else {
                    current.copy(entries = listOf(event.entry) + current.entries)
                }
            }

            is CoreEvent.EntryDeleted -> _state.update { current ->
                if (event.userId != current.viewedPairing) {
                    current
                } else {
                    current.copy(entries = current.entries.filterNot { it.id == event.entryId })
                }
            }

            is CoreEvent.HistoryChanged -> {
                if (event.userId == _state.value.viewedPairing) {
                    viewModelScope.launch { refreshHistory(event.userId) }
                }
            }

            // The queue's depth, from the uploader that just changed it. This is
            // how the count reaches zero on the foreground that flushes it, and how
            // it climbs while there is no route to the Relay.
            //
            // The screen-wide count is matched against the Active Pairing and not
            // against the phase's user id: the uploader drains the queue long
            // before the SSE reader reports Online, so a phase-based gate drops the
            // one count that matters while the phone is still `Looking`. The card
            // takes it for whichever Pairing it belongs to, because a queue on a
            // Pairing this device has switched away from is visible nowhere else.
            is CoreEvent.PendingCount -> _state.update { current ->
                current.copy(
                    pending = if (event.userId == current.activeUserId) {
                        event.count
                    } else {
                        current.pending
                    },
                    pairings = current.pairings.map {
                        if (it.userId == event.userId) it.copy(pending = event.count) else it
                    },
                )
            }

            // The core moves the Active Pairing itself when one is forgotten, so
            // this is not merely an echo of `setActivePairing`.
            is CoreEvent.ActivePairingChanged -> {
                _state.update { it.copy(activeUserId = event.userId) }
                viewModelScope.launch { refreshPairings() }
            }

            is CoreEvent.PairingRemoved -> _state.update { current ->
                current.copy(pairings = current.pairings.filterNot { it.userId == event.userId })
            }

            is CoreEvent.PairingAdded -> viewModelScope.launch { refreshPairings() }

            // `Contact` is ticket 11's: the History screen reads Contact through
            // `SessionPhase`, not through the stamp. The rest are the pairing
            // flow's. Named rather than caught by an `else` so that adding a
            // variant to the core is a compile error here instead of an event
            // silently dropped on the floor.
            is CoreEvent.Contact,
            is CoreEvent.PairShortcode,
            is CoreEvent.PairClaimed,
            CoreEvent.PairExpired,
            -> Unit
        }
    }

    /**
     * Every Pairing this phone holds, and the two facts the History is rendered
     * with.
     *
     * Read from `listPairings`, which needs no network, so a phone with no route
     * to the Relay still renders its rows' Origins correctly and still knows what
     * each Pairing is holding. A failure leaves the previous values: a missing
     * `ownDeviceId` shows an Origin that need not be shown, which is better than
     * hiding one that must be.
     *
     * `ownDeviceId` follows the **Viewed** Pairing, because Origin is "the device
     * an Entry was captured on, as distinct from the device viewing it" and the
     * rows on screen belong to whichever Pairing is being viewed.
     */
    private suspend fun refreshPairings() {
        val pairings = try {
            repo.listPairings()
        } catch (e: AppException) {
            return
        }
        _state.update { current ->
            current.copy(
                pairings = pairings,
                ownDeviceId = pairings.firstOrNull { it.userId == current.viewedPairing }?.deviceId
                    ?: current.ownDeviceId,
                pending = pairings.firstOrNull { it.userId == current.activeUserId }?.pending
                    ?: current.pending,
            )
        }
    }

    /**
     * The Viewed Pairing's cached Entries.
     *
     * The write is guarded on the Pairing still being the viewed one. A read
     * started for one Pairing can land after the person has switched to another
     * — `viewPairing` starts one, and so does every `HistoryChanged` — and
     * putting one Pairing's Entries into the list of another is the one mistake
     * on this screen that a person cannot spot.
     */
    private suspend fun refreshHistory(userId: String) {
        val entries = try {
            repo.listHistory(userId)
        } catch (e: AppException) {
            return
        }
        _state.update { if (userId == it.viewedPairing) it.copy(entries = entries) else it }
    }

    /**
     * The core's connection reading, in the phone's terms.
     *
     * The foreground flag decides more than the wording. `onStop` takes every
     * session down, but a `ConnectionState` frame raised a moment earlier can still
     * be travelling through the sink when it does — and rendering that frame would
     * put a phone that has just hung up on the Relay on screen claiming to be in
     * contact with it. Ticket 09's `SessionLifecycleTest` names the same hazard
     * arriving the other way, through `connectionState`, and the answer is the
     * same: with no session there is nothing to be in contact through, so the
     * honest reading is Resting.
     *
     * A revoked Pairing is the exception, and it is not a wording exception: no
     * amount of waiting or reconnecting fixes it, so it is news whether the app is
     * in front or not.
     */
    private fun phaseFor(
        userId: String,
        state: ConnectionState,
        lastError: String?,
    ): SessionPhase {
        if (state == ConnectionState.AUTH_FAILED) return SessionPhase.Refused(userId, lastError)
        if (!_state.value.foreground) return SessionPhase.Resting(userId)
        return when (state) {
            ConnectionState.ONLINE -> SessionPhase.InContact(userId)
            ConnectionState.CONNECTING -> SessionPhase.Looking
            ConnectionState.DISCONNECTED -> SessionPhase.OutOfContact(userId)
            // Answered above. Repeated rather than swept up by an `else`, so a
            // reading added to the core arrives here as a compile error.
            ConnectionState.AUTH_FAILED -> SessionPhase.Refused(userId, lastError)
        }
    }

    /** The phase a failed `startSession` leaves the phone in. */
    private fun sessionPhaseFor(userId: String, e: AppException): SessionPhase = when (e) {
        is AppException.Auth -> SessionPhase.Refused(userId, e.detail)
        // Everything else — no route to the Relay, a refused cleartext relay, a
        // keychain that would not open — leaves the phone out of contact, which
        // is where a phone spends most of its life anyway.
        else -> SessionPhase.OutOfContact(userId)
    }

    /**
     * Which of the pairing failures this was.
     *
     * Each one gets its own sentence. A single "pairing failed" would be true of
     * all of them and useful for none: an expired code needs a fresh code, a
     * cleartext relay needs a certificate, and a code that is not a code needs
     * retyping. `InsecureRelay` is the one that shows the core's own words,
     * because the core names the relay and the reason and no wording here could.
     */
    private fun pairFailureFor(e: AppException): PairAttempt.Failed = when (e) {
        is AppException.PairExpired -> PairAttempt.Failed(R.string.pair_code_expired)
        is AppException.InsecureRelay -> PairAttempt.Failed(R.string.pair_insecure_relay, e.detail)
        is AppException.BadInput -> PairAttempt.Failed(R.string.pair_not_a_code)
        is AppException.Auth -> PairAttempt.Failed(R.string.pair_refused)
        // The Relay answers a claim for a slot it has already expired with a 404,
        // so this is the same news as `PairExpired` and gets the same sentence.
        is AppException.NotFound -> PairAttempt.Failed(R.string.pair_code_expired)
        is AppException.Network -> PairAttempt.Failed(R.string.pair_unreachable)
        else -> PairAttempt.Failed(R.string.pair_failed)
    }

    /**
     * Which of the Recall failures this was.
     *
     * `NotFound` is the one worth telling apart, and it covers the two ways a
     * Recall has nothing to hand over: an Undecryptable Entry, whose cached
     * plaintext is NULL, and a History with nothing in it at all. Both mean "no
     * plaintext here", which is why the *list* is where an Undecryptable Entry is
     * marked and why its Recall control is refused before it is pressed — a
     * message after the fact is the second line of defence, not the first.
     */
    private fun recallFailureFor(e: AppException): Notice.Failed = when (e) {
        is AppException.NotFound -> Notice.Failed(R.string.recall_nothing_to_recall)
        is AppException.InsecureRelay -> Notice.Failed(R.string.recall_failed, e.detail)
        else -> Notice.Failed(R.string.recall_failed, e.explain())
    }

    companion object {
        /**
         * Builds the state holder over the process's one facade.
         *
         * The repository is a constructor parameter rather than reached for
         * inside this class, so a test can drive this exact code against a
         * facade of its own — which is how the two lifecycle edges get proven
         * against a real Relay without the shipped app's transport policy
         * having to bend for a test.
         */
        val Factory: ViewModelProvider.Factory = object : ViewModelProvider.Factory {
            override fun <T : ViewModel> create(modelClass: Class<T>, extras: CreationExtras): T {
                val app = extras[ViewModelProvider.AndroidViewModelFactory.APPLICATION_KEY]
                    as SharepasteApplication
                @Suppress("UNCHECKED_CAST")
                return SharepasteViewModel(app.repository) as T
            }
        }
    }
}

private fun SessionPhase.userIdOrNull(): String? = when (this) {
    is SessionPhase.InContact -> userId
    is SessionPhase.OutOfContact -> userId
    is SessionPhase.Resting -> userId
    is SessionPhase.Refused -> userId
    // Never the whole phone's phase, only a card's. Answered anyway rather than
    // swept up, so the `when` stays a compile-time check.
    is SessionPhase.NotActive -> userId
    SessionPhase.Unpaired, SessionPhase.Looking -> null
}

/**
 * The core's own words, whichever variant carried them.
 *
 * UniFFI gives each variant its own `detail` field rather than one on the sealed
 * parent — `AppException` already has `message`, and a variant field of that name
 * generates bindings that do not compile. So reading the detail generically costs
 * a `when`, and the `when` is worth having: it is exhaustive, so a variant added
 * to the core arrives here as a compile error rather than as a blank sentence.
 */
private fun AppException.explain(): String = when (this) {
    is AppException.Network -> detail
    is AppException.Auth -> detail
    is AppException.NotFound -> detail
    is AppException.BadInput -> detail
    is AppException.Storage -> detail
    is AppException.Crypto -> detail
    is AppException.PairExpired -> detail
    is AppException.Keychain -> detail
    is AppException.Update -> detail
    is AppException.InsecureRelay -> detail
}
