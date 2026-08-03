package com.sharepaste.android

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.platform.AndroidKeychain
import com.sharepaste.android.ui.SessionPhase
import com.sharepaste.android.ui.entryDeleteTag
import com.sharepaste.android.ui.entryRecallTag
import com.sharepaste.android.ui.entryRowTag
import com.sharepaste.android.ui.entryUndecryptableTag
import com.sharepaste.core.AppException
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * An Undecryptable Entry is marked, cannot be Recalled, and can still be deleted.
 *
 * Manufactured the way the core's own `ingest_aad_mismatch_marks_undecryptable`
 * does — an Entry sealed with a **different user key** — but end to end, through
 * the Relay and the phone's real keychain: the other device puts an Entry on the
 * Relay, this phone's stored user key is replaced with another one, and only then
 * does the session come up and try to decrypt it. The ciphertext is real, the
 * failure is real, and the row that results is the row a person would see if their
 * Pairing had been re-keyed underneath them.
 *
 * The key is replaced through `AndroidKeychain` rather than by reaching into the
 * database, because the keychain is a documented seam Kotlin already owns and
 * because a hand-written cache row would prove nothing about decryption.
 */
@RunWith(AndroidJUnit4::class)
class UndecryptableEntryTest {

    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val resources = context.resources

    private lateinit var phone: PhoneUnderTest

    @Before
    fun open() {
        phone = PhoneUnderTest.open(compose, DATABASE)
        phone.pairWithCode(Inviter.shared(), "undecryptable test phone")
    }

    @After
    fun close() = phone.close()

    @Test
    fun an_entry_sealed_with_another_key_is_marked_not_recallable_and_still_deletable() {
        val userId = phone.userId!!
        // Re-keyed *before* the Entry exists, and while this phone has no session:
        // 64 hex characters is a structurally valid user key, and it is not this
        // User's. Nothing here has decrypted this Entry with the right key first,
        // which is the whole of the manufacture.
        AndroidKeychain(context).put("$userId:key", ANOTHER_USER_KEY)
        Evidence.log("re-keyed      = $userId:key replaced with a different 32-byte key")

        val sealedText = "sealed-under-another-key-${System.currentTimeMillis()}"
        val sealedId = Inviter.shared().offerAndWaitForUpload(sealedText)
        Evidence.log("other device  = put Entry id=$sealedId on the Relay under the real key")

        phone.enterForeground()
        // Not by `sealedId`: that is the other device's own id for the row, and an
        // id stopped crossing the Relay with the Entry when one became local to the
        // device that made it (ADR 0016). Not by Preview either — this phone cannot
        // read one. What identifies it here is that it is the Entry this phone
        // cannot decrypt, which is also the fact under test.
        val sealedEntry = phone.awaitEntry("the re-keyed Entry must be backfilled") {
            it.undecryptable
        }
        // The flag is the facade's answer and not this test's inference, so the
        // Preview being empty has to be a *consequence* of it rather than the
        // evidence for it: an Entry whose plaintext is genuinely empty would look
        // the same from here, and only the core can tell the two apart.
        assertEquals(
            "the sealed Entry is the one this phone cannot read, and the only one",
            1,
            phone.state.entries.count { it.undecryptable },
        )
        assertEquals(
            "and it hands over no Preview, because there is none to hand over",
            "",
            sealedEntry.preview,
        )
        Evidence.log(
            "undecryptable = id=${sealedEntry.id} preview=\"${sealedEntry.preview}\" " +
                "flag=${sealedEntry.undecryptable} phase=${phone.state.session}",
        )
        assertNull(
            "there is no plaintext to read for an Entry this device holds no key for",
            runBlocking { phone.repo.readEntry(userId, sealedEntry.id) },
        )

        // Marked, and the Recall refused before it is pressed.
        phone.scrollTo(entryRowTag(sealedEntry.id))
        compose.onNodeWithTag(entryUndecryptableTag(sealedEntry.id))
            .assertTextEquals(resources.getString(R.string.entry_undecryptable))
        compose.onNodeWithTag(entryRecallTag(sealedEntry.id)).assertIsNotEnabled()

        // And refused underneath as well: the disabled control is the first line of
        // defence, not the only one, since ticket 12 will invoke Recall from
        // outside any screen.
        try {
            runBlocking { phone.repo.recall(userId, sealedEntry.id) }
            fail("Recalling an Undecryptable Entry must fail rather than paste something else")
        } catch (e: AppException.NotFound) {
            Evidence.log("recall refused= detail=${e.detail}")
        }

        // Still deletable. Ciphertext this phone cannot read is the row a person
        // most wants gone, and deleting is the only thing they can do with it.
        compose.onNodeWithTag(entryDeleteTag(sealedEntry.id)).assertIsEnabled()
        compose.onNodeWithTag(entryDeleteTag(sealedEntry.id)).performClick()
        phone.await("the deleted Entry must leave the list") { state ->
            state.entries.none { it.id == sealedEntry.id }
        }
        compose.onNodeWithTag(entryRowTag(sealedEntry.id)).assertDoesNotExist()
        Evidence.log("deleted       = id=${sealedEntry.id} gone from the Relay and the cache")
    }

    private companion object {
        const val DATABASE = "undecryptable-proof.db"

        /**
         * A structurally valid user key that is not this User's.
         *
         * 64 hex characters, so `decode_user_key` accepts it and the failure lands
         * where it should — in the AEAD, on this User's own ciphertext — rather
         * than as a keychain error that would prove nothing.
         */
        const val ANOTHER_USER_KEY = "5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c"
    }
}
