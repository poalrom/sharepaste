package com.sharepaste.android

import android.content.pm.ApplicationInfo
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The leakage controls, read off the app as installed.
 *
 * Backup and data extraction both default to **permissive**, so omitting either
 * ships the exposure and nothing at runtime looks wrong. `MergedManifestTest`
 * holds both attributes against the manifest AGP produced; this holds the backup
 * flag against the app the platform actually installed, which is a stronger
 * statement about the same thing and catches a manifest that merged correctly and
 * then lost the attribute somewhere between there and the APK.
 *
 * The data-extraction rules are not reachable from here: `ApplicationInfo` exposes
 * the backup flag and no accessor for `dataExtractionRules`, which is why that
 * half is asserted against the merged manifest instead of guessed at through
 * reflection on a hidden field.
 */
@RunWith(AndroidJUnit4::class)
class LeakageControlsTest {

    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    @Test
    fun backup_is_denied_on_the_installed_app() {
        val info = context.applicationInfo
        val allowsBackup = info.flags and ApplicationInfo.FLAG_ALLOW_BACKUP
        Evidence.log(
            "backup flag   = FLAG_ALLOW_BACKUP=$allowsBackup for ${context.packageName} " +
                "(0 means denied)",
        )
        assertEquals(
            "FLAG_ALLOW_BACKUP is set on the installed app: the plaintext cache and the " +
                "keystore-wrapped user key would be handed to the backup transport",
            0,
            allowsBackup,
        )
    }

    /**
     * The rules resource is present and reachable from the installed app.
     *
     * Not proof that the manifest points at it — that is
     * `MergedManifestTest.data_extraction_rules_are_declared_and_deny_both_transports` —
     * but proof that a resource-shrinking release build did not drop the thing the
     * manifest points at, which would leave a dangling reference the framework
     * silently treats as absent.
     */
    @Test
    fun the_data_extraction_rules_resource_survives_into_the_apk() {
        val id = context.resources.getIdentifier("data_extraction_rules", "xml", context.packageName)
        assertTrue("no @xml/data_extraction_rules in the installed app", id != 0)
        context.resources.getXml(id).use { parser ->
            val tags = generateSequence { if (parser.next() == org.xmlpull.v1.XmlPullParser.END_DOCUMENT) null else parser }
                .filter { it.eventType == org.xmlpull.v1.XmlPullParser.START_TAG }
                .map { it.name }
                .toList()
            Evidence.log("extraction    = tags=${tags.distinct()}")
            assertTrue("the rules do not deny cloud backup", tags.contains("cloud-backup"))
            assertTrue("the rules do not deny device transfer", tags.contains("device-transfer"))
            assertTrue("the rules exclude nothing", tags.contains("exclude"))
        }
    }
}
