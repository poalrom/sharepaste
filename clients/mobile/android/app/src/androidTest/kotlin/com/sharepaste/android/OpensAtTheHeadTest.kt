package com.sharepaste.android

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.sharepaste.android.ui.HeadMove
import com.sharepaste.android.ui.Receipt
import com.sharepaste.android.ui.SessionPhase
import com.sharepaste.android.ui.TAG_OFFER
import com.sharepaste.android.ui.entryRowTag
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The join: a real open of a real phone puts the list at the head.
 *
 * ADR 0019's rule is held in two halves that never meet in any other test.
 * `OpenJumpTest` pins the gate's sequence on the JVM — armed at an open, spent
 * once — and `HistoryListTest` pins what the screen does when it is handed a
 * [HeadMove]. Between them sits the thing the record is actually about, and it
 * was covered nowhere: that a phone put down, left away while Entries landed,
 * and opened again raises exactly one motion, and that it is the jump.
 *
 * So this drives the two lifecycle edges `MainActivity` delegates to, over the
 * live Relay, with the **Catch-Up** doing the work rather than a hand-emitted
 * signal. The other device offers while the phone is closed, because that is the
 * only way to get a Catch-Up that finds something: an Entry that arrived over a
 * live stream is a different event and a different rule.
 *
 * `PhoneUnderTest` hands `model.headMoves` to the real [HistoryScreen] exactly as
 * the activity does. Until this test that argument was left defaulted, so the
 * harness rendered the one screen this rule governs with an `emptyFlow()` — which
 * is the class of wiring bug it exists to refuse.
 *
 * **What this deliberately does not cover.** A hand on the list spending the
 * gate: that needs a History long enough to drag, which over a real Relay is
 * twenty round trips bought to re-prove two things already proven apart — the
 * gate's own edge in `OpenJumpTest`, and the screen reporting a drag rather than
 * its own scrolling in `HistoryListTest`. What is left unproven is one line of
 * `appActions`. Nor is jump-versus-animation asserted here, for the reason
 * `HistoryListTest` gives: both end at index 0, so telling them apart means
 * counting frames.
 */
@RunWith(AndroidJUnit4::class)
class OpensAtTheHeadTest {

    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private lateinit var phone: PhoneUnderTest
    private lateinit var other: Inviter

    @Before
    fun open() {
        phone = PhoneUnderTest.open(compose, DATABASE)
        other = Inviter.shared()
        phone.pairWithCode(other, "the phone that was away")
        // `pairWithCode` leaves every session stopped, which is the state a phone
        // that has not reached its first `onStart` is in. Everything below depends
        // on it: an Entry offered "while the phone was closed" that arrived over a
        // live stream instead would exercise the other half of the rule.
    }

    @After
    fun close() {
        if (this::phone.isInitialized) phone.close()
    }

    /**
     * One method and three sections, in the order they have to happen.
     *
     * Split into three `@Test`s they would each pay for a pairing and a Relay
     * round trip to rebuild the state the previous one had already reached, and
     * the second and third assert about *not* moving — which is only meaningful
     * after something has moved once. `SessionLifecycleTest` is sectioned the same
     * way for the same reason.
     */
    @Test
    fun an_open_jumps_once_and_then_the_foreground_goes_quiet() {
        // --- the open ------------------------------------------------------
        //
        // Two Entries, because one would not tell "the list went to the head"
        // from "the head is where it already was". `offerAndWaitForUpload` on the
        // second is what makes this a Catch-Up rather than a race: the uploader
        // drains in the order acts were queued, so the second one being on the
        // Relay means the first is too.
        val first = "away-first-${System.currentTimeMillis()}"
        val second = "away-second-${System.currentTimeMillis()}"
        other.offer(first)
        other.offerAndWaitForUpload(second)
        Evidence.log("while closed  = the other device put two Entries on the Relay")

        phone.enterForeground()
        phone.await("the Catch-Up must bring both Entries the phone missed") {
            it.entries.size >= 2
        }
        val move = phone.awaitHeadMove("the open must put the list at the head")

        assertEquals(
            "the open's first Catch-Up must raise a jump and not a follow: nothing changed " +
                "under anybody's eyes, because nobody was looking at the list at all",
            HeadMove.Jump,
            move,
        )
        assertEquals(
            "and exactly one motion. A second would mean the gate never closed, which is the " +
                "arrival rule ADR 0019 replaced wearing this one's clothes",
            listOf(HeadMove.Jump),
            phone.headMoves,
        )
        // The head is on screen. Read out of the snapshot rather than predicted
        // from the two texts above: which Entry leads is the Relay's stamp to
        // make, and this assertion is about where the list is.
        val head = phone.state.entries.first()
        compose.onNodeWithTag(entryRowTag(head.id)).assertIsDisplayed()
        Evidence.log("opened        = ${phone.headMoves}, head=${head.id} on screen")

        // --- a second change in the same foreground ------------------------
        //
        // The phone is in contact now, so this one arrives over the live stream:
        // `EntryAdded` for the row, `HistoryChanged` for the order. Neither may
        // move anything. Waiting for `InContact` first is what makes it a live
        // arrival rather than a second Catch-Up.
        phone.await("the phone must be in contact before the live case") {
            it.session is SessionPhase.InContact
        }
        val live = "arrived-while-watching-${System.currentTimeMillis()}"
        other.offerAndWaitForUpload(live)
        phone.awaitEntry("the live stream must deliver the third Entry") { it.preview == live }

        assertEquals(
            "a mid-session arrival from another Device must move nothing. The gate is spent, " +
                "and chasing the row would cost a reader their Place for news they can scroll to",
            listOf(HeadMove.Jump),
            phone.headMoves,
        )
        Evidence.log("live arrival  = ${phone.headMoves} — still just the open's jump")

        // --- an Offer this phone made --------------------------------------
        //
        // The other half of `EntryAdded`, and the one that must still move: a
        // Capture is a **Use** (CONTEXT.md), and `RECALL FIRST` hands over the
        // first *displayed* row, so an Offer whose own row landed off screen is a
        // verb bar pointing at something nobody can see.
        val ours = "offered-here-${System.currentTimeMillis()}"
        phone.clip.putText(ours)
        compose.onNodeWithTag(TAG_OFFER).performClick()
        phone.awaitReceipt("the Offer must be taken") { it is Receipt.Offered }
        phone.awaitEntry("the Offer must reach the list") { it.preview == ours }
        compose.waitUntil(TIMEOUT_MS) { phone.headMoves.size >= 2 }

        assertEquals(
            "an Offer made here follows its own row to the head, and it animates: this is a " +
                "Use this phone made, not an arrival",
            listOf(HeadMove.Jump, HeadMove.Follow),
            phone.headMoves,
        )
        Evidence.log("our own offer = ${phone.headMoves}")
    }

    private companion object {
        const val DATABASE = "opens-at-the-head.sqlite"

        /** The suite's own wait, in the units `waitUntil` takes. */
        const val TIMEOUT_MS = PhoneUnderTest.TIMEOUT_SECONDS * 1_000
    }
}
