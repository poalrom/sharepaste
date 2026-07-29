package com.sharepaste.android.platform

import android.content.Context
import android.content.SharedPreferences
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import com.sharepaste.core.AppException
import com.sharepaste.core.Keychain

/**
 * The phone's answer to the desktop's system keychain.
 *
 * Preferences encrypted with a key that lives in the Android Keystore, which is
 * hardware-backed where the device has a secure element. This holds the user
 * key and the device token — the two secrets that, together, are the pairing.
 *
 * Called from whatever thread made the FFI call, never the main thread.
 */
class AndroidKeychain(context: Context) : Keychain {

    // Application context: this outlives any activity, and holding one would
    // leak it for the lifetime of the facade.
    private val appContext = context.applicationContext

    private val prefs: SharedPreferences by lazy {
        val masterKey = MasterKey.Builder(appContext)
            .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
            .build()
        EncryptedSharedPreferences.create(
            appContext,
            PREFS_NAME,
            masterKey,
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
        )
    }

    override fun put(account: String, secret: String) = guard("put") {
        // `commit`, not `apply`: the core treats a returned `put` as durable
        // and will write the pairing row next. An asynchronous flush would let
        // a crash between the two leave a pairing whose key is gone.
        check(prefs.edit().putString(account, secret).commit()) { "the write was not committed" }
    }

    override fun get(account: String): String? = guard("get") {
        prefs.getString(account, null)
    }

    override fun delete(account: String) = guard("delete") {
        check(prefs.edit().remove(account).commit()) { "the delete was not committed" }
    }

    /**
     * Keystore failures — a wiped key, a locked device, a corrupt preferences
     * file — arrive as any of a dozen unrelated exception types. They all mean
     * the same thing to the core, and none of them may cross the FFI boundary
     * as something other than an [AppException].
     */
    private inline fun <T> guard(operation: String, block: () -> T): T =
        try {
            block()
        } catch (e: Exception) {
            throw AppException.Keychain("$operation: ${e.message ?: e::class.java.simpleName}")
        }

    private companion object {
        const val PREFS_NAME = "sharepaste-keychain"
    }
}
