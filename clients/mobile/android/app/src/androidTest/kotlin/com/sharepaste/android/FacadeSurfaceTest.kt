package com.sharepaste.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.platform.AndroidClipboard
import com.sharepaste.android.platform.AndroidKeychain
import com.sharepaste.core.AppException
import com.sharepaste.core.ConnectionState
import com.sharepaste.core.HotkeyPatch
import com.sharepaste.core.Sharepaste
import com.sharepaste.core.SettingsPatch
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Every exported operation is reachable from Kotlin, and all three platform
 * traits are supplied by Kotlin.
 *
 * Most operations need a pairing, and this facade has none, so most of them are
 * exercised through their failure. That is not a weaker test than a happy path:
 * a typed `AppException` arriving in Kotlin proves the whole chain — argument
 * lowering, the blocking `block_on`, the error's variant surviving the crossing
 * — for exactly the same code the happy path would use. The one variant that
 * genuinely matters on a phone, `InsecureRelay`, can only be produced this way.
 */
@RunWith(AndroidJUnit4::class)
class FacadeSurfaceTest {

    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    private lateinit var keychain: AndroidKeychain
    private lateinit var clipboard: RecordingClipboard
    private lateinit var sink: RecordingSink
    private lateinit var core: Sharepaste

    @Before
    fun open() {
        keychain = AndroidKeychain(context)
        clipboard = RecordingClipboard(AndroidClipboard(context))
        sink = RecordingSink()
        // `false` on purpose: this suite reaches the cleartext test relay, which
        // the shipped app's `true` would refuse. That the app itself passes `true`
        // is what `TransportPolicyTest` exists to prove.
        core = Sharepaste.openInMemory(keychain, clipboard, sink, requireHttps = false)
    }

    @After
    fun close() {
        core.stopAllSessions()
        core.close()
    }

    @Test
    fun kotlin_supplies_the_keychain() {
        // The real `EncryptedSharedPreferences` implementation, exercised on
        // the device: a master key in the Android Keystore, which is the one
        // part of this that cannot be tested anywhere else.
        keychain.delete("surface-test")
        assertNull(keychain.get("surface-test"))
        keychain.put("surface-test", "a-secret")
        assertEquals("a-secret", keychain.get("surface-test"))
        keychain.delete("surface-test")
        assertNull(keychain.get("surface-test"))
        Evidence.log("keychain      = put/get/delete round-tripped through the Android Keystore")
    }

    @Test
    fun kotlin_supplies_the_clipboard() {
        core.writeClipboard("clipboard crossing")
        assertEquals(listOf("clipboard crossing"), clipboard.written)
        // A read is allowed to return null: since Android 10 the clipboard is
        // readable only by the focused app or the default IME, and an
        // instrumentation run is neither. Not a bug — the platform rule ADR
        // 0007 is built on.
        Evidence.log("clipboard     = wrote via the facade, read back ${clipboard.readText()}")
    }

    @Test
    fun the_read_only_operations_answer_on_an_empty_facade() {
        assertEquals(emptyList<Any>(), core.listPairings())
        assertNull(core.activePairing())
        assertNull(core.resumeActivePairing())
        assertEquals(ConnectionState.DISCONNECTED, core.connectionState("nobody"))
        assertEquals(emptyList<Any>(), core.listHistory("nobody", null, 50))
        assertNull(core.readEntry("nobody", 1))
        Evidence.log("empty facade  = no pairings, no active user, disconnected, no history")
    }

    @Test
    fun settings_round_trip_through_a_typed_patch() {
        val before = core.getSettings()
        Evidence.log("settings pre  = capture=${before.captureEnabled} hotkey=${before.hotkey}")

        val disabled = core.updateSettings(SettingsPatch(captureEnabled = false))
        assertEquals(false, disabled.captureEnabled)

        val bound = core.updateSettings(SettingsPatch(hotkey = HotkeyPatch.Set("CommandOrControl+Shift+V")))
        assertEquals("CommandOrControl+Shift+V", bound.hotkey)

        // The reason `hotkey` is an enum rather than a nullable: Kotlin has no
        // `String??`, and "leave it alone" and "clear it" are different asks.
        val cleared = core.updateSettings(SettingsPatch(hotkey = HotkeyPatch.Clear))
        assertNull(cleared.hotkey)
        assertEquals(false, cleared.captureEnabled)

        val untouched = core.updateSettings(SettingsPatch(denyList = listOf("com.example.vault")))
        assertEquals(listOf("com.example.vault"), untouched.denyList)
        assertNull("an absent field must not clear a stored one", untouched.hotkey)
        Evidence.log("settings post = capture=${untouched.captureEnabled} deny=${untouched.denyList}")
    }

    @Test
    fun a_cleartext_relay_is_explained_rather_than_guessed_at() {
        // Port 1 refuses immediately. The core does not reject `http://` — a
        // desktop paired to a cleartext relay must keep working — but when the
        // request fails it says why, instead of surfacing an opaque transport
        // error a phone's owner cannot act on.
        try {
            core.pairWithInvite("http://127.0.0.1:1", "not-a-real-token", "instrumented test")
            fail("pairing against a dead cleartext relay must fail")
        } catch (e: AppException.InsecureRelay) {
            Evidence.log("insecure relay= ${e.message}")
            assertTrue(e.message!!.isNotEmpty())
        }
    }

    @Test
    fun every_operation_that_needs_a_pairing_says_so_in_its_own_words() {
        val failures = buildList {
            add("pairStart" to failureOf { core.pairStart("nobody") })
            add("pairWithCode" to failureOf { core.pairWithCode("not-a-code", "instrumented test") })
            add("startSession" to failureOf { core.startSession("nobody") })
            add("recall" to failureOf { core.recall("nobody", 1) })
            add("recallLatest" to failureOf { core.recallLatest("nobody") })
            add("offer" to failureOf { core.offer("nobody", "text") })
            add("deleteEntry" to failureOf { core.deleteEntry("nobody", 1) })
            add("clearHistory" to failureOf { core.clearHistory("nobody") })
        }
        failures.forEach { (name, failure) -> Evidence.log("$name -> $failure") }
        failures.forEach { (name, failure) ->
            assertTrue("$name did not raise a typed AppException", failure.startsWith("AppException"))
        }
    }

    @Test
    fun the_tolerant_operations_are_callable_on_an_empty_facade() {
        // These four do not fail on an unknown user, and that is the facade's
        // behaviour rather than an oversight: selecting and forgetting are
        // idempotent bookkeeping, and stopping something that is not running is
        // what `onStop` does on every launch.
        core.setActivePairing("nobody")
        assertEquals("nobody", core.activePairing())
        core.forgetPairing("nobody")
        assertNull("forgetting the active pairing must clear it", core.activePairing())
        core.stopSession("nobody")
        core.stopAllSessions()
        Evidence.log("contact       = ${runCatching { core.getContact("nobody") }}")
        Evidence.log("tolerant ops  = setActivePairing/forgetPairing/stopSession/stopAllSessions all returned")
    }

    private inline fun failureOf(block: () -> Unit): String =
        try {
            block()
            "no failure"
        } catch (e: AppException) {
            "AppException.${e::class.java.simpleName}: ${e.message}"
        }
}
