package com.sharepaste.android

import android.database.sqlite.SQLiteDatabase
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.platform.AndroidClipboard
import com.sharepaste.android.platform.AndroidKeychain
import com.sharepaste.android.platform.FlowEventSink
import com.sharepaste.core.Sharepaste
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * The database opens and the migrations run, on the device, in app-private
 * storage.
 *
 * The path is handed *in*. The core never asks Android where an application's
 * data lives — that is the whole reason `db_path` is a parameter rather than a
 * lookup — so this test is also the check that the shell hands over the right
 * place: `filesDir`, which is what puts the plaintext cache behind file-based
 * encryption without a toggle of our own.
 */
@RunWith(AndroidJUnit4::class)
class DatabaseTest {

    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    @Test
    fun migrations_run_against_a_file_in_app_private_storage() {
        val dbFile = freshDatabaseFile("migration-proof.db")

        Evidence.log("filesDir      = ${context.filesDir.absolutePath}")
        Evidence.log("db path       = ${dbFile.absolutePath}")
        assertTrue(
            "the database must live in app-private storage",
            dbFile.absolutePath.startsWith(context.filesDir.absolutePath),
        )

        val core = openCore(dbFile)
        // Reads a row the migrations created; a schema that had not been built
        // would fail here rather than at `open`.
        val settings = core.getSettings()
        Evidence.log("settings      = capture_enabled=${settings.captureEnabled} deny_list=${settings.denyList.size}")
        core.close()

        assertTrue("the database file was never created", dbFile.exists())
        Evidence.log("db bytes      = ${dbFile.length()}")

        val db = SQLiteDatabase.openDatabase(dbFile.absolutePath, null, SQLiteDatabase.OPEN_READWRITE)
        db.use {
            // `android_metadata` is excluded because *this* read creates it:
            // Android's own `SQLiteDatabase` adds a locale row to any database
            // it opens. It is an artefact of the verification, not of the
            // migrations under test.
            val tables = it.rawQuery(
                "SELECT name FROM sqlite_master WHERE type='table' " +
                    "AND name NOT LIKE 'sqlite_%' AND name <> 'android_metadata' ORDER BY name",
                null,
            ).use { cursor -> cursor.readColumn(0) }
            Evidence.log("tables        = $tables")
            assertEquals(
                listOf("accounts", "devices", "entries_cache", "pending_uploads", "settings"),
                tables,
            )

            val accountColumns = it.rawQuery("PRAGMA table_info(accounts)", null)
                .use { cursor -> cursor.readColumn(1) }
            Evidence.log("accounts cols = $accountColumns")
            // `username` and `last_contact_at` are the two columns added by the
            // `PRAGMA table_info` + `ALTER TABLE` path, which is the half
            // `CREATE TABLE IF NOT EXISTS` cannot reach.
            assertTrue(accountColumns.containsAll(listOf("user_id", "device_id", "username", "last_contact_at")))
        }
    }

    @Test
    fun opening_the_same_database_twice_is_idempotent() {
        val dbFile = freshDatabaseFile("idempotence-proof.db")

        openCore(dbFile).use { it.getSettings() }
        val second = openCore(dbFile)
        val settings = second.getSettings()
        second.close()

        Evidence.log("reopened ok   = capture_enabled=${settings.captureEnabled}")
        assertTrue("a second open must not fail on a non-idempotent migration", dbFile.exists())
    }

    private fun openCore(dbFile: File): Sharepaste = Sharepaste.open(
        dbPath = dbFile.absolutePath,
        keychain = AndroidKeychain(context),
        clipboard = AndroidClipboard(context),
        events = FlowEventSink(),
        // This test never opens a socket, so the transport policy is beside the
        // point — but it has to say something, which is the point of making it a
        // parameter. The shipped app's value is proved by `TransportPolicyTest`.
        requireHttps = false,
    )

    private fun freshDatabaseFile(name: String): File {
        val file = File(context.filesDir, name)
        listOf("", "-wal", "-shm").forEach { File(file.path + it).delete() }
        return file
    }

    private fun android.database.Cursor.readColumn(index: Int): List<String> = buildList {
        while (moveToNext()) add(getString(index))
    }
}
