package com.sharepaste.android

import com.sharepaste.android.ui.AppActions
import com.sharepaste.android.ui.Confirmation
import com.sharepaste.core.ConnectionState
import com.sharepaste.core.Entry
import com.sharepaste.core.PairingSummary

/**
 * An [AppActions] whose members do nothing.
 *
 * For a screen rendered to be *read* rather than pressed — which is most of what
 * there is to assert about wording. Every member is overridable by name, so a
 * test that does press something says which one at its call site instead of
 * burying it in seventeen lambdas.
 *
 * A test that drives real behaviour uses `appActions(model)` instead: that is the
 * activity's own wiring, and a hand-assembled bag here could pass while the app
 * was wired to something else.
 */
fun noActions(
    offerClipboard: () -> Unit = {},
    recallLatest: () -> Unit = {},
    recall: (Entry) -> Unit = {},
    deleteEntry: (Entry) -> Unit = {},
    dismissNotice: () -> Unit = {},
    openPairings: () -> Unit = {},
    openHistory: () -> Unit = {},
    openAddPairing: () -> Unit = {},
    viewPairing: (String) -> Unit = {},
    activatePairing: (String) -> Unit = {},
    confirm: (Confirmation?) -> Unit = {},
    clearHistory: (String) -> Unit = {},
    forgetPairing: (String) -> Unit = {},
    setShowRecalled: (Boolean) -> Unit = {},
    dismissForegroundNote: () -> Unit = {},
    enableStandingActions: () -> Unit = {},
) = AppActions(
    setDeviceLabel = {},
    setPairingCode = {},
    codeScanned = {},
    pairWithCode = {},
    setCameraProblem = {},
    dismissPairFailure = {},
    offerClipboard = offerClipboard,
    recallLatest = recallLatest,
    recall = recall,
    deleteEntry = deleteEntry,
    dismissNotice = dismissNotice,
    openPairings = openPairings,
    openHistory = openHistory,
    openAddPairing = openAddPairing,
    viewPairing = viewPairing,
    activatePairing = activatePairing,
    confirm = confirm,
    clearHistory = clearHistory,
    forgetPairing = forgetPairing,
    setShowRecalled = setShowRecalled,
    dismissForegroundNote = dismissForegroundNote,
    enableStandingActions = enableStandingActions,
)

/**
 * An [Entry] as the facade hands one over.
 *
 * `preview` defaults to something already normalised, because the facade's
 * Preview always is — a test that wants to prove the screen does not blank an
 * indented Entry passes the raw text as `plaintext` and the *normalised* string
 * the facade would build from it as `preview`, or it is testing a normalisation
 * that does not live here.
 *
 * `originLabel` defaults to whatever the core would resolve from the other two
 * fields, so a caller that overrides one of them and not this gets an Entry the
 * facade could actually have produced.
 */
fun entry(
    id: Long,
    preview: String = "an Entry",
    plaintext: String? = preview,
    deviceId: String = "other-device",
    deviceLabel: String? = "the laptop",
    originLabel: String = deviceLabel?.trim()?.ifEmpty { null } ?: deviceId.take(4),
    undecryptable: Boolean = false,
    userId: String = "u",
    createdAt: Long = 1_700_000_000_000,
    // An Entry never used since capture, which is the ordinary case and the one
    // a test that says nothing about use means.
    lastUse: Long = createdAt,
) = Entry(
    id = id,
    userId = userId,
    preview = preview,
    plaintext = plaintext,
    createdAt = createdAt,
    lastUse = lastUse,
    deviceId = deviceId,
    deviceLabel = deviceLabel,
    originLabel = originLabel,
    undecryptable = undecryptable,
)

/**
 * A [PairingSummary] as `listPairings` hands one over.
 *
 * `status` defaults to `DISCONNECTED` and `isActive` to `false`, which is the
 * combination that matters most: a Pairing that is merely not the Active one and
 * not connected is *resting*, and the whole of this ticket's tone rule is about
 * it not reading as a fault.
 *
 * `relayHost` defaults to what the core would resolve from `serverUrl`, so a
 * caller that names a Relay and says nothing about its host gets a Pairing the
 * facade could actually have produced. Only the ordinary shape is derived here
 * — a test about a credential or a scheme-less address belongs against
 * `render::relay_host` in the core, where the rule lives.
 */
fun pairing(
    userId: String,
    username: String? = null,
    label: String = "this phone",
    deviceId: String = "device-$userId",
    serverUrl: String = "http://10.0.2.2:8443",
    relayHost: String = serverUrl.substringAfter("://").substringBefore('/'),
    status: ConnectionState = ConnectionState.DISCONNECTED,
    pending: Long = 0,
    isActive: Boolean = false,
) = PairingSummary(
    userId = userId,
    deviceId = deviceId,
    label = label,
    username = username,
    serverUrl = serverUrl,
    relayHost = relayHost,
    status = status,
    pending = pending,
    isActive = isActive,
)
