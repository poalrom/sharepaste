package com.sharepaste.android

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.ui.Notice
import com.sharepaste.android.ui.Screen
import com.sharepaste.android.ui.SessionPhase
import com.sharepaste.android.ui.TAG_BACK_TO_HISTORY
import com.sharepaste.android.ui.TAG_FAULT
import com.sharepaste.android.ui.TAG_OFFER
import com.sharepaste.android.ui.TAG_OPEN_PAIRINGS
import com.sharepaste.android.ui.TAG_PENDING
import com.sharepaste.android.ui.pairActiveTag
import com.sharepaste.android.ui.pairPendingTag
import com.sharepaste.android.ui.pairUseTag
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * A queue on a Pairing this phone has **switched away from**, on screen.
 *
 * The card is the single place it is visible at all: the History's own count
 * belongs to the Active Pairing, so the moment the device moves on, Entries that
 * were captured and never uploaded become invisible everywhere else — kept, not
 * sent, and not mentioned.
 *
 * Manufacturing one takes a Pairing whose Relay can be taken away, which means
 * [RelayProxy] and a Pairing made against it: a short code carries the *inviting*
 * device's address inside its payload, so pairing against a port this test owns
 * costs a single-use invite of its own.
 */
@RunWith(AndroidJUnit4::class)
class PendingOnANonActivePairingTest {

    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val resources = InstrumentationRegistry.getInstrumentation().targetContext.resources

    private lateinit var proxy: RelayProxy
    private lateinit var phone: PhoneUnderTest
    private lateinit var queued: String
    private lateinit var movedTo: String

    @Before
    fun open() {
        proxy = RelayProxy.inFrontOfTheTestRelay()
        phone = PhoneUnderTest.open(compose, DATABASE)
        queued = phone.pairWithInvite(proxy.url, "the Pairing that keeps a queue")
        movedTo = phone.pairWithCode(Inviter.shared(), "the Pairing this phone moves to")
        phone.makeActive(queued)
        Evidence.log("proxy         = $queued paired against ${proxy.url} -> ${TestRelay.url}")
    }

    @After
    fun close() {
        // The Relay first, so `forgetPairing` can reach it.
        proxy.reopen()
        phone.close()
        proxy.shutdown()
    }

    @Test
    fun a_queue_on_a_pairing_the_phone_switched_away_from_is_still_on_screen() {
        phone.enterForeground()
        phone.await("in contact through the proxy") {
            it.session == SessionPhase.InContact(queued)
        }

        proxy.close()
        proxy.assertUnreachable()
        Evidence.log("relay gone    = the proxy is closed; the port now refuses connections")

        val stranded = "captured-and-never-sent-${System.currentTimeMillis()}"
        phone.clip.putText(stranded)
        compose.onNodeWithTag(TAG_OFFER).performClick()
        phone.await("an Offer made offline is still taken") { it.notice is Notice.Offered }
        phone.await("and the count reaches the screen") { it.pending == 1L }

        // Now move the device on. The Entry is still here, still un-uploaded, and
        // from this point the History's own count is about a different Pairing.
        compose.onNodeWithTag(TAG_OPEN_PAIRINGS).performClick()
        phone.await("the Pairings screen") { it.screen == Screen.Pairings }
        phone.scrollToPairing(pairUseTag(movedTo))
        compose.onNodeWithTag(pairUseTag(movedTo)).performClick()
        phone.await("the Active Pairing must move") { it.activeUserId == movedTo }

        phone.await("the card must carry the queue of the Pairing left behind") {
            it.pairings.firstOrNull { p -> p.userId == queued }?.pending == 1L
        }
        val one = resources.getQuantityString(R.plurals.pending_count, 1, 1)
        phone.scrollToPairing(pairPendingTag(queued))
        compose.onNodeWithTag(pairPendingTag(queued)).assertTextEquals(one)
        compose.onNodeWithTag(pairActiveTag(queued)).assertDoesNotExist()
        // And it still is not a fault. A Pairing holding a queue it cannot send
        // is resting, not broken.
        compose.onNodeWithTag(TAG_FAULT).assertDoesNotExist()
        Evidence.log("queue kept    = $queued (not active) shows: $one")

        // The other half of "the single place it is visible": the History says
        // nothing about it, because the History is about the Active Pairing.
        compose.onNodeWithTag(TAG_BACK_TO_HISTORY).performClick()
        phone.await("back on the History") { it.screen == Screen.History }
        assertEquals(
            "the History's count belongs to the Active Pairing, which has nothing queued",
            0L,
            phone.state.pending,
        )
        compose.onNodeWithTag(TAG_PENDING).assertDoesNotExist()
        Evidence.log("history says  = nothing; the card is the only surface that shows it")
    }

    private companion object {
        const val DATABASE = "pending-elsewhere-proof.db"
    }
}
