package com.sharepaste.android

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.ui.SessionPhase
import kotlinx.coroutines.runBlocking
import org.junit.FixMethodOrder
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.MethodSorters

/**
 * A phone holding two Pairings keeps syncing the **Active** one across a restart.
 *
 * Two halves, named so JUnit runs them in order, and the interesting gap is
 * between them:
 *
 * ```
 * adb shell am instrument -w -e class '…TwoPairingsSurviveRestartTest#a_…' …
 * adb shell am force-stop com.sharepaste.android
 * adb shell am instrument -w -e class '…TwoPairingsSurviveRestartTest#b_…' …
 * ```
 *
 * `am force-stop` cannot be driven from inside: the instrumentation runs in the
 * app's own process and would be killed with it. The gap is the host's job — see
 * the issue file.
 *
 * The claim is not merely that two rows survive. The `a_` half leaves an Entry on
 * the Relay for **each** Pairing while the phone is closed, and `b_` asserts that
 * only the Active one's arrives. A phone that came back up on the wrong Pairing,
 * or on both, fails on the Entries rather than on a settings value.
 *
 * `b_` needs no inviting device and claims no invite token, which is what lets it
 * run in a process that has just been created by `am instrument`.
 *
 * Both halves work on the app's **own** database, like ticket 09's
 * `PairingSurvivesRestartTest`, and both begin by forgetting whatever is in it.
 * JUnit runs a class's methods contiguously, so the two restart suites cannot
 * interleave; if that ever changes, they will fight over one file.
 */
@RunWith(AndroidJUnit4::class)
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class TwoPairingsSurviveRestartTest {

    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val application = context.applicationContext as SharepasteApplication

    /**
     * Pair twice into the app's own database, choose one, and leave an Entry on
     * the Relay for each — with the phone closed, so neither can arrive yet.
     */
    @Test
    fun a_pairs_twice_and_chooses_one() {
        val repo = SharepasteRepository.open(
            context,
            requireHttps = false,
            databaseName = SharepasteRepository.DATABASE_NAME,
        )
        try {
            runBlocking {
                repo.listPairings().forEach { repo.forgetPairing(it.userId) }
                val active = repo.pairWithCode(Inviter.shared().freshCompactCode(), ACTIVE_LABEL)
                val resting = repo.pairWithCode(Inviter.second().freshCompactCode(), RESTING_LABEL)
                repo.setActivePairing(active.userId)
                // Pairing brings a session up of its own accord. Put the phone
                // back down, or the Entries below arrive over a live stream and
                // the restart proves nothing.
                repo.stopAllSessions()
                Evidence.log(
                    "persisted     = active=${active.userId}/$ACTIVE_LABEL " +
                        "resting=${resting.userId}/$RESTING_LABEL",
                )
            }
        } finally {
            // Closed so the second half opens the file rather than sharing this
            // handle — which is the point of the exercise.
            runBlocking { repo.close() }
        }

        Inviter.shared().offerAndWaitForUpload(ACTIVE_TEXT)
        Inviter.second().offerAndWaitForUpload(RESTING_TEXT)
        Evidence.log("while closed  = one Entry on the Relay for each of the two Pairings")
    }

    /**
     * Both Pairings are still here, the Active one is the one that was chosen,
     * and it is the only one this phone syncs.
     */
    @Test
    fun b_both_are_still_here_and_syncing_follows_the_active_one() {
        // The shipped facade first: `BuildConfig.REQUIRE_HTTPS`, the app's
        // `filesDir`, the app's keystore-backed keychain, and nothing that opens
        // a socket. This is the storage half of the claim, on the configuration
        // that actually ships.
        val pairings = runBlocking { application.repository.listPairings() }
        Evidence.log(
            "after restart = ${pairings.size} pairing(s): " +
                pairings.joinToString { "${it.userId}/${it.label}" },
        )
        assertEquals("both Pairings must survive", 2, pairings.size)
        val active = pairings.single { it.label == ACTIVE_LABEL }
        val resting = pairings.single { it.label == RESTING_LABEL }
        assertEquals(
            "the Active Pairing is a settings value the resume path has to find again",
            active.userId,
            runBlocking { application.repository.resumeActivePairing() },
        )
        Evidence.log(
            "resumed       = ${active.userId} on the shipped " +
                "requireHttps=${BuildConfig.REQUIRE_HTTPS} facade",
        )

        // The syncing half needs a socket to the plain-HTTP test Relay, so it
        // runs on a facade of its own — the shipped one refuses cleartext, and
        // `TransportPolicyTest` is the test that keeps that true.
        val phone = PhoneUnderTest.open(
            compose,
            SharepasteRepository.DATABASE_NAME,
            fresh = false,
        )
        phone.pairedUserIds += listOf(active.userId, resting.userId)
        try {
            phone.enterForeground()
            phone.await("the Active Pairing must come back up on its own") {
                it.session == SessionPhase.InContact(active.userId)
            }
            val arrived = phone.awaitEntry("the Entry left on the Relay for the Active Pairing") {
                it.preview == ACTIVE_TEXT
            }
            Evidence.log("backfilled    = id=${arrived.id} preview=${arrived.preview}")

            assertTrue(
                "an Entry offered to the Pairing this phone is *not* syncing must not be here. " +
                    "A phone that came back up on both Pairings would pass every assertion " +
                    "about settings and still be wrong.",
                runBlocking { phone.repo.listHistory(resting.userId) }
                    .none { it.preview == RESTING_TEXT },
            )
            Evidence.log("not synced    = the resting Pairing's Entry stayed on the Relay")
        } finally {
            phone.close()
        }
    }

    private companion object {
        /** Distinct enough to identify across a process death. */
        const val ACTIVE_LABEL = "two pairings: the active one"
        const val RESTING_LABEL = "two pairings: the resting one"

        /**
         * Constant, with no timestamp in it, because the half that asserts on
         * these strings runs in a process that never saw the half that wrote
         * them. The Users are fresh per run — each costs an invite token — so a
         * constant cannot collide with an earlier run.
         */
        const val ACTIVE_TEXT = "offered-to-the-active-pairing-while-the-phone-was-gone"
        const val RESTING_TEXT = "offered-to-the-pairing-this-phone-does-not-sync"
    }
}
