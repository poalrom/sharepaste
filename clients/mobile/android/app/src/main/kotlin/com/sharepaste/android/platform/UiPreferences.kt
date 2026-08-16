package com.sharepaste.android.platform

import android.content.Context
import androidx.annotation.VisibleForTesting
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.emptyPreferences
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import java.io.IOException

/**
 * What this phone has been told to do about its own chrome.
 *
 * Three booleans, and they have nothing in common with the core's key material —
 * which is why they are here and not in [AndroidKeychain]'s
 * `EncryptedSharedPreferences`. That store is guarded by a hardware-backed key
 * because what it holds would decrypt somebody's Entries; a switch position and
 * a dismissed note would survive being read aloud, and putting them behind the
 * same key would say otherwise.
 *
 * DataStore rather than plain `SharedPreferences` for the shape of the read.
 * All three are on screen — two are switches, one decides whether a band is
 * composed at all — so the state holder wants them as a [Flow] it can fold
 * into [com.sharepaste.android.ui.UiState] the same way it folds core events,
 * rather than as a blocking read it has to remember to repeat.
 */
class UiPreferences(context: Context) {

    private val store = context.applicationContext.uiPreferences

    /**
     * All three values, as one snapshot, from the moment there is one.
     *
     * A corrupt or unreadable file reads as the defaults rather than as a crash.
     * None of the three is load-bearing: losing them turns both confirmations back
     * on and brings the foreground note back, which is exactly the state a fresh
     * install is in and is the safe direction for all three.
     */
    val values: Flow<UiPreferenceValues> = store.data
        .catch { if (it is IOException) emit(emptyPreferences()) else throw it }
        .map {
            UiPreferenceValues(
                showRecalled = it[SHOW_RECALLED] ?: true,
                confirmOffers = it[CONFIRM_OFFERS] ?: true,
                foregroundNoteDismissed = it[FOREGROUND_NOTE_DISMISSED] ?: false,
            )
        }

    suspend fun setShowRecalled(show: Boolean) {
        store.edit { it[SHOW_RECALLED] = show }
    }

    suspend fun setConfirmOffers(confirm: Boolean) {
        store.edit { it[CONFIRM_OFFERS] = confirm }
    }

    suspend fun dismissForegroundNote() {
        store.edit { it[FOREGROUND_NOTE_DISMISSED] = true }
    }

    /**
     * The switch positions as they stand, for whoever is about to report a verb.
     *
     * Read by all three reporting paths, and the closed-phone two have no choice:
     * a Standing Action and a share have no state holder and no composition, so
     * neither can take these off [com.sharepaste.android.ui.UiState] the way the
     * screen does. The state holder *could* — it folds this very snapshot into
     * `UiState` for the switches to draw from — and asks here anyway, so that
     * whether Sharepaste speaks is decided from one place on an open phone and a
     * closed one alike. The alternative is a rule that reads a preference two
     * ways and can be made to disagree with itself.
     *
     * **One read, not one per switch.** Both positions come out of the same
     * snapshot and go into the same predicate
     * ([com.sharepaste.android.ui.silences]), so there is no arrangement of
     * point reads that can leave a person's two switches half-applied.
     *
     * Named for what it answers rather than when: [values] is the flow, this is
     * one of its values.
     */
    suspend fun snapshot(): UiPreferenceValues = values.first()

    /**
     * Put all three preferences back to what a fresh install has.
     *
     * The shipped app never calls this, and there is deliberately no un-dismiss
     * on the surface a screen can reach: closing the foreground note for good is
     * the whole of what `▴ CLOSE` promises, and a control that could quietly
     * undo it would make the promise a lie.
     *
     * An instrumented test is the exception, and it needs one because of how
     * DataStore works rather than because of anything here: `preferencesDataStore`
     * hands the whole process **one** store per file and refuses a second over
     * the same path, so a test cannot get a clean one by constructing its own,
     * and deleting the file underneath the live instance would leave its cache
     * ahead of the disk. Every test that writes any of them has to hand the
     * suite back its defaults, and this is the honest way to do it —
     * `SharepasteRepository.close` exists for the same kind of reason.
     */
    @VisibleForTesting
    suspend fun resetToDefaults() {
        store.edit { it.clear() }
    }

    private companion object {
        val SHOW_RECALLED = booleanPreferencesKey("show_recalled")
        val CONFIRM_OFFERS = booleanPreferencesKey("confirm_offers")
        val FOREGROUND_NOTE_DISMISSED = booleanPreferencesKey("foreground_note_dismissed")
    }
}

/** All three preferences as the app reads them: never absent, only defaulted. */
data class UiPreferenceValues(
    /** Whether a Recall says what it put on the clipboard. See ADR 0009. */
    val showRecalled: Boolean = true,
    /**
     * Whether a taken Offer says so. See ADR 0018.
     *
     * Its own switch rather than the Recall one widened, because the two are off
     * for different reasons: a Recall Receipt names an Entry the person did not
     * choose, and an Offer Receipt names nothing and is merely the app speaking
     * over whatever they were doing. Silences [com.sharepaste.android.ui.Receipt.Offered]
     * and nothing else — a recognised Offer saved nothing and still says so.
     */
    val confirmOffers: Boolean = true,
    /**
     * Whether the History Screen's foreground-only band has been closed for
     * good.
     *
     * Persisted, unlike the band's open/closed state, which is a
     * `rememberSaveable` and is exploration rather than acknowledgement. The
     * note itself does not disappear with it: it is on the Settings Screen at
     * full length, because it is the app's most important disclosure and a
     * dismissal must not be the last time it can be read.
     */
    val foregroundNoteDismissed: Boolean = false,
)

/**
 * The file. Named for what is in it, and separate from everything else this app
 * persists — the core owns its own SQLite, the keychain owns its own store.
 */
private val Context.uiPreferences by preferencesDataStore(name = "ui")
