package com.sharepaste.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.core.AppException
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * A scan that arrives after the 120-second slot has expired.
 *
 * The third failure mode, proven by letting a real code really expire rather than
 * by handing the screen a state and believing it. It costs the suite a little over
 * two minutes, which is the price of the slot's actual lifetime — the relay is
 * what decides when a code dies (`pairingTtlMs = 2 * 60 * 1000`), and it answers a
 * claim against a dead slot with `410 Gone`, which the core maps to
 * `AppException.PairExpired`.
 *
 * Faking it with an unknown pair id would take the `404` path instead, which the
 * app maps to the same sentence but is a different journey through the relay. If
 * the relay ever changes which status an expired slot gets, this test notices and
 * that one would not.
 */
@RunWith(AndroidJUnit4::class)
class ExpiredCodeTest {

    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    private lateinit var repo: SharepasteRepository

    @Before
    fun open() {
        listOf("", "-wal", "-shm").forEach { File(context.filesDir, DATABASE + it).delete() }
        repo = SharepasteRepository.open(context, requireHttps = false, databaseName = DATABASE)
    }

    @After
    fun close() {
        runBlocking { repo.close() }
    }

    @Test
    fun a_code_claimed_after_its_slot_expires_is_reported_as_expired() {
        val other = Inviter.shared()
        val code = other.freshCompactCode()
        Evidence.log("expiry        = a real slot opened; waiting ${WAIT_MS / 1000}s for the relay to close it")

        // The relay holds the slot for 120 seconds. Waiting past it is the only
        // way to observe the real thing.
        Thread.sleep(WAIT_MS)

        try {
            runBlocking { repo.pairWithCode(code, "too late") }
            fail("a code whose 120-second slot has closed must not pair")
        } catch (e: AppException.PairExpired) {
            Evidence.log("expired       = AppException.PairExpired: ${e.message}")
            assertTrue("the relay's own reason travels with it", e.detail.isNotEmpty())
        }

        // Nothing was left behind by the failed attempt.
        assertEquals(0, runBlocking { repo.listPairings() }.size)
    }

    private companion object {
        const val DATABASE = "expiry-proof.db"

        /** The relay's `pairingTtlMs` is 120s; five seconds past it removes any doubt. */
        const val WAIT_MS = 125_000L
    }
}
