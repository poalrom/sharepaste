package com.sharepaste.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * The shipped app's transport policy has nowhere to go wrong, and this reads the
 * file that makes that true.
 *
 * Every instrumented test in this module opens a facade with
 * `requireHttps = false` — the test Relay is plain HTTP and there is no publicly
 * trusted certificate to put in front of it from inside an emulator, and the
 * Android contract permits the concession. Ticket 12 needed one more: its
 * Standing Actions run with the app **not running**, so no instrumentation is
 * attached and the only facade they can use is `SharepasteApplication`'s own.
 *
 * The concession therefore had to reach production code, and the shape it took
 * is a **per-variant source file**: `app/src/debug` may consult a marker file,
 * `app/src/release` may not, and which one is compiled is a fact about the
 * artifact rather than a branch inside it. That is strictly stronger than a
 * runtime flag guarded by `BuildConfig.DEBUG`, because a branch is something a
 * later author can be persuaded to reach and an absent file is not.
 *
 * A source-text assertion, deliberately. What is being protected is the *shape*
 * of the release implementation — no branch, no input, one expression — and a
 * behavioural test could not tell "returns true because the constant is true"
 * from "returns true because the marker happens to be missing". This trips on any
 * edit that adds a decision, which is exactly the edit worth tripping on.
 *
 * `TransportPolicyTest` on the emulator covers the other half: that the app's own
 * facade, opened by the app's own code, really does refuse a cleartext Relay.
 */
class ShippedTransportPolicyTest {

    private val release: String by lazy {
        read("src/release/kotlin/com/sharepaste/android/OpenTheFacade.kt")
    }

    private val debug: String by lazy {
        read("src/debug/kotlin/com/sharepaste/android/OpenTheFacade.kt")
    }

    @Test
    fun the_release_policy_is_one_expression_that_reads_nothing() {
        val body = codeOf(release)
        assertEquals(
            "the release facade must be exactly one expression-bodied function; found:\n$body",
            1,
            Regex("""^internal fun openTheFacade\(""", RegexOption.MULTILINE).findAll(body).count(),
        )
        assertTrue(
            "the release facade must hand BuildConfig.REQUIRE_HTTPS straight to the core:\n$body",
            body.contains("requireHttps = BuildConfig.REQUIRE_HTTPS"),
        )
        // Every way a decision could get in. Any of these appearing means the
        // shipped policy has grown something that can answer differently on two
        // runs of the same binary, which is the whole thing this file forbids.
        listOf(
            " if ", "if(", "when", "?:", "&&", "||", "!",
            "File(", "getBoolean", "getProperty", "getenv", "SystemProperties",
        ).forEach { decision ->
            assertTrue(
                "the release facade contains \"$decision\". It must be one expression with no " +
                    "branch and no input — the debug source set is where a concession goes:\n$body",
                !body.contains(decision),
            )
        }
    }

    /**
     * The debug relaxation says so out loud.
     *
     * A security relaxation that logs nothing is how a debug affordance quietly
     * becomes load-bearing: somebody notices cleartext working, cannot find why,
     * and concludes the policy was never real.
     */
    @Test
    fun the_debug_relaxation_is_loud() {
        val body = codeOf(debug)
        assertTrue(
            "the debug facade must warn when the marker takes effect:\n$body",
            body.contains("Log.w("),
        )
        assertTrue(
            "the debug facade must still consult BuildConfig.REQUIRE_HTTPS:\n$body",
            body.contains("BuildConfig.REQUIRE_HTTPS"),
        )
    }

    /** The code, with the comments taken out, so a comment cannot fail a test. */
    private fun codeOf(source: String): String = source
        .replace(Regex("""/\*.*?\*/""", RegexOption.DOT_MATCHES_ALL), "")
        .replace(Regex("""//.*"""), "")
        .lines()
        .filter { it.isNotBlank() }
        .joinToString("\n")

    private fun read(path: String): String {
        val file = File(path)
        require(file.isFile) { "no file at ${file.absolutePath}" }
        return file.readText()
    }
}
