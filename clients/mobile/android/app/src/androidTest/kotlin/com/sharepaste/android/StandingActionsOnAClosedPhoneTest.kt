package com.sharepaste.android

import android.app.NotificationManager
import android.content.Intent
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.android.standing.StandingActions
import com.sharepaste.core.Sharepaste
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Both verbs, with the app **not running**.
 *
 * This class cannot be run by `connectedDebugAndroidTest`. Its halves are
 * separate `am instrument` invocations with `adb shell am force-stop` between
 * them, because "the app is not running" is precisely the state an
 * instrumentation destroys by existing. The exact sequence, and the output it
 * produced, is in `.scratch/mobile-client/issues/12-android-standing-actions.md`.
 *
 * ## How a Standing Action is pressed with nothing to press it with
 *
 * The host fires `StandingActions.intentFor(context, action)` with `am start`.
 * That is equivalent to a tap, and the equivalence is asserted rather than
 * assumed: `StandingActionsNotificationTest` rebuilds each action's
 * `PendingIntent` from that same function with `FLAG_NO_CREATE` and proves the
 * posted notification is holding it. Two `PendingIntent`s are one object when
 * their package, request code and `Intent` match, so a tap and this `am start`
 * ask ActivityManager to start the same component with the same action. The
 * `shell` user holds `START_ANY_ACTIVITY`, which is what lets it reach an
 * activity this app deliberately does not export.
 *
 * ## Why the app's own facade, and what that cost
 *
 * With the app not running there is no instrumentation, so a Standing Action
 * necessarily uses `SharepasteApplication.repository` — it cannot be handed a
 * test facade the way every other test here is. That facade's transport policy
 * is `BuildConfig.REQUIRE_HTTPS = true`, and the test Relay is plain HTTP with
 * no publicly trusted certificate available from inside an emulator. So each
 * method below runs inside [relaxed], which plants the debug-only marker
 * `app/src/debug/.../OpenTheFacade.kt` reads — **and deletes it in a `finally`,
 * pass or fail**, so a method that dies midway cannot leave an emulator relaxed
 * for every later run on it. The release source set has no branch that could
 * read the marker at all; `ShippedTransportPolicyTest` reads the file and says
 * so.
 *
 * The compose rule is here for window focus rather than for a screen. A
 * clipboard **read** needs it, and half of what these methods do is check what
 * the clipboard now holds.
 */
@RunWith(AndroidJUnit4::class)
class StandingActionsOnAClosedPhoneTest {

    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    private val clip = Clip(context)

    /**
     * Every method here skips unless the host says it is driving the sequence.
     *
     * `connectedDebugAndroidTest` runs a class's methods back to back in one
     * process, and these methods are separated by a force-stop and an `am start`
     * that only the host can perform. Run in one process they would assert on
     * Standing Actions that never fired, and fail for a reason that has nothing
     * to do with the app. An assumption rather than `@Ignore`, because `@Ignore`
     * would also skip them when the host *does* drive them.
     */
    @Before
    fun onlyWhenTheHostIsDriving() {
        assumeTrue(
            "this class is a host-driven sequence: each method is a separate `am instrument` " +
                "with `am force-stop` between. Pass -e closedPhone true. The whole sequence is " +
                "in .scratch/mobile-client/issues/12-android-standing-actions.md.",
            InstrumentationRegistry.getArguments().getString("closedPhone") == "true",
        )
    }

    /**
     * Pair the app's own facade, and leave something on the clipboard for a
     * Standing Action to find.
     *
     * The Pairing is made by invite rather than by short code because the phone
     * has to end up on a Relay address this sequence knows, and a short code
     * carries the *inviting* device's address inside its payload. It costs the
     * run one single-use token.
     *
     * The session is taken down before this method returns. What follows must
     * begin from a phone that is genuinely closed, and a Pairing brings a
     * session up on its own.
     */
    @Test
    fun a_pairs_the_apps_own_facade_and_copies_something() = relaxed {
        TestRelay.assertReachable()
        Logcat.clear()
        val repository = this.repository
        runBlocking {
            // A clean slate on the app's own database: this sequence is the only
            // thing that uses it and a Pairing left by a previous run would make
            // "the newest Entry" ambiguous.
            repository.listPairings().forEach { repository.forgetPairing(it.userId) }
            val paired = repository.pairWithInvite(
                TestRelay.url,
                TestRelay.nextInvite(),
                "the phone that is not running",
            )
            repository.setActivePairing(paired.userId)
            repository.stopAllSessions()
            scratch(PAIRED_USER).writeText(paired.userId)
        }
        val offered = "offered-by-a-closed-phone-${System.currentTimeMillis()}"
        scratch(OFFERED).writeText(offered)
        clip.putText(offered)
        Evidence.log("closed phone  = paired ${scratch(PAIRED_USER).readText()} against ${TestRelay.url}")
        Evidence.log("closed phone  = clipboard now holds $offered")
    }

    /**
     * The Offer that ran with the app not running made an Entry, and it reached
     * the Relay.
     *
     * Two assertions and they are different questions. The first is the ticket's:
     * the Standing Action read the clipboard — which a `BroadcastReceiver` could
     * not have done — and captured it. The second is stronger and needs a
     * witness that has never seen this phone's database: a fresh in-memory
     * facade joins the same Pairing with a short code and reads the newest Entry
     * back off the Relay with `recall_latest`, which always performs the round
     * trip. A stale local cache cannot fake that, and neither can a queue that
     * never drained.
     */
    @Test
    fun b_the_offer_became_an_entry_and_reached_the_relay() = relaxed {
        val offered = scratch(OFFERED).readText()
        val userId = scratch(PAIRED_USER).readText()
        val repository = this.repository

        val entry = runBlocking {
            awaitEntry("the Standing Action's Offer must be an Entry on this phone") {
                repository.listHistory(userId).firstOrNull { it.preview == offered }
            }
        }
        Evidence.log("closed offer  = Entry id=${entry.id} preview=${entry.preview}")

        val witness = runBlocking { repository.joinAsASecondDevice(userId) }
        try {
            val onTheRelay = witness.recallLatest(userId).text
            assertEquals(
                "the Entry a closed phone captured must reach the Relay, where another device " +
                    "reads it. It did not.",
                offered,
                onTheRelay,
            )
            Evidence.log("closed offer  = a second device read it back off the Relay")
        } finally {
            witness.close()
        }
    }

    /**
     * The notification never shows an Entry's text, checked against a real one.
     *
     * The whitelist half of this is in `StandingActionsNotificationTest`, which
     * is the stronger form. This is the blacklist half, and it is worth the
     * separate run because it can only be done here: it needs a phone with an
     * actual Entry on it, whose actual Preview is a string nothing in this app's
     * resources contains.
     */
    @Test
    fun c_the_notification_shows_no_entry_text_even_with_entries_to_show() {
        val offered = scratch(OFFERED).readText()
        assertTrue(
            "the notification must be up on a phone that was just started",
            StandingActions.post(context),
        )
        val said = PostedNotifications.words(context)
        assertTrue("there is no notification to inspect", said.isNotEmpty())
        assertTrue(
            "a posted notification carries the Entry's text \"$offered\". It must preview no " +
                "Entry text anywhere. What it says: $said",
            said.none { it.contains(offered) },
        )
        Evidence.log("no preview    = posted notifications say $said, and no Entry text")
    }

    /**
     * Put something else on the clipboard, so a Recall has to actually change it.
     *
     * Separated from the assertion by a force-stop and an `am start`, which is
     * the whole point: the Recall in between runs in a process this method's
     * process knows nothing about.
     */
    @Test
    fun d_replaces_the_clipboard_before_the_recall() {
        Logcat.clear()
        clip.putText("something else entirely, which a Recall must replace")
        Evidence.log("closed recall = clipboard cleared of the Entry before the action")
    }

    /**
     * The Recall that ran with the app not running put the newest Entry on the
     * clipboard.
     *
     * Read through `ClipboardManager` itself rather than through this app's own
     * `Clipboard`, so what is asserted is that the platform's clipboard changed
     * — the criterion — rather than that a wrapper agrees with itself.
     */
    @Test
    fun e_the_recall_put_the_newest_entry_on_the_clipboard() {
        val offered = scratch(OFFERED).readText()
        assertEquals(
            "a Recall from the notification, with the app not running, must leave the newest " +
                "Entry on the clipboard",
            offered,
            clip.requireText("after the Standing Action's Recall"),
        )
        Evidence.log("closed recall = the clipboard now holds the Entry the Relay had")
    }

    /**
     * A share from another app becomes an Entry; one the sender marks sensitive
     * does not.
     *
     * Both halves fired at the real exported activity from the host, as any
     * other app's share sheet would. [ShareTargetTest] covers the judgement in
     * detail; this proves it is wired to the manifest entry and to the facade.
     */
    @Test
    fun f_a_share_became_an_entry_and_a_sensitive_one_did_not() = relaxed {
        // Both texts come from the host, which put the same two strings in the
        // two `am start` commands it fired while the app was not running. They
        // cannot be scratch files: the sharing app is `adb`, and it has no way
        // to read one.
        val shared = argument("sharedText")
        val secret = argument("sensitiveText")
        val userId = scratch(PAIRED_USER).readText()
        val repository = this.repository

        val entry = runBlocking {
            awaitEntry("the shared text must become an Entry") {
                repository.listHistory(userId).firstOrNull { it.preview == shared }
            }
        }
        Evidence.log("share         = Entry id=${entry.id} preview=${entry.preview}")

        val refused = runBlocking { repository.listHistory(userId) }.none { it.preview == secret }
        assertTrue(
            "content the sending app marked sensitive became an Entry. Honouring the flag means " +
                "not sending it: an Offered Capture reaches the Relay and every paired device.",
            refused,
        )
        Evidence.log("share         = the sensitive one was refused and is in no History")
    }

    /**
     * A Recall from a Standing Action with no route to the Relay hands over the
     * cache **and says so**.
     *
     * The missing network is real and it is [RelayProxy]'s: the Pairing is made
     * against a port this process owns, so a request to it gets a genuine
     * connection refusal from the real network stack rather than a mocked one.
     * The cut is [RelayProxy.close] as usual, and it stays cut for the run that
     * follows because the process that owned the port is force-stopped — nothing
     * rebinds it. The emulator's own switches would have taken the network away
     * from every other test in the run.
     *
     * What has to be surfaced is `RecallSource.CACHE`. There is no screen to put
     * a band on, so the Toast is the surface, and the sentence it carries is the
     * same `recall_from_cache` the in-app band uses. The assertion is on the
     * report rather than on the return value, exactly as ticket 10's on-screen
     * one is: a silent fallback hands over yesterday's link and looks like a
     * success.
     */
    @Test
    fun g_pairs_against_a_proxy_seeds_a_cached_entry_and_cuts_the_relay() = relaxed {
        TestRelay.assertReachable()
        Logcat.clear()
        val repository = this.repository
        val proxy = RelayProxy.inFrontOfTheTestRelay()
        val stale = "the-newest-entry-this-phone-had-${System.currentTimeMillis()}"
        runBlocking {
            repository.listPairings().forEach { repository.forgetPairing(it.userId) }
            val paired = repository.pairWithInvite(proxy.url, TestRelay.nextInvite(), "cut-off phone")
            repository.setActivePairing(paired.userId)
            scratch(PAIRED_USER).writeText(paired.userId)
            // A genuinely cached Entry rather than a fixture: offered here, sent
            // through the Relay, and read back, so the fallback has something
            // real to fall back to.
            repository.startSession(paired.userId)
            repository.offerText(stale)
            awaitEntry("the Entry to fall back to must be cached") {
                repository.listHistory(paired.userId).firstOrNull { it.preview == stale }
            }
            repository.stopAllSessions()
        }
        scratch(STALE).writeText(stale)
        clip.putText("not the Entry, so a Recall has to change this")

        proxy.close()
        proxy.assertUnreachable()
        proxy.shutdown()
        Evidence.log("relay gone    = ${proxy.url} refuses connections; the Pairing points at it")
    }

    /**
     * And the fallback was reported rather than slipped through.
     *
     * Two assertions, and the first is the one that matters: the clipboard did
     * change, so the operation was correct — which is exactly what makes a
     * silent version of it dangerous.
     */
    @Test
    fun h_the_cut_off_recall_handed_over_the_cache_and_said_so() {
        val stale = scratch(STALE).readText()
        assertEquals(
            "the fallback still has to hand over the newest Entry this phone had",
            stale,
            clip.requireText("after a Standing Action Recall with no Relay"),
        )
        val sentence = context.getString(R.string.recall_from_cache)
        val line = Logcat.await(
            STANDING_ACTION_TAG,
            "a Recall that fell back to the cache must say so",
        ) { it.contains(StandingActions.ACTION_RECALL_LATEST) }
        assertTrue(
            "a Recall that fell back to the cache reported \"$line\" instead of the stale " +
                "warning. A silent fallback hands over yesterday's link and looks like a success.",
            line.contains(sentence.take(SENTENCE_PREFIX)),
        )
        Evidence.log("stale recall  = reported: ${line.substringAfter(": ").take(120)}")
    }

    /** Forget everything this sequence left on the Relay and on the phone. */
    @Test
    fun z_cleans_up() = relaxed {
        val repository = this.repository
        runBlocking {
            repository.listPairings().forEach { runCatching { repository.forgetPairing(it.userId) } }
        }
        listOf(PAIRED_USER, OFFERED, STALE).forEach { scratch(it).delete() }
        Evidence.log("cleanup       = every Pairing forgotten, scratch files gone")
    }

    // -- the machinery ---------------------------------------------------------

    /**
     * Run [block] with the debug transport-policy marker in place, and take it
     * away again whatever happens.
     *
     * The `finally` is not tidiness. A method that fails midway must not leave a
     * debug install permanently willing to talk to a cleartext Relay for every
     * later run on that emulator — the relaxation has to be as short-lived as
     * the test that needed it. The marker is read when the facade is first
     * opened, so planting it before the first touch of `repository` is
     * what makes it take effect for this process.
     */
    private fun relaxed(block: () -> Unit) {
        val marker = File(context.filesDir, RELAXED_TRANSPORT_POLICY_MARKER)
        try {
            marker.createNewFile()
            block()
        } finally {
            marker.delete()
        }
    }

    private val repository: SharepasteRepository
        get() = (context.applicationContext as SharepasteApplication).repository

    /** An argument the host passed with `am instrument -e <name> <value>`. */
    private fun argument(name: String): String = requireNotNull(
        InstrumentationRegistry.getArguments().getString(name),
    ) { "the host must pass -e $name; see the sequence in the issue file" }

    private fun scratch(name: String) = File(context.filesDir, name)

    /**
     * A second Device on the same User, which has never seen this phone's
     * database.
     *
     * The only honest witness for "it reached the Relay". A short code minted by
     * the phone's own Pairing is what a person scanning from another device
     * would use, and the facade that claims it is in memory and gone by the end
     * of the method.
     */
    private suspend fun SharepasteRepository.joinAsASecondDevice(userId: String): Sharepaste {
        val code = pairStart(userId).code
        val witness = Sharepaste.openInMemory(
            keychain = InMemoryKeychain(),
            clipboard = NoClipboard,
            events = SilentSink,
            requireHttps = false,
        )
        val paired = witness.pairWithCode(code, "the witness")
        witness.setActivePairing(paired.userId)
        return witness
    }

    /**
     * Poll for an Entry, or say what was there instead.
     *
     * Everything interesting arrived from another process, so there is nothing
     * to await on: only the database, and only by asking it again.
     */
    private suspend fun awaitEntry(what: String, read: suspend () -> com.sharepaste.core.Entry?):
        com.sharepaste.core.Entry {
        val deadline = System.nanoTime() + ENTRY_TIMEOUT_SECONDS * 1_000_000_000L
        while (System.nanoTime() < deadline) {
            read()?.let { return it }
            Thread.sleep(300)
        }
        throw AssertionError("$what: it never appeared in ${ENTRY_TIMEOUT_SECONDS}s")
    }

    private companion object {
        const val STANDING_ACTION_TAG = "SharepasteStandingAction"
        const val ENTRY_TIMEOUT_SECONDS = 45L

        /** See `StandingActionsNotificationTest.REPORT_PREFIX`. */
        const val SENTENCE_PREFIX = 30

        // Passed between processes through app-private files, because a
        // force-stop is what separates the halves and nothing else survives it.
        const val PAIRED_USER = "standing-actions-user"
        const val OFFERED = "standing-actions-offered"
        const val STALE = "standing-actions-stale"
    }
}
