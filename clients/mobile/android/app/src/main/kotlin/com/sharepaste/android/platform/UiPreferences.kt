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
 * Two booleans, and they have nothing in common with the core's key material —
 * which is why they are here and not in [AndroidKeychain]'s
 * `EncryptedSharedPreferences`. That store is guarded by a hardware-backed key
 * because what it holds would decrypt somebody's Entries; a switch position and
 * a dismissed note would survive being read aloud, and putting them behind the
 * same key would say otherwise.
 *
 * DataStore rather than plain `SharedPreferences` for the shape of the read.
 * Both preferences are on screen — one is a switch, one decides whether a band
 * is composed at all — so the state holder wants them as a [Flow] it can fold
 * into [com.sharepaste.android.ui.UiState] the same way it folds core events,
 * rather than as a blocking read it has to remember to repeat.
 */
class UiPreferences(context: Context) {

    private val store = context.applicationContext.uiPreferences

    /**
     * Both values, as one snapshot, from the moment there is one.
     *
     * A corrupt or unreadable file reads as the defaults rather than as a crash.
     * Neither preference is load-bearing: losing them turns the Receipt back on
     * and brings the foreground note back, which is exactly the state a fresh
     * install is in and is the safe direction for both.
     */
    val values: Flow<UiPreferenceValues> = store.data
        .catch { if (it is IOException) emit(emptyPreferences()) else throw it }
        .map {
            UiPreferenceValues(
                showRecalled = it[SHOW_RECALLED] ?: true,
                foregroundNoteDismissed = it[FOREGROUND_NOTE_DISMISSED] ?: false,
            )
        }

    suspend fun setShowRecalled(show: Boolean) {
        store.edit { it[SHOW_RECALLED] = show }
    }

    suspend fun dismissForegroundNote() {
        store.edit { it[FOREGROUND_NOTE_DISMISSED] = true }
    }

    /**
     * The one preference a closed phone has to read.
     *
     * A Standing Action has no state holder and no composition, so it cannot
     * take the value off [com.sharepaste.android.ui.UiState] the way the screen
     * does. It asks here instead, once, on the way to reporting.
     */
    suspend fun showRecalledNow(): Boolean = values.first().showRecalled

    /**
     * Put both preferences back to what a fresh install has.
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
     * ahead of the disk. Every test that writes either value has to hand the
     * suite back its defaults, and this is the honest way to do it —
     * `SharepasteRepository.close` exists for the same kind of reason.
     */
    @VisibleForTesting
    suspend fun resetToDefaults() {
        store.edit { it.clear() }
    }

    private companion object {
        val SHOW_RECALLED = booleanPreferencesKey("show_recalled")
        val FOREGROUND_NOTE_DISMISSED = booleanPreferencesKey("foreground_note_dismissed")
    }
}

/** Both preferences as the app reads them: never absent, only defaulted. */
data class UiPreferenceValues(
    /** Whether a Recall says what it put on the clipboard. See ADR 0009. */
    val showRecalled: Boolean = true,
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
