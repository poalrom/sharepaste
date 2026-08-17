package com.sharepaste.android

import com.sharepaste.android.ui.Confirmation
import com.sharepaste.android.ui.Filtered
import com.sharepaste.android.ui.SessionPhase
import com.sharepaste.android.ui.UiState
import com.sharepaste.core.Entry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * What a phone forgets when it is put down, and the one thing it now keeps.
 *
 * The **Viewed Pairing** goes and the **Filter** stays (ADR 0019), which is the
 * reverse of the desktop and the reverse of what this phone did until that
 * record. Both are transient view choices, so the difference is not derivable
 * from what they are: a needle narrows *whichever* History is shown, while a
 * Viewed Pairing chooses which one that is, and only the second can leave rows
 * on screen attributed to the wrong User.
 *
 * A JVM test because the rule is a function of the snapshot and nothing else.
 * The alternative is a `leaveForeground` → `enterForeground` round trip against
 * a live Relay, which would prove the same sentence a hundred times more slowly
 * and would still not say which field the answer came from.
 *
 * **`scanned` is the load-bearing case.** [UiState.shown] hands back the last
 * scan verbatim whenever a needle is on and the list is not empty, so a needle
 * that now outlives a put-down must not leave an answer about rows that are
 * gone: on the way back in, `refreshHistory` fills the list for the Active
 * Pairing, and a surviving scan of another Pairing's rows would be shown under
 * the Active Pairing's name.
 */
class WhatAPutDownForgetsTest {

    private fun entry(id: Long, plaintext: String) = Entry(
        id = id,
        userId = "u",
        preview = plaintext,
        plaintext = plaintext,
        createdAt = 1_700_000_000_000,
        lastUse = 1_700_000_000_000,
        deviceId = "other-device",
        deviceLabel = "the laptop",
        originLabel = "the laptop",
        undecryptable = false,
        pending = false,
        refusedReason = null,
    )

    private val rows = listOf(entry(2, "ssh deploy@staging"), entry(1, "the standing order"))

    /** A phone looking at its own Active Pairing, with a needle in the field. */
    private fun looking() = UiState(
        session = SessionPhase.InContact("active"),
        foreground = true,
        activeUserId = "active",
        entries = rows,
        filter = "ssh",
        scanned = Filtered(needle = "ssh", entries = listOf(rows[0])),
    )

    /**
     * The needle survives, and the Viewed Pairing does not.
     *
     * One test for both because the pair is the decision: keeping the Filter is
     * only defensible while the thing that could misattribute rows still goes.
     */
    @Test
    fun the_filter_survives_and_the_viewed_pairing_does_not() {
        val down = looking().copy(viewedUserId = "held").putDown()

        assertEquals(
            "the Filter must outlive the surface being put down. A two-second flip to an " +
                "authenticator and back is not somebody saying they were done filtering",
            "ssh",
            down.filter,
        )
        assertNull(
            "the Viewed Pairing must still go: rows left behind would be read as the Active " +
                "Pairing's on the way back in, and attributed to the wrong User",
            down.viewedUserId,
        )
    }

    /**
     * The phone stops thinking it is in front.
     *
     * The flag is what tells a disconnected session apart from a resting one, so
     * a put-down that left it standing would put "not in contact, looking" on a
     * screen nobody is looking at.
     */
    @Test
    fun the_phone_stops_thinking_it_is_in_front() {
        assertFalse("a put-down phone is not in front", looking().putDown().foreground)
    }

    /**
     * A question waiting to be answered is taken back.
     *
     * A confirmation strip guards something that cannot be undone. Coming back
     * to a phone already holding "delete every Entry?" from minutes ago is a
     * destructive action one stray tap away from an answer nobody remembers
     * being asked for.
     */
    @Test
    fun a_pending_confirmation_is_taken_back() {
        val down = looking().copy(confirming = Confirmation.ClearHistory("active")).putDown()
        assertNull("a destructive question must not wait through a put-down", down.confirming)
    }

    /**
     * A diverged phone drops the other Pairing's rows **and** the scan of them.
     *
     * Both or neither. The rows are what the list draws and the scan is what it
     * draws while a needle is on, so dropping one and keeping the other is how
     * one Pairing's Entries end up under another's name for the frame between
     * `refreshHistory` and the next scan.
     */
    @Test
    fun a_diverged_phone_drops_the_rows_and_the_answer_about_them() {
        val down = looking().copy(viewedUserId = "held").putDown()

        assertEquals(
            "the rows belonged to the Pairing being stopped looking at",
            emptyList<Entry>(),
            down.entries,
        )
        assertEquals(
            "and so did the scan of them. With a needle surviving, [UiState.shown] returns " +
                "this verbatim as soon as the list is filled again",
            Filtered(),
            down.scanned,
        )
    }

    /**
     * A phone looking at its own Active Pairing keeps both.
     *
     * The nominal open, and the whole reason the Filter now survives: the rows
     * come back to the same list, the needle still answers the same question
     * about it, and nothing has to be re-typed or re-scanned to show it.
     */
    @Test
    fun an_undiverged_phone_keeps_its_rows_and_its_scan() {
        val down = looking().putDown()

        assertEquals("the rows are the Active Pairing's and still are", rows, down.entries)
        assertEquals(
            "the scan answers the surviving needle about the surviving rows",
            Filtered(needle = "ssh", entries = listOf(rows[0])),
            down.scanned,
        )
    }
}
