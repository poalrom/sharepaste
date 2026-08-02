import Foundation
import SharepasteCore
import XCTest

/// The one test that catches a silent protocol divergence.
///
/// `clients/core/src/crypto.rs` carries this vector as the check for wire
/// compatibility with mobile libsodium clients. Running it on a laptop proves
/// the laptop. Running it *here* — through the bindings, on the cross-compiled
/// archive, on the architecture the app ships — is what proves that the shared
/// core ADR 0006 chose over a reimplementation really is one cipher and not two
/// that happen to agree with themselves.
///
/// The expected bytes are written out below rather than fetched from the
/// library, because a test that asks the implementation what the answer is
/// cannot fail. Encrypt-then-decrypt would pass under a divergent cipher too;
/// only a fixed ciphertext catches it.
final class CryptoKatTest: XCTestCase {

    /// `crypto_aead_xchacha20poly1305_ietf_encrypt(key = 0x08…08, nonce =
    /// 0x07…07, ad = "alice", msg = "hello")`, laid out the way the protocol
    /// puts it on the wire: 24-byte nonce, 5-byte ciphertext, 16-byte Poly1305
    /// tag.
    private let expectedWireHex =
        "070707070707070707070707070707070707070707070707"
        + "60aee5a6097a0567ad89162becb8585898932faaab"

    private let expectedPlaintext = Data("hello".utf8)

    func testTheKnownAnswerVectorDecryptsToTheExactPlaintext() throws {
        let expectedWire = Data(hex: expectedWireHex)

        let wire = cryptoKatWire()
        XCTAssertEqual(wire.count, 24 + 5 + 16, "nonce + ciphertext + tag")
        XCTAssertEqual(
            wire.hex,
            expectedWireHex,
            "the vector the library carries is not the pinned one"
        )

        XCTAssertEqual(try cryptoKatDecrypt(wire: expectedWire), expectedPlaintext)
        XCTAssertEqual(cryptoKatPlaintext(), expectedPlaintext)
    }

    func testAFlippedBitFailsTheTag() {
        var tampered = Data(hex: expectedWireHex)
        tampered[tampered.count - 1] ^= 0x01
        XCTAssertThrowsError(try cryptoKatDecrypt(wire: tampered)) { error in
            guard case AppError.Crypto = error else {
                return XCTFail("a tampered ciphertext must be refused as a crypto failure: \(error)")
            }
        }
    }

    func testTheAssociatedDataIsBoundToTheCiphertext() {
        // The vector's associated data is the user id. Truncating the wire is
        // the only tampering reachable from here without a second key, and it
        // proves the length check and the tag are both live.
        let truncated = Data(hex: expectedWireHex).prefix(20)
        XCTAssertThrowsError(try cryptoKatDecrypt(wire: Data(truncated))) { error in
            guard case AppError.Crypto = error else {
                return XCTFail("a truncated ciphertext must be refused as a crypto failure: \(error)")
            }
        }
    }

    func testEncryptionDrawsAFreshNonceEveryTime() throws {
        let first = try cryptoKatEncrypt(plaintext: expectedPlaintext)
        let second = try cryptoKatEncrypt(plaintext: expectedPlaintext)
        XCTAssertNotEqual(
            first.hex,
            second.hex,
            "a repeated nonce under one key destroys XChaCha20-Poly1305"
        )
        XCTAssertNotEqual(first.hex, expectedWireHex)
        XCTAssertEqual(try cryptoKatDecrypt(wire: first), expectedPlaintext)
        XCTAssertEqual(try cryptoKatDecrypt(wire: second), expectedPlaintext)
    }
}

extension Data {

    /// The bytes as lower-case hex, which is how the vector above is written and
    /// the only readable form for a failure message about ciphertext.
    var hex: String {
        map { String(format: "%02x", $0) }.joined()
    }

    /// A hex string as bytes. Odd or non-hex input is a mistake in the test
    /// itself, so it traps rather than answering something plausible.
    init(hex: String) {
        precondition(hex.count.isMultiple(of: 2), "a hex vector has an even number of digits")
        var bytes: [UInt8] = []
        bytes.reserveCapacity(hex.count / 2)
        var index = hex.startIndex
        while index < hex.endIndex {
            let next = hex.index(index, offsetBy: 2)
            guard let byte = UInt8(hex[index..<next], radix: 16) else {
                preconditionFailure("not hex: \(hex[index..<next])")
            }
            bytes.append(byte)
            index = next
        }
        self.init(bytes)
    }
}
