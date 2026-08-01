package com.sharepaste.android

import androidx.datastore.preferences.preferencesDataStoreFile
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.platform.UiPreferences
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Both UI preferences leave memory, and an instance that did not write them
 * reads them back.
 *
 * Ticket 02 asks that "both preferences survive process death" and ticket 04
 * says the same of the switch. This test cannot literally show that: the
 * instrumentation runs inside the app's own process, so force-stopping the app
 * kills the run with it. [PairingSurvivesRestartTest] meets the same wall and
 * hands the force-stop to the host. Rather than take a name for something it
 * does not do, this one proves the property the claim actually rests on, and
 * stops exactly there:
 *
 *  - **The values are in a file.** After a write there is a non-empty
 *    `ui.preferences_pb` in the app's own storage — the thing a restart has to
 *    find. Before that file exists there is nothing for durability to be a
 *    property of.
 *  - **A reader that did not write sees the write.** A [UiPreferences]
 *    constructed after the fact, over the same context, reads what an earlier
 *    one put there, rather than only the writer's own copy.
 *  - **An empty store answers with the defaults**, so the first launch after an
 *    install behaves as the tables in ticket 02 say.
 *
 * What is taken on trust is the last link only: DataStore's own contract that
 * bytes committed to that file come back after the process that wrote them is
 * gone. Nothing in this app influences that link.
 *
 * One honest caveat about the second point. `UiPreferences` is a thin wrapper
 * over **one process-wide DataStore object** — the `preferencesDataStore`
 * delegate hands every instance the same one — so a second instance may be
 * answered from that object's cache rather than from disk. The reader half is
 * therefore a check that `UiPreferences` holds no state of its own, and the
 * file half is what keeps the durability claim honest. Neither alone would be
 * enough; together they cover everything up to the disk.
 */
@RunWith(AndroidJUnit4::class)
class UiPreferencesTest {

    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    /**
     * The store is shared with the whole suite and with the app itself, so this
     * runs on both ends of every test.
     *
     * There is exactly one DataStore file named `ui` in this process. Leaving
     * `showRecalled = false` behind would silence the Receipt for every test
     * that ran afterwards, and leaving `foregroundNoteDismissed = true` behind
     * would take the History Screen's foreground band away from them — neither
     * failure would look like this class's fault. Clearing beforehand as well
     * means a previous run that crashed between the write and the teardown
     * cannot poison this one.
     */
    @Before
    fun startFromTheDefaults() = clearTheStore()

    @After
    fun leaveTheDefaults() = clearTheStore()

    /**
     * A store nobody has written answers `true` and `false`, in that order.
     *
     * Both defaults are load-bearing and in opposite directions. A fresh
     * install must behave as the app did before these preferences existed, plus
     * the new note: the Receipt drawn, the band shown. A store that failed open
     * to `showRecalled = false` would disable the Recall Receipt on every fresh
     * install and look like nothing at all was wrong — the Entry still reaches
     * the clipboard, so the only symptom is silence.
     */
    @Test
    fun an_unwritten_store_reads_the_receipt_on_and_the_note_undismissed() {
        val values = runBlocking { UiPreferences(context).values.first() }
        Evidence.log(
            "defaults      = showRecalled=${values.showRecalled} " +
                "noteDismissed=${values.foregroundNoteDismissed}",
        )

        assertTrue(
            "a fresh install must draw the Recall Receipt; failing open to off is silent",
            values.showRecalled,
        )
        assertFalse(
            "a fresh install must be shown the foreground-only note at least once",
            values.foregroundNoteDismissed,
        )
    }

    /**
     * Turning the Receipt off reaches a file, and reaches a reader that did not
     * turn it off.
     *
     * The switch is on the Settings Screen and the value is read in two other
     * places — the state holder that folds it into `UiState`, and the Standing
     * Action path. Both of those construct their own [UiPreferences]. A write
     * that only the writing instance could see would leave the switch looking
     * like it worked and doing nothing anywhere else.
     */
    @Test
    fun turning_the_receipt_off_is_visible_to_an_instance_that_did_not_write_it() {
        runBlocking { UiPreferences(context).setShowRecalled(false) }

        // The durability half: the write is bytes on disk in the app's own
        // storage, which is what a restarted process has to read.
        val file = context.preferencesDataStoreFile("ui")
        Evidence.log("prefs file    = ${file.absolutePath} (${file.length()} bytes)")
        assertTrue(
            "the switch position is not in a file, so there is nothing for a restart to find: $file",
            file.isFile && file.length() > 0,
        )

        // The reader half: an instance built after the write, over the same
        // context, holding none of the writer's state.
        val reader = UiPreferences(context)
        val values = runBlocking { reader.values.first() }
        Evidence.log("reread        = showRecalled=${values.showRecalled} from a second UiPreferences")
        assertFalse(
            "a second UiPreferences did not see the switch go off; the Settings switch would " +
                "reset itself and the Standing Action would keep reporting Recalls",
            values.showRecalled,
        )

        // And back on again, so the value is being read rather than a `false`
        // being produced by something that only ever writes once.
        runBlocking { reader.setShowRecalled(true) }
        assertTrue(
            "the switch does not come back on; a one-way switch is not a switch",
            runBlocking { UiPreferences(context).values.first().showRecalled },
        )
    }

    /**
     * Dismissing the foreground note reaches a new reader, and nothing brings it
     * back.
     *
     * The dismissal is the one preference with no undo on any path a User can
     * take, on purpose: the note stays readable at full length on the Settings
     * Screen, so a dismissal is an acknowledgement rather than a hidden
     * disclosure. That makes an accidental un-dismissal the failure worth
     * defending against — the band would return on the History Screen after the
     * User closed it for good, which is exactly the nagging the persisted flag
     * exists to stop. Writing the *other* preference is the realistic way that
     * would happen, so that is what this drives.
     */
    @Test
    fun dismissing_the_foreground_note_is_visible_to_a_new_instance_and_is_one_way() {
        runBlocking { UiPreferences(context).dismissForegroundNote() }

        val afterDismissal = runBlocking { UiPreferences(context).values.first() }
        Evidence.log(
            "dismissed     = noteDismissed=${afterDismissal.foregroundNoteDismissed} " +
                "from a second UiPreferences",
        )
        assertTrue(
            "a second UiPreferences did not see the dismissal; the band would come back on the " +
                "next launch and the flag would be persisting nothing",
            afterDismissal.foregroundNoteDismissed,
        )

        // An unrelated write through an unrelated instance, twice over, and a
        // repeat of the dismissal itself: none of them is an un-dismiss.
        runBlocking {
            UiPreferences(context).setShowRecalled(false)
            UiPreferences(context).setShowRecalled(true)
            UiPreferences(context).dismissForegroundNote()
        }

        val afterOtherWrites = runBlocking { UiPreferences(context).values.first() }
        Evidence.log(
            "still         = noteDismissed=${afterOtherWrites.foregroundNoteDismissed} " +
                "after writing the switch twice",
        )
        assertTrue(
            "the dismissal was undone by writing the other preference; the two keys share one " +
                "file and must not share a value",
            afterOtherWrites.foregroundNoteDismissed,
        )
        assertTrue(
            "and the switch it was written beside must be readable at its own last value",
            afterOtherWrites.showRecalled,
        )
    }

    /**
     * `showRecalledNow()` never disagrees with the flow, in either position.
     *
     * These are the two ways the same preference is read and they serve the two
     * halves of one feature: the flow feeds `UiState` for the open app, and
     * `showRecalledNow()` is the single read a Standing Action makes on a closed
     * phone, where there is no state holder to ask. A disagreement would not
     * look like a bug in either one — it would look like the switch silencing
     * the in-app Receipt and leaving the notification's alone, or the reverse,
     * which is precisely the half-applied setting the User cannot diagnose.
     */
    @Test
    fun show_recalled_now_agrees_with_the_flow_in_both_positions() {
        runBlocking {
            val on = UiPreferences(context)
            assertEquals(
                "the two reads disagree with the Receipt on",
                on.values.first().showRecalled,
                on.showRecalledNow(),
            )
            assertTrue("and the agreed value must be the default, `true`", on.showRecalledNow())

            UiPreferences(context).setShowRecalled(false)

            // A fresh instance for the off position, so this is also the
            // closed-phone case: a Standing Action reads a switch it never saw
            // set.
            val off = UiPreferences(context)
            assertEquals(
                "the two reads disagree with the Receipt off; the switch would silence one path only",
                off.values.first().showRecalled,
                off.showRecalledNow(),
            )
            assertFalse("and the agreed value must be `false`", off.showRecalledNow())
            Evidence.log("agreement     = showRecalledNow() matched values.first() at true and at false")
        }
    }

    /**
     * Puts both keys back to absent, which is what the defaults are made of.
     *
     * `UiPreferences.resetToDefaults` is `@VisibleForTesting` and carries its
     * own reasons: there is one DataStore per file and a second over the same
     * path is refused, so a test cannot open its own way in, and deleting the
     * file behind the live one only leaves its cache ahead of the disk. It is
     * also not an un-dismiss in disguise — nothing on a User's path reaches it,
     * and the clear it performs is the whole store rather than one key.
     */
    private fun clearTheStore() {
        runBlocking { UiPreferences(context).resetToDefaults() }
    }
}
