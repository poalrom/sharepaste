package com.sharepaste.android

import com.sharepaste.android.ui.UiState
import com.sharepaste.core.Entry
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Which arrivals move the list, and which are news that can wait.
 *
 * `CoreEvent.EntryAdded` has two halves and one event, so the whole of ADR 0019's
 * arrival rule turns on telling them apart. An Entry from **another Device**
 * landed above somebody who is reading and moves nothing. An Entry this phone
 * captured itself is a **Capture**, which is a **Use** (CONTEXT.md), and a Use
 * this phone made follows its row to the head — the Offer's half of the rule
 * `recall` states, and the reason `RECALL FIRST` is never left pointing at a row
 * off screen.
 *
 * A JVM test because the distinction is a function of the snapshot: the core
 * stamps an Entry's **Origin**, so nothing here is derived from the list's shape.
 * The instrumented tests cannot reach this — they drive the screen, and by then
 * the choice has already been made.
 */
class WhichArrivalsMoveTheListTest {

    private fun entry(deviceId: String) = Entry(
        id = 7,
        userId = "u",
        preview = "ssh deploy@staging",
        plaintext = "ssh deploy@staging",
        createdAt = 1_700_000_000_000,
        lastUse = 1_700_000_000_000,
        deviceId = deviceId,
        deviceLabel = "the laptop",
        originLabel = "the laptop",
        undecryptable = false,
        pending = false,
        refusedReason = null,
    )

    private val phone = UiState(activeUserId = "u", ownDeviceId = "this-phone")

    /** An Offer made here follows its own row. */
    @Test
    fun an_entry_this_phone_captured_is_its_own() {
        assertTrue(
            "an Offer made on this phone must follow its row to the head, or the verb bar " +
                "hands over an Entry the person cannot see",
            phone.capturedHere(entry("this-phone")),
        )
    }

    /** An Entry from the laptop is news, and news does not move anybody. */
    @Test
    fun an_entry_from_another_device_is_not() {
        assertFalse(
            "an Entry from another Device arrived while somebody was reading; chasing it " +
                "costs them their Place for a row they can reach by scrolling",
            phone.capturedHere(entry("the-laptop")),
        )
    }

    /**
     * Before this phone knows its own Device id, nothing is its own.
     *
     * `ownDeviceId` is `null` until the Pairings have been read back, which is a
     * window of a few frames after every open. The safe direction is the quiet
     * one: an Offer that does not scroll costs a scroll, and a stranger's Entry
     * that does costs a reader their place.
     */
    @Test
    fun an_unknown_own_device_claims_nothing() {
        assertFalse(
            "a phone that does not know which Device it is must not claim an Entry",
            phone.copy(ownDeviceId = null).capturedHere(entry("this-phone")),
        )
    }
}
