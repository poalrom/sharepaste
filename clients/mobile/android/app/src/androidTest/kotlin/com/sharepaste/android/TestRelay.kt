package com.sharepaste.android

import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.core.AppException
import com.sharepaste.core.Sharepaste
import java.net.InetSocketAddress
import java.net.Socket

/**
 * The relay `docker compose` serves on the host, as seen from inside the AVD.
 *
 * The relay is plain HTTP, which is exactly why every facade a test opens against
 * it passes `requireHttps = false`. The shipped app passes `true` — see
 * [TransportPolicyTest], which is the test that exists so this concession stays a
 * concession rather than becoming the configuration.
 */
object TestRelay {

    /** The host's loopback, as seen from inside the emulator. */
    private const val DEFAULT_URL = "http://10.0.2.2:8443"

    private val arguments = InstrumentationRegistry.getArguments()

    val url: String = arguments.getString("relayUrl") ?: DEFAULT_URL

    /**
     * The invite tokens the host created for this run, handed out one at a time.
     *
     * An invite is **single use** — the relay answers a second claim with `409
     * Conflict` — so a run needs one per test that claims one, and a comma-
     * separated list is how the host supplies them:
     *
     * ```
     * -Pandroid.testInstrumentationRunnerArguments.inviteToken=$T1,$T2
     * ```
     */
    private val invites: ArrayDeque<String> = ArrayDeque(
        (arguments.getString("inviteToken") ?: "")
            .split(',')
            .map { it.trim() }
            .filter { it.isNotEmpty() },
    )

    /** Take the next unused invite, or fail saying how to make one. */
    fun nextInvite(): String = synchronized(invites) {
        invites.removeFirstOrNull() ?: error(
            "out of invite tokens. Each is single-use; create one per claiming test with " +
                "`docker exec sharepaste node /app/dist/index.js user create <name>` and pass them " +
                "comma-separated as " +
                "-Pandroid.testInstrumentationRunnerArguments.inviteToken=<t1>,<t2>.",
        )
    }

    /**
     * A plain TCP connect before the protocol is asked to do anything.
     *
     * Without it, a relay that is simply not running looks exactly like a broken
     * FFI boundary. It also catches the AVD's own trap: `spike35` brings up
     * `wlan0` with no IPv4 default route and prefers it over `eth0`, so every
     * IPv4 connect fails `ENETUNREACH` while `ip route` looks perfectly healthy.
     * `adb shell cmd wifi set-wifi-enabled disabled` fixes it and does not
     * survive a cold boot.
     */
    fun assertReachable() {
        val authority = url.substringAfter("://")
        val host = authority.substringBefore(':')
        val port = authority.substringAfter(':', "80").substringBefore('/').toInt()
        try {
            Socket().use { it.connect(InetSocketAddress(host, port), 5_000) }
        } catch (e: Exception) {
            throw AssertionError(
                "the relay at $url is not reachable from this emulator (${e.message}). Start it " +
                    "on the host, and run `adb shell cmd wifi set-wifi-enabled disabled`.",
                e,
            )
        }
    }
}

/**
 * The device on the other end of the pairing.
 *
 * A short code is minted by a device that is *already* paired, so proving this
 * phone can claim one needs a second device to exist. This is that device: it
 * claims an invite, mints codes, and can offer an Entry while the phone under test
 * is "backgrounded".
 *
 * In-memory, because none of it needs to survive the run — but a real facade
 * against the real relay, because a stub inviter would prove nothing about the
 * bytes in a code.
 *
 * **One per run**, via [shared]. A User can mint any number of short codes, so a
 * second inviting device buys nothing and costs a single-use invite token per test
 * — which is how the first run of this suite failed, with a `409 Conflict` from
 * the relay on the second claim.
 */
class Inviter private constructor(
    private val core: Sharepaste,
    val userId: String,
) {

    /**
     * A fresh short code, in the **compact** form the desktop's QR carries:
     * whitespace and dashes stripped, upper case.
     *
     * The core hands back the grouped-for-reading form, which is what a person
     * types. The desktop's pairing pane strips it before encoding the square, so
     * that is the string a scan actually produces — and the string this returns.
     *
     * Deliberately not logged. For the next two minutes it *is* the pairing secret.
     */
    fun freshCompactCode(): String =
        core.pairStart(userId).code.filterNot { it == '-' || it.isWhitespace() }.uppercase()

    /** Put an Entry on the relay, as the other device would. */
    fun offer(text: String) {
        core.startSession(userId)
        core.offer(userId, text)
    }

    /**
     * The newest Entry this User has on the relay, decrypted by *this* device.
     *
     * `recall_latest` is what reads it, because `recall_latest` always performs
     * the round trip: the answer is what the relay holds now, not what this
     * facade happened to have cached. That is what makes it usable as the
     * assertion for "the phone's Offer reached the relay and another device can
     * read it" — one call covers both halves, and a stale cache cannot fake it.
     *
     * Throws `AppException.NotFound` while the User has no Entries at all.
     */
    fun newestOnRelay(): String = core.recallLatest(userId).text

    /**
     * Offer, and do not return until the relay actually has it.
     *
     * The uploader is asynchronous, so an offer that has only been *queued* proves
     * nothing about what another device can see. Every test that hands this device
     * something for the phone to find needs the Entry to be on the relay before the
     * phone is allowed to look, or the phone finding nothing would look like a
     * broken backfill.
     *
     * Hands back the relay's id for it, which is the only handle on an Entry whose
     * Preview a test cannot predict — an Undecryptable one has none.
     */
    fun offerAndWaitForUpload(text: String): Long {
        offer(text)
        val deadline = System.nanoTime() + UPLOAD_TIMEOUT_SECONDS * 1_000_000_000L
        while (System.nanoTime() < deadline) {
            val newest = try {
                core.recallLatest(userId)
            } catch (e: AppException) {
                null
            }
            // Only `entryId` is read. `Recalled` carries the plaintext and has a
            // generated `toString`; the core deliberately gives its Rust
            // equivalent no `Debug`, so the rule has to be kept by hand here.
            if (newest?.text == text) return newest.entryId
            Thread.sleep(200)
        }
        throw AssertionError(
            "the other device's Entry never reached the relay in ${UPLOAD_TIMEOUT_SECONDS}s",
        )
    }

    companion object {
        @Volatile
        private var instance: Inviter? = null

        @Volatile
        private var secondInstance: Inviter? = null

        /**
         * How long the other device's uploader is given.
         *
         * Generous: it covers a session coming up, an encrypt and a POST over the
         * emulator's loopback, and a slow one here would fail a test about the
         * *phone* for a reason that has nothing to do with the phone.
         */
        private const val UPLOAD_TIMEOUT_SECONDS = 30L

        /**
         * The run's one inviting device, claimed on first use.
         *
         * Never closed: it dies with the instrumentation process, and keeping its
         * session live between tests is what lets one test offer an Entry that
         * another test's phone backfills.
         */
        fun shared(): Inviter = synchronized(this) {
            instance ?: claim().also { instance = it }
        }

        /**
         * A **second** User, for the tests that need this phone to hold two
         * Pairings at once.
         *
         * A Pairing binds this machine to one User on one Relay, so two Pairings
         * means two Users: pairing twice against [shared] would give one Pairing
         * two Devices, which is a different thing entirely and proves nothing
         * about an Active Pairing.
         *
         * One per run, for the same reason [shared] is. A User can mint any
         * number of short codes, so every test that wants a second Pairing can
         * claim one from this device for free — where a second *inviter* per test
         * would cost a single-use invite token each time.
         */
        fun second(): Inviter = synchronized(this) {
            secondInstance ?: claim("the second User in the test").also { secondInstance = it }
        }

        private fun claim(label: String = "inviting side of the test"): Inviter {
            TestRelay.assertReachable()
            val core = Sharepaste.openInMemory(
                keychain = InMemoryKeychain(),
                clipboard = NoClipboard,
                events = SilentSink,
                requireHttps = false,
            )
            val paired = core.pairWithInvite(TestRelay.url, TestRelay.nextInvite(), label)
            core.setActivePairing(paired.userId)
            return Inviter(core, paired.userId)
        }
    }
}
