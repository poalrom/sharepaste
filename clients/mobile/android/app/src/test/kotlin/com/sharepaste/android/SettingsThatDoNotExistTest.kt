package com.sharepaste.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * Two settings this product does not have, pinned so a later author does not
 * helpfully add one.
 *
 * **There is no plaintext-at-rest toggle.** The cache stores plaintext
 * unconditionally, on both clients; the database lives in app-private storage,
 * which Android's file-based encryption covers. A switch would either lie about
 * what it does or do nothing, and the spec's mention of one is mistaken.
 *
 * **There is no biometric gate in this release.** Not an oversight — see the
 * Android contract's leakage controls, and spec rows 12 and 14.
 *
 * A JVM test, because the two things worth asserting are both build-time
 * artefacts: the words the app is capable of saying, and what the merged manifest
 * asks the platform for. A device sees neither as text.
 * `PairingsScreenTest.no_plaintext_toggle_and_no_biometric_gate_were_added` covers
 * the runtime half. It is a census rather than a count of zero: the Settings
 * Screen now has exactly two switches, `SHOW WHAT WAS RECALLED` and `CONFIRM
 * OFFERS`, which decide only whether Sharepaste speaks after a verb it performed
 * either way and touch no stored byte (ADR 0009, ADR 0018). Any other switch, and
 * any biometric API on the classpath, is a failure there.
 */
class SettingsThatDoNotExistTest {

    private val strings: String by lazy { read("src/main/res/values/strings.xml") }

    private val manifest: String by lazy {
        val path = requireNotNull(System.getProperty("sharepaste.mergedManifest")) {
            "the merged manifest path was not passed in. The build file wires it from " +
                "SingleArtifact.MERGED_MANIFEST; run this through Gradle, not standalone."
        }
        File(path).also { require(it.isFile) { "no merged manifest at $path" } }.readText()
    }

    /**
     * The app cannot offer a biometric gate, because it cannot say the words and
     * cannot ask for the permission.
     *
     * The permission half is the load-bearing one: `USE_BIOMETRIC` is what any
     * real gate needs, and a merged manifest is where a library would sneak one
     * in without a line appearing in `src/main/AndroidManifest.xml`.
     */
    @Test
    fun nothing_offers_a_biometric_gate() {
        listOf("biometric", "fingerprint", "face unlock", "unlock with your").forEach { word ->
            assertTrue(
                "strings.xml offers \"$word\". There is no biometric gate in this release.",
                !strings.contains(word, ignoreCase = true),
            )
        }
        listOf("USE_BIOMETRIC", "USE_FINGERPRINT").forEach { permission ->
            assertTrue(
                "the merged manifest requests $permission:\n$manifest",
                !manifest.contains(permission),
            )
        }
        val build = read("build.gradle.kts") + read("../gradle/libs.versions.toml")
        assertTrue(
            "androidx.biometric is declared. Adding it is the change that needs justifying.",
            !build.contains("biometric", ignoreCase = true),
        )
    }

    /**
     * Nothing offers to switch encryption-at-rest on or off.
     *
     * The vocabulary is what a control would have to be described in. There is
     * no such control, so none of these words has any business in the app's own
     * strings — and if one ever does, it is because somebody built the setting.
     */
    @Test
    fun nothing_offers_a_plaintext_at_rest_toggle() {
        listOf(
            "plaintext at rest",
            "encrypt this phone",
            "store entries encrypted",
            "encrypt the cache",
        ).forEach { phrase ->
            assertTrue(
                "strings.xml offers \"$phrase\". The cache stores plaintext unconditionally; " +
                    "a toggle would either lie or do nothing.",
                !strings.contains(phrase, ignoreCase = true),
            )
        }
    }

    /**
     * The Device Label rule is stated once, where it can still be acted on.
     *
     * It used to be a quoted note on the settings surface, which is the one
     * place a person can do nothing about it: the phone is already paired and
     * the name is already fixed. It is now the second sentence of
     * `pair_label_explainer`, beside the field where a name is being chosen.
     *
     * Asserted here rather than on the screen, because the screen can only prove
     * the note is not drawn. This proves the app **cannot say it** — the string
     * is gone, so no composable can be wired back to it by accident.
     *
     * Matched against the file with its runs of whitespace collapsed, because a
     * sentence in `strings.xml` wraps across source lines and the resource
     * loader joins it back up. Matching the raw text would make this a test of
     * where somebody put a line break.
     */
    @Test
    fun the_device_label_rule_is_stated_beside_the_field_and_nowhere_else() {
        val said = strings.replace(Regex("\\s+"), " ")
        assertTrue(
            "strings.xml still carries the settings-screen Device Label note. Its one fact " +
                "belongs in pair_label_explainer, where a name is still being chosen.",
            !said.contains("This phone told the Relay its name"),
        )
        assertEquals(
            "the rule that a Device Label cannot be changed after pairing is stated in " +
                "pair_label_explainer, and exactly once in the whole file",
            1,
            Regex("It cannot be changed later").findAll(said).count(),
        )
    }

    /**
     * One cipher is named in this product, and it is the one it uses.
     *
     * `core/crypto.rs` seals with XChaCha20-Poly1305; the mock this product was
     * drawn from carried an `AES-256-GCM` badge, and the desktop's own test
     * asserts `AES` appears nowhere for exactly this reason.
     *
     * ADR 0002 wanted the disclosure beside pairing. On this phone it is no
     * longer there — the pairing flow's footer band went with the rest of its
     * inert facts — so what this pins is that the word survives *somewhere*, on
     * the Settings Screen's Pairing card. That is the weaker placement ADR 0002
     * now records, and it is still the difference between disclosing the cipher
     * and disclosing nothing.
     */
    @Test
    fun the_only_cipher_named_anywhere_is_the_one_this_product_seals_with() {
        assertTrue(
            "strings.xml names AES. This product seals with XChaCha20-Poly1305.",
            !strings.contains("AES", ignoreCase = true),
        )
        assertTrue(
            "the cipher disclosure ADR 0002 asks for is missing from strings.xml",
            strings.contains("XCHACHA20-POLY1305"),
        )
    }

    private fun read(path: String): String {
        val file = File(path)
        require(file.isFile) { "no file at ${file.absolutePath}" }
        return file.readText()
    }
}
