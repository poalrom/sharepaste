package com.sharepaste.android

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.ui.Notice
import com.sharepaste.android.ui.SessionPhase
import com.sharepaste.android.ui.TAG_NOTICE_STALE
import com.sharepaste.android.ui.TAG_OFFER
import com.sharepaste.android.ui.TAG_PENDING
import com.sharepaste.android.ui.TAG_RECALL_LATEST
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * What the phone does with no route to the Relay: it says what it handed over,
 * and it keeps what it could not send.
 *
 * The missing network is real and it is this test's own. [RelayProxy] forwards to
 * the Relay on a port this process owns; closing it gives the next request a
 * genuine connection refusal from the real network stack, and reopening it gives
 * the Relay back on the same port — which matters, because the port is baked into
 * the Pairing's stored `server_url`. The emulator's own switches (`svc data
 * disable`, airplane mode) would do the same thing to every other test in the run.
 *
 * A short code carries the *inviting* device's Relay address inside its payload,
 * so a phone that pairs by code talks to whatever that device talks to. Pairing
 * against a port this test controls therefore means claiming an invite of its own,
 * and costs the run a third single-use token.
 */
@RunWith(AndroidJUnit4::class)
class OfflineOfferAndRecallTest {

    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val resources = InstrumentationRegistry.getInstrumentation().targetContext.resources

    private lateinit var proxy: RelayProxy
    private lateinit var phone: PhoneUnderTest

    @Before
    fun open() {
        proxy = RelayProxy.inFrontOfTheTestRelay()
        phone = PhoneUnderTest.open(compose, DATABASE)
        phone.pairWithInvite(proxy.url, "offline test phone")
        Evidence.log("proxy         = paired against ${proxy.url}, forwarding to ${TestRelay.url}")
    }

    @After
    fun close() {
        // The Relay first, so `forgetPairing` can reach it and this User's Entries
        // do not outlive the test on the Relay.
        proxy.reopen()
        phone.close()
        proxy.shutdown()
    }

    /**
     * Recall Latest with no Relay falls back to the newest cached Entry **and says
     * so on screen.**
     *
     * The visible statement is the assertion, not the return value. A silent
     * fallback hands over yesterday's link and looks exactly like a success, so the
     * only proof worth having is the sentence a person would read.
     */
    @Test
    fun recall_latest_with_no_relay_falls_back_to_the_cache_and_says_so_on_screen() {
        phone.enterForeground()
        phone.await("in contact through the proxy") { it.session is SessionPhase.InContact }

        // Something to fall back *to*. Offered here and round-tripped through the
        // Relay, so it is a genuinely cached Entry rather than a fixture.
        val cached = "the-newest-thing-this-phone-had-${System.currentTimeMillis()}"
        phone.clip.putText(cached)
        compose.onNodeWithTag(TAG_OFFER).performClick()
        phone.await("the seed Offer must be taken") { it.notice is Notice.Offered }
        phone.awaitEntry("the seed Offer must come back from the Relay and be cached") {
            it.preview == cached
        }
        phone.model.dismissNotice()

        proxy.close()
        proxy.assertUnreachable()
        Evidence.log("relay gone    = the proxy is closed; the port now refuses connections")

        phone.clip.putText("something else entirely")
        compose.onNodeWithTag(TAG_RECALL_LATEST).performClick()
        phone.await("Recall Latest must settle") { it.notice != null }

        assertEquals(
            "a fetch that failed must be reported as a cache read, not as a success",
            Notice.RecalledFromCache,
            phone.state.notice,
        )
        val sentence = resources.getString(R.string.recall_from_cache)
        compose.onNodeWithTag(TAG_NOTICE_STALE).assertIsDisplayed()
        compose.onNodeWithText(sentence).assertIsDisplayed()
        Evidence.log("stale recall  = on screen: $sentence")

        assertEquals(
            "the fallback still has to hand over the newest Entry it had",
            cached,
            phone.clip.requireText("after Recall Latest with the Relay gone"),
        )
    }

    /**
     * An Offer with no route to the Relay is kept, the count is on screen, and the
     * queue flushes on the **next foreground** — never in the background.
     *
     * The last clause is the one worth the trouble. The uploader lives on the
     * session and the session only exists in the foreground (ADR 0007), so the
     * proof is that the Relay comes back while the phone is still put down and
     * *nothing happens* until it is picked up again.
     */
    @Test
    fun an_offer_with_no_relay_is_queued_and_flushes_on_the_next_foreground() {
        phone.enterForeground()
        phone.await("in contact through the proxy") { it.session is SessionPhase.InContact }

        proxy.close()
        proxy.assertUnreachable()
        Evidence.log("relay gone    = the proxy is closed before the Offer")

        val queued = "offered-with-no-connection-${System.currentTimeMillis()}"
        phone.clip.putText(queued)
        compose.onNodeWithTag(TAG_OFFER).performClick()
        phone.await("an Offer made offline must still be taken") { it.notice is Notice.Offered }
        phone.await("the pending count must reach the screen") { it.pending == 1L }

        val one = resources.getQuantityString(R.plurals.pending_count, 1)
        compose.onNodeWithTag(TAG_PENDING).assertTextEquals(one)
        Evidence.log("queued        = $one")

        // Put the phone down, *then* give the Relay back. A session that was still
        // up would reconnect and upload on its own, which would prove nothing
        // about foregrounds.
        phone.leaveForeground()
        phone.await("resting after onStop") { it.session is SessionPhase.Resting }
        proxy.reopen()
        Evidence.log("relay back    = reachable again, but this phone is put down")

        Thread.sleep(BACKGROUND_WINDOW_MS)
        assertEquals(
            "the queue flushed in the background. Nothing may upload while the app is closed — " +
                "that is the whole of ADR 0007",
            1L,
            phone.state.pending,
        )
        compose.onNodeWithTag(TAG_PENDING).assertTextEquals(one)

        phone.enterForeground()
        phone.await("the queue must flush on the foreground that follows") { it.pending == 0L }
        compose.onNodeWithTag(TAG_PENDING).assertDoesNotExist()
        val arrived = phone.awaitEntry("the flushed Entry must come back off the Relay") {
            it.preview == queued
        }
        Evidence.log("flushed       = pending 1 -> 0; Entry id=${arrived.id} is on the Relay")
    }

    private companion object {
        const val DATABASE = "offline-proof.db"

        /**
         * How long the Relay is reachable while the phone is put down.
         *
         * Long enough that "still pending" means nothing tried rather than that
         * nothing had had time to — an uploader that was going to run would have
         * run several times over by now.
         */
        const val BACKGROUND_WINDOW_MS = 6_000L
    }
}
