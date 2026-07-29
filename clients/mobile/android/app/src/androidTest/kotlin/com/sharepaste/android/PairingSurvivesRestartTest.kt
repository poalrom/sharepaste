package com.sharepaste.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.FixMethodOrder
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.MethodSorters

/**
 * The Pairing survives a force-quit and a restart.
 *
 * Two halves, named so JUnit runs them in order, and the interesting gap is
 * between them:
 *
 * ```
 * ./gradlew :app:connectedDebugAndroidTest                      # runs a_ and b_
 * adb shell am force-stop com.sharepaste.android                 # the force-quit
 * adb shell am instrument -w -e class \
 *   com.sharepaste.android.PairingSurvivesRestartTest#b_the_pairing_is_still_there \
 *   com.sharepaste.android.test/androidx.test.runner.AndroidJUnitRunner
 * ```
 *
 * `am force-stop` cannot be driven from inside this test: the instrumentation runs
 * in the app's own process, so force-stopping the app kills the test with it. The
 * gap is therefore the host's job, and the second half is a class the host can run
 * on its own afterwards.
 *
 * Both halves are worth having even in one run. Within a single process the second
 * half still opens a **fresh facade, with the shipped app's own transport policy**,
 * over the same database and the same keystore-backed keychain — which is the
 * storage half of the claim. The force-stop run adds the process half.
 *
 * The `a_` half pairs with `requireHttps = false`, because the test relay is plain
 * HTTP. The `b_` half runs entirely on the app's real `requireHttps = true` facade
 * — nothing it calls opens a socket, so the policy does not stand in the way of
 * proving persistence on the shipped configuration.
 */
@RunWith(AndroidJUnit4::class)
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class PairingSurvivesRestartTest {

    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val application = context.applicationContext as SharepasteApplication

    @Test
    fun a_pairs_into_the_apps_own_database() {
        val other = Inviter.shared()
        // The app's own database name and the app's own keychain, so that what is
        // written here is what the app itself will read back.
        val repo = SharepasteRepository.open(
            context,
            requireHttps = false,
            databaseName = SharepasteRepository.DATABASE_NAME,
        )
        try {
            runBlocking {
                repo.listPairings().forEach { repo.forgetPairing(it.userId) }
                val paired = repo.pairWithCode(other.freshCompactCode(), LABEL)
                repo.setActivePairing(paired.userId)
                Evidence.log("persisted     = user=${paired.userId} label=$LABEL into the app's own db")
            }
        } finally {
            // Closed so the second half opens the file rather than sharing this
            // handle — which is the point of the exercise.
            runBlocking { repo.close() }
        }
    }

    @Test
    fun b_the_pairing_is_still_there() {
        // The shipped facade: `BuildConfig.REQUIRE_HTTPS`, the app's `filesDir`,
        // the app's keystore-backed keychain. Nothing arranged.
        val repo = application.repository
        val pairings = runBlocking { repo.listPairings() }
        Evidence.log(
            "after restart = ${pairings.size} pairing(s): " +
                pairings.joinToString { "${it.userId}/${it.label}" },
        )

        val mine = pairings.singleOrNull { it.label == LABEL }
        assertNotNull(
            "no Pairing labelled \"$LABEL\" survived. Pairings seen: $pairings",
            mine,
        )
        assertEquals(TestRelay.url, mine!!.serverUrl)

        // And it is still the Active Pairing, which is the half a restart is most
        // likely to lose: the label is a row in the database, but the Active
        // Pairing is a settings value the resume path has to find again.
        val resumed = runBlocking { repo.resumeActivePairing() }
        assertEquals(
            "the Active Pairing did not survive; `onStart` would land on the pairing screen",
            mine.userId,
            resumed,
        )
        Evidence.log("resumed       = $resumed on the shipped requireHttps=${BuildConfig.REQUIRE_HTTPS} facade")
    }

    private companion object {
        /** Distinct enough to identify across a process death. */
        const val LABEL = "survives a force-stop"
    }
}
