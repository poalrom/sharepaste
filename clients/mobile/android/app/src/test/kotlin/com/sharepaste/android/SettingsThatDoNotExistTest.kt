package com.sharepaste.android

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
 * the runtime half — no switch on the settings surface, no biometric API on the
 * classpath.
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
     * One cipher is named in this product, and it is the one it uses.
     *
     * ADR 0002 puts the disclosure beside pairing. `core/crypto.rs` seals with
     * XChaCha20-Poly1305; the mock this product was drawn from carried an
     * `AES-256-GCM` badge, and the desktop's own test asserts `AES` appears
     * nowhere for exactly this reason.
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
