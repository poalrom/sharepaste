package com.sharepaste.android

import android.util.Log

/**
 * Test output that survives the run.
 *
 * `connectedAndroidTest` reports pass or fail and nothing else, which is not
 * evidence of anything. Each assertion that is worth reading — the exact bytes
 * of a known-answer vector, the tables a migration created, the event a session
 * task raised — is logged under one tag so the run can be quoted verbatim with
 * `adb logcat -s SharepasteProof`.
 */
object Evidence {
    const val TAG = "SharepasteProof"

    fun log(line: String) = Log.i(TAG, line)

    fun ByteArray.hex(): String = joinToString("") { "%02x".format(it) }

    fun hexToBytes(hex: String): ByteArray {
        require(hex.length % 2 == 0) { "odd-length hex" }
        return ByteArray(hex.length / 2) { hex.substring(it * 2, it * 2 + 2).toInt(16).toByte() }
    }
}
