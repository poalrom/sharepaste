package com.sharepaste.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.sharepaste.core.AppException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The shipped app refuses a cleartext Relay, and says why.
 *
 * **This test exists because every other instrumented test in this module opens
 * the facade with `requireHttps = false`.** They have to: the test relay is plain
 * HTTP and there is no publicly trusted certificate to put in front of it from
 * inside an emulator. That concession is only safe while something proves the app
 * itself does not make it — which is this.
 *
 * Ticket 08 established why the enforcement has to live in the core at all.
 * `android:usesCleartextTraffic="false"` and the network security config are
 * honoured by Android's *Java* stack only; every Sharepaste request goes through
 * Rust `reqwest` on raw sockets, which never consults either. An instrumented
 * session paired and ran a full SSE stream over `http://` with both settings in
 * place. They stay shipped — they still cover WebView and any future Java-side
 * networking — but they are not the enforcement point and must not be read as one.
 */
@RunWith(AndroidJUnit4::class)
class TransportPolicyTest {

    private val application = InstrumentationRegistry.getInstrumentation()
        .targetContext
        .applicationContext as SharepasteApplication

    @Test
    fun the_shipped_configuration_requires_https() {
        // Set in one place, in the build file, for every variant. This is the
        // value that travels into `Sharepaste.open`.
        assertTrue(
            "the shipped app must require HTTPS; BuildConfig.REQUIRE_HTTPS is false",
            BuildConfig.REQUIRE_HTTPS,
        )
        Evidence.log("policy        = BuildConfig.REQUIRE_HTTPS=${BuildConfig.REQUIRE_HTTPS}")
    }

    @Test
    fun the_apps_own_facade_refuses_a_cleartext_relay_and_names_the_reason() {
        // `application.repository` is the facade the app runs on, opened by the
        // app's own code with the app's own policy. Nothing here is arranged.
        try {
            runBlocking {
                application.repository.pairWithInvite(
                    TestRelay.url,
                    "a-token-this-never-gets-to-send",
                    "policy test",
                )
            }
            fail("the shipped configuration must refuse ${TestRelay.url}")
        } catch (e: AppException.InsecureRelay) {
            Evidence.log("cleartext     = ${e.message}")
            val detail = e.detail
            assertTrue("the explanation names the relay: $detail", detail.contains(TestRelay.url))
            assertTrue("the explanation names what is required: $detail", detail.contains("HTTPS"))
            assertTrue("the explanation names why it matters: $detail", detail.contains("token"))
        }
    }

    /**
     * The policy is about the scheme and nothing else.
     *
     * A refusal that fired for any unreachable relay would look identical from
     * the outside and would be useless. Port 1 over `https` refuses immediately at
     * the transport layer — so the failure comes back as a network error, which is
     * what tells us the policy let it *through*.
     */
    @Test
    fun an_https_relay_is_not_refused_by_the_policy() {
        try {
            runBlocking {
                application.repository.pairWithInvite(
                    "https://127.0.0.1:1",
                    "a-token-this-never-gets-to-send",
                    "policy test",
                )
            }
            fail("nothing is listening on port 1; this cannot succeed")
        } catch (e: AppException.InsecureRelay) {
            fail("an https:// relay must not be refused by the transport policy: ${e.message}")
        } catch (e: AppException) {
            Evidence.log("https allowed = refused by the network, not the policy: ${e.message}")
        }
    }
}
