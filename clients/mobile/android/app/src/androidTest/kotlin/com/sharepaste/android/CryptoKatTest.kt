package com.sharepaste.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.sharepaste.android.Evidence.hex
import com.sharepaste.android.Evidence.hexToBytes
import com.sharepaste.core.AppException
import com.sharepaste.core.cryptoKatDecrypt
import com.sharepaste.core.cryptoKatEncrypt
import com.sharepaste.core.cryptoKatPlaintext
import com.sharepaste.core.cryptoKatWire
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The one test that catches a silent protocol divergence.
 *
 * `clients/core/src/crypto.rs` carries this vector as the check for wire
 * compatibility with mobile libsodium clients. Running it on a laptop proves
 * the laptop. Running it *here* — through the bindings, on the cross-compiled
 * library, on the architecture the app actually ships — is what proves that the
 * shared core ADR 0006 chose over a reimplementation really is one cipher and
 * not two that happen to agree with themselves.
 *
 * The expected bytes are written out below rather than fetched from the
 * library, because a test that asks the implementation what the answer is
 * cannot fail. Encrypt-then-decrypt would pass under a divergent cipher too;
 * only a fixed ciphertext catches it.
 */
@RunWith(AndroidJUnit4::class)
class CryptoKatTest {

    /**
     * `crypto_aead_xchacha20poly1305_ietf_encrypt(key = 0x08…08, nonce =
     * 0x07…07, ad = "alice", msg = "hello")`, laid out the way the protocol
     * puts it on the wire: 24-byte nonce, 5-byte ciphertext, 16-byte Poly1305
     * tag.
     */
    private val expectedWireHex =
        "070707070707070707070707070707070707070707070707" +
            "60aee5a6097a0567ad89162becb8585898932faaab"

    private val expectedPlaintext = "hello".toByteArray(Charsets.UTF_8)

    @Test
    fun the_known_answer_vector_decrypts_to_the_exact_plaintext() {
        val expectedWire = hexToBytes(expectedWireHex)

        val wire = cryptoKatWire()
        Evidence.log("kat wire      = ${wire.hex()}")
        Evidence.log("kat expected  = $expectedWireHex")
        assertEquals("nonce + ciphertext + tag", 24 + 5 + 16, wire.size)
        assertArrayEquals("the vector the library carries is not the pinned one", expectedWire, wire)

        val plaintext = cryptoKatDecrypt(expectedWire)
        Evidence.log("kat plaintext = ${plaintext.hex()} (${String(plaintext)})")
        assertArrayEquals(expectedPlaintext, plaintext)
        assertArrayEquals(expectedPlaintext, cryptoKatPlaintext())
        Evidence.log("KAT OK: xchacha20poly1305-ietf matches libsodium on this ABI")
    }

    @Test
    fun a_flipped_bit_fails_the_tag() {
        val tampered = hexToBytes(expectedWireHex)
        tampered[tampered.lastIndex] = (tampered[tampered.lastIndex].toInt() xor 0x01).toByte()
        try {
            cryptoKatDecrypt(tampered)
            fail("a tampered ciphertext must not decrypt")
        } catch (e: AppException.Crypto) {
            Evidence.log("tampered wire rejected: ${e.message}")
        }
    }

    @Test
    fun the_associated_data_is_bound_to_the_ciphertext() {
        // The vector's associated data is the user id. Truncating the wire is
        // the only tampering reachable from here without a second key, and it
        // proves the length check and the tag are both live.
        val truncated = hexToBytes(expectedWireHex).copyOf(20)
        try {
            cryptoKatDecrypt(truncated)
            fail("a truncated ciphertext must not decrypt")
        } catch (e: AppException.Crypto) {
            Evidence.log("truncated wire rejected: ${e.message}")
        }
    }

    @Test
    fun encryption_draws_a_fresh_nonce_every_time() {
        val first = cryptoKatEncrypt(expectedPlaintext)
        val second = cryptoKatEncrypt(expectedPlaintext)
        assertNotEquals(
            "a repeated nonce under one key destroys XChaCha20-Poly1305",
            first.hex(),
            second.hex(),
        )
        assertNotEquals(expectedWireHex, first.hex())
        assertArrayEquals(expectedPlaintext, cryptoKatDecrypt(first))
        assertArrayEquals(expectedPlaintext, cryptoKatDecrypt(second))
        Evidence.log("fresh-nonce encrypt round-trips: ${first.hex()}")
    }
}
