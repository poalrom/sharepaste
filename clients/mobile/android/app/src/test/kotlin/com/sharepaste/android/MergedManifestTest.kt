package com.sharepaste.android

import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * The two leakage controls, read off the manifest AGP actually produced.
 *
 * Both default to **permissive**, which is what makes this test worth its weight:
 * a merge that drops either attribute ships the exposure, nothing at runtime
 * looks wrong, and no other test in this project would notice. The failure mode
 * is silent by construction, so the assertion has to be structural.
 *
 * It reads the *merged* manifest rather than the source one because that is what
 * ships — a library manifest or a later flavour could add
 * `android:allowBackup="true"` and win the merge, and the file in `src/main`
 * would still look correct.
 *
 * A JVM test rather than an instrumented one, deliberately: the merged manifest
 * exists only under `build/`, and a device never sees it. `ApplicationInfo` on a
 * device exposes the backup flag but not the data-extraction rules, so half of
 * this criterion is not reachable from there at all.
 * `LeakageControlsTest.backup_is_denied_on_the_installed_app` covers the half
 * that is, on the real install.
 *
 * The build file hands over the path — see `mergedManifestForUnitTests`.
 */
class MergedManifestTest {

    private val manifest: String by lazy {
        val path = requireNotNull(System.getProperty("sharepaste.mergedManifest")) {
            "the merged manifest path was not passed in. The build file wires it from " +
                "SingleArtifact.MERGED_MANIFEST; run this through Gradle, not standalone."
        }
        val file = File(path)
        require(file.isFile) { "no merged manifest at $path" }
        file.readText()
    }

    @Test
    fun cloud_backup_is_denied() {
        // Off, so no copy of the plaintext cache or the keystore-wrapped user key
        // is handed to Google's backup transport.
        assertTrue(
            "android:allowBackup must be false in the merged manifest, and is:\n$manifest",
            manifest.contains(Regex("""android:allowBackup="false"""")),
        )
    }

    @Test
    fun data_extraction_rules_are_declared_and_deny_both_transports() {
        assertTrue(
            "android:dataExtractionRules must point at the rules resource; merged manifest:\n$manifest",
            manifest.contains(Regex("""android:dataExtractionRules="@[^"]*data_extraction_rules"""")),
        )

        // `allowBackup="false"` alone leaves device-to-device transfer permissive
        // on API 31+: they are configured separately and each defaults to on. So
        // the referenced resource has to deny both, not just exist.
        val rules = File("src/main/res/xml/data_extraction_rules.xml")
        require(rules.isFile) { "no data_extraction_rules.xml at ${rules.absolutePath}" }
        val text = rules.readText()
        listOf("cloud-backup", "device-transfer").forEach { section ->
            val body = text.substringAfter("<$section>", "").substringBefore("</$section>", "")
            assertTrue("<$section> is missing from the rules resource", body.isNotEmpty())
            // Every domain the framework can reach, not a subset: `root` does not
            // imply the others, and an unlisted domain is an included domain.
            listOf("root", "file", "database", "sharedpref", "external").forEach { domain ->
                assertTrue(
                    "<$section> does not exclude the $domain domain",
                    body.contains(Regex("""<exclude\s+domain="$domain"\s*/>""")),
                )
            }
        }
    }

    /**
     * The Standing Actions are **not** backed by a foreground service, and this
     * reads the manifest AGP produced rather than the one in `src/main`.
     *
     * Three independent reasons, spelled out in `StandingActions`: recent
     * Android caps a `dataSync` foreground service at six hours per
     * twenty-four; an ongoing notification is user-dismissible regardless; and
     * a foreground service confers no clipboard access anyway, because only
     * window focus does. Underneath them is ADR 0007 — a clipboard tool that
     * runs unattended is a clipboard tool that reads your clipboard unattended.
     *
     * Merged rather than source because that is the assertion worth having. A
     * library can contribute a `<service>` and its permissions with nothing
     * appearing in this project's own manifest at all, and the whole point of
     * ADR 0007 is that nothing in this app runs when the person is not looking
     * at it. If this fails, the answer is not to relax it.
     */
    @Test
    fun no_foreground_service_is_declared() {
        listOf(
            "android.permission.FOREGROUND_SERVICE",
            "android:foregroundServiceType",
        ).forEach { declaration ->
            assertTrue(
                "the merged manifest declares $declaration. There is no foreground service in " +
                    "this app — see StandingActions for the three reasons:\n$manifest",
                !manifest.contains(declaration),
            )
        }
        // Every `<service>` element, with its attributes, so both halves below
        // can be asked of the same text.
        val services = Regex("""<service\b[^>]*?/?>""", RegexOption.DOT_MATCHES_ALL)
            .findAll(manifest)
            .map { it.value }
            .toList()
        val ours = services.filter { it.contains("com.sharepaste.android") }
        assertTrue(
            "this app declares a service: $ours. It runs nothing while nobody is looking at it " +
                "(ADR 0007), and the Standing Actions are a notification, not a service:\n$manifest",
            ours.isEmpty(),
        )
        // CameraX contributes `MetadataHolderService`, which is a place to hang
        // a `meta-data` tag and is declared `enabled="false"` — it is never
        // started and has no `onStartCommand`. Asserting "no service at all"
        // would fail on that and teach the next person to delete the test, so
        // the rule is the one that matters: nothing here can be *run*.
        val runnable = services.filterNot { it.contains("""android:enabled="false"""") }
        assertTrue(
            "the merged manifest carries a startable service: $runnable. A library contributing " +
                "one is exactly what this test is here to catch:\n$manifest",
            runnable.isEmpty(),
        )
    }

    /**
     * The one broadcast this app receives, and what it may not turn into.
     *
     * `BOOT_COMPLETED` exists so the Standing Actions come back after a reboot
     * without the app being opened — a notification survives nothing, so the one
     * thing this client offers someone who never opens it would otherwise be
     * gone until they did. It posts a notification and returns. Asserting the
     * permission is present *and* that no scheduling permission joined it is
     * what keeps "re-post a notification" from drifting into "do work while
     * nobody is looking".
     */
    @Test
    fun the_boot_receiver_schedules_nothing() {
        assertTrue(
            "the Standing Actions must come back after a reboot:\n$manifest",
            manifest.contains("android.permission.RECEIVE_BOOT_COMPLETED"),
        )
        listOf(
            "android.permission.WAKE_LOCK",
            "android.permission.SCHEDULE_EXACT_ALARM",
            "android.permission.USE_EXACT_ALARM",
            "android.permission.REQUEST_IGNORE_BATTERY_OPTIMIZATIONS",
        ).forEach { permission ->
            assertTrue(
                "the merged manifest requests $permission. Nothing here runs unattended:\n$manifest",
                !manifest.contains(permission),
            )
        }
    }

    /**
     * A second press reuses the window already open rather than stacking another.
     *
     * Both Standing Action windows are `singleTask` with an empty `taskAffinity`,
     * and that pair of attributes is the whole reason
     * `StandingActionActivity.onNewIntent` and `ShareTargetActivity.onNewIntent`
     * exist: `singleTask` means the platform delivers a second press's `Intent` to
     * the live instance instead of creating a second one, so an activity that does
     * not adopt it drops the verb in silence — which is what it did until this
     * ticket. The empty affinity is what keeps an invisible one-shot out of the
     * task of any Sharepaste window somebody left open.
     *
     * Asserted here so the two halves cannot drift apart. A `launchMode` relaxed
     * to the default would make both overrides dead code and break the second
     * press again, with nothing on screen looking wrong;
     * `StandingActionPressesTest.the_two_standing_action_activities_adopt_a_new_intent`
     * is the half that fails if an override goes away instead.
     */
    @Test
    fun the_standing_action_windows_are_reused_rather_than_stacked() {
        listOf(
            "com.sharepaste.android.standing.StandingActionActivity",
            "com.sharepaste.android.standing.ShareTargetActivity",
        ).forEach { name ->
            val declaration = Regex("""<activity\b[^>]*?\Q$name\E[^>]*?/?>""", RegexOption.DOT_MATCHES_ALL)
                .find(manifest)
                ?.value
            val declared = requireNotNull(declaration) {
                "the merged manifest declares no activity named $name:\n$manifest"
            }
            listOf("android:launchMode=\"singleTask\"", "android:taskAffinity=\"\"").forEach { attribute ->
                assertTrue(
                    "$name is declared without $attribute. `singleTask` is what makes a second " +
                        "press reuse the window already open, which is what its `onNewIntent` " +
                        "override is for; without it that override is dead code. Declared as: " +
                        declared,
                    declared.contains(attribute),
                )
            }
        }
    }

    /**
     * The launcher shows the product's mark, not the platform's robot.
     *
     * `android:icon` is optional. An application without one builds, installs
     * and runs, and the only symptom is the default green Android silhouette on
     * the home screen — which is what this app shipped with until it was
     * noticed by eye. Silent by construction, like everything else in this file.
     *
     * The layers are asserted as well as the attribute. An `<adaptive-icon>`
     * that lost its `background` draws the ribbons on black or on nothing
     * depending on the launcher, and one that lost its `monochrome` opts out of
     * themed icons on Android 13+ without failing anything.
     */
    @Test
    fun the_launcher_wears_the_products_mark() {
        val application = Regex("""<application\b[^>]*?>""", RegexOption.DOT_MATCHES_ALL)
            .find(manifest)
            ?.value
        val declared = requireNotNull(application) {
            "the merged manifest declares no <application> at all:\n$manifest"
        }
        assertTrue(
            "<application> carries no android:icon, so the launcher falls back to the " +
                "platform's default robot. Declared as: $declared",
            declared.contains("""android:icon="@mipmap/ic_launcher""""),
        )

        val icon = File("src/main/res/mipmap-anydpi-v26/ic_launcher.xml")
        require(icon.isFile) { "android:icon names a resource that is not there: ${icon.absolutePath}" }
        val layers = icon.readText()
        listOf("background", "foreground", "monochrome").forEach { layer ->
            assertTrue(
                "the adaptive icon declares no <$layer> layer",
                layers.contains(Regex("""<$layer\s+android:drawable="@[^"]+"\s*/>""")),
            )
        }
    }
}
