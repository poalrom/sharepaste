import Foundation
import SharepasteCore
import SharepasteKit
import XCTest

/// The shipped app refuses a cleartext Relay, and says why.
///
/// **This test exists because every other test in this suite opens the facade
/// with `requireHttps: false`.** They have to: the test relay is plain HTTP and
/// there is no publicly trusted certificate to put in front of it on a CI
/// runner. That concession is only safe while something proves the app itself
/// does not make it — which is this.
///
/// The enforcement has to live in the core, and on this platform there is no
/// second opinion available at all. App Transport Security governs `URLSession`
/// and reaches no further; every Sharepaste request goes out of Rust `reqwest`
/// on raw sockets, which never consults it. So unlike Android — which at least
/// has `usesCleartextTraffic` covering its Java stack — iOS has exactly one
/// enforcement point, and it is the flag asserted below.
///
/// Where Android asserted against `application.repository`, this asserts against
/// a facade opened with ``TransportPolicy/requireHttps`` itself. The app module
/// is not a dependency of this target (see `Package.swift`), and the constant is
/// the whole of what the app passes: `SharepasteRepository` takes it as a
/// parameter and `SharepasteApp` hands it this value.
final class TransportPolicyTest: XCTestCase {

    func testTheShippedConfigurationRequiresHttps() {
        // One constant, in one place, for every build. This is the value that
        // travels into `Sharepaste.open`.
        XCTAssertTrue(
            TransportPolicy.requireHttps,
            "the shipped app must require HTTPS; TransportPolicy.requireHttps is false"
        )
    }

    func testTheShippedPolicyRefusesACleartextRelayAndNamesTheReason() throws {
        let core = try openWithTheShippedPolicy()
        XCTAssertThrowsError(
            try core.pairWithInvite(
                serverUrl: Suite.relayURL,
                token: "a-token-this-never-gets-to-send",
                deviceLabel: "policy test"
            ),
            "the shipped configuration must refuse \(Suite.relayURL)"
        ) { error in
            guard case AppError.InsecureRelay(let detail) = error else {
                return XCTFail("a cleartext relay must be refused by the policy: \(error)")
            }
            XCTAssertTrue(detail.contains(Suite.relayURL), "the explanation names the relay: \(detail)")
            XCTAssertTrue(detail.contains("HTTPS"), "the explanation names what is required: \(detail)")
            XCTAssertTrue(detail.contains("token"), "the explanation names why it matters: \(detail)")
        }
    }

    /// The policy is about the scheme and nothing else.
    ///
    /// A refusal that fired for any unreachable relay would look identical from
    /// the outside and would be useless. Port 1 over `https` refuses immediately
    /// at the transport layer — so the failure comes back as a network error,
    /// which is what tells us the policy let it *through*.
    func testAnHttpsRelayIsNotRefusedByThePolicy() throws {
        let core = try openWithTheShippedPolicy()
        XCTAssertThrowsError(
            try core.pairWithInvite(
                serverUrl: "https://127.0.0.1:1",
                token: "a-token-this-never-gets-to-send",
                deviceLabel: "policy test"
            ),
            "nothing is listening on port 1; this cannot succeed"
        ) { error in
            if case AppError.InsecureRelay = error {
                XCTFail("an https:// relay must not be refused by the transport policy: \(error)")
            }
            XCTAssertTrue(error is AppError, "refused by the network, and typed: \(error)")
        }
    }

    private func openWithTheShippedPolicy() throws -> Sharepaste {
        try Sharepaste.openInMemory(
            keychain: InMemoryKeychain(),
            clipboard: NoClipboard(),
            events: SilentSink(),
            requireHttps: TransportPolicy.requireHttps
        )
    }
}
