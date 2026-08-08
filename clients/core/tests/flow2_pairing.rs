mod common;

use sharepaste_core::crypto::{decrypt, encrypt, random_user_key};
use sharepaste_core::http::{ServerClient, TransportPolicy};
use sharepaste_core::pairing::payload::{
    base64_decode, base64_encode, fetch_and_decrypt_pair_payload, secret_proof_hex, start_pair,
    upload_pair_payload, PairPayload,
};
use sharepaste_core::pairing::shortcode::decode as decode_shortcode;
use sharepaste_core::facade::{RecallSource, Sharepaste, SharepasteConfig};
use sharepaste_core::keychain::InMemoryKeychain;
use sharepaste_core::relay::RelayDial;
use sharepaste_core::testing::{FakeClipboard, RecordingSink};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[tokio::test]
async fn pair_second_device_via_shortcode() {
    let Some(server) = common::start() else {
        return;
    };
    let (_username, invite) = common::create_invite(&server, "alice");
    let api = ServerClient::new(server.url.clone(), TransportPolicy::AllowCleartext).unwrap();
    let claimed = api.claim_invite(&invite, "mac-A").await.unwrap();
    let user_key = random_user_key();
    let inviter = api.with_token(claimed.device_token);

    let started = start_pair(&inviter).await.unwrap();
    let payload = decode_shortcode(&started.shortcode).unwrap();

    let claimer = ServerClient::new(server.url.clone(), TransportPolicy::AllowCleartext).unwrap();
    let proof = secret_proof_hex(&payload.pairing_secret);
    claimer
        .pair_claim(&payload.pair_id.to_string(), &proof)
        .await
        .unwrap();

    upload_pair_payload(
        &inviter,
        payload.pair_id,
        &payload.pairing_secret,
        &claimed.user_id,
        &user_key,
        &server.url,
    )
    .await
    .unwrap();

    let pp: PairPayload =
        fetch_and_decrypt_pair_payload(&claimer, payload.pair_id, &payload.pairing_secret)
            .await
            .unwrap();
    assert_eq!(pp.user_id, claimed.user_id);

    let device = claimer
        .devices(&payload.pair_id.to_string(), &proof, "mac-B")
        .await
        .unwrap();
    let claimer = claimer.with_token(device.device_token);

    let key2 =
        sharepaste_core::pairing::invite::hex::decode_user_key(&pp.user_key).unwrap();
    let ct = encrypt(&key2, &pp.user_id, b"from B").unwrap();
    claimer.post_entry(&base64_encode(&ct)).await.unwrap();

    let rows = inviter.list_entries(0, 10).await.unwrap();
    assert_eq!(rows.len(), 1);
    let pt = decrypt(
        &user_key,
        &claimed.user_id,
        &base64_decode(&rows[0].ciphertext).unwrap(),
    )
    .unwrap();
    assert_eq!(pt, b"from B");
}

/// The same handshake, driven through the facade, against the live relay.
///
/// Two things only a real relay settles. **One:** `pair_start` is a single
/// operation — it uploads the payload before it reveals the code — and the code
/// it hands back is one a second device can genuinely claim, all the way through
/// to an entry made on B arriving on A. **Two:** that relay is
/// `http://127.0.0.1:8443`, plain cleartext, and every call here succeeds, so
/// `AppError::InsecureRelay` never appears. The variant is an explanation for a
/// transport failure and *not* a scheme check: a desktop already paired to a
/// cleartext relay has to keep working across an upgrade.
///
/// Sync rather than `#[tokio::test]` on purpose. The facade owns a runtime, and
/// dropping one from inside somebody else's async context panics — which is also
/// the shape a shell with no runtime of its own uses.
#[test]
fn pair_second_device_through_the_facade_over_a_cleartext_relay() {
    let Some(server) = common::start() else {
        return;
    };
    assert!(server.url.starts_with("http://"), "this test is about a cleartext relay");
    let (_username, invite) = common::create_invite(&server, "alice");

    let (a, a_sink, _a_clip) = facade();
    let (b, _b_sink, b_clip) = facade();

    let device_a = a
        .block_on(a.pair_with_invite(&server.url, &invite, "mac-A"))
        .expect("claiming an invite over a working cleartext relay must not error");
    let revealed = a.block_on(a.pair_start(&device_a.user_id)).unwrap();

    let device_b = b
        .block_on(b.pair_with_code(&revealed.code, "mac-B"))
        .expect("the code revealed after the upload must be claimable");
    assert_eq!(device_b.user_id, device_a.user_id, "both devices are the same User");
    assert_ne!(device_b.device_id, device_a.device_id);

    // The claim reaches the inviter as an event, which is the only way it can:
    // `pair_start` returned long before anybody typed the code.
    poll(|| a_sink.pair_claimed() == vec![Some("mac-B".to_string())], "the claim to reach A");

    // B offers, B's uploader drains, A recalls. Nothing else proves the claimer
    // came away with the real user key.
    b.block_on(b.offer(&device_b.user_id, "from B")).unwrap();
    let recalled = poll_for(
        || match a.block_on(a.recall_latest(&device_a.user_id)) {
            Ok(r) if r.text == "from B" => Some(r),
            _ => None,
        },
        "B's entry to reach A through the relay",
    );
    assert_eq!(recalled.source, RecallSource::Relay, "the round trip was made and it answered");
    assert_eq!(b_clip.writes(), Vec::<String>::new(), "an Offer never touches the clipboard");

    a.stop_all_sessions();
    b.stop_all_sessions();
}

/// A facade over nothing but fakes. The relay is the only real thing these tests
/// attach, which is the whole point of the three seams.
fn facade() -> (Arc<Sharepaste>, Arc<RecordingSink>, Arc<FakeClipboard>) {
    let sink = Arc::new(RecordingSink::default());
    let clipboard = FakeClipboard::new();
    let sp = Sharepaste::open_in_memory(SharepasteConfig {
        db_path: PathBuf::from("ignored-by-open_in_memory"),
        keychain: Arc::new(InMemoryKeychain::default()),
        clipboard: clipboard.clone(),
        events: sink.clone(),
        relay: RelayDial::over_http(TransportPolicy::AllowCleartext),
    })
    .unwrap();
    (sp, sink, clipboard)
}

fn poll(pred: impl Fn() -> bool, what: &str) {
    poll_for(|| pred().then_some(()), what);
}

/// Retry for up to thirty seconds. The uploader and the pair-watch run on the
/// facade's own runtime, so a test watches for their effects rather than awaiting
/// them.
fn poll_for<T>(mut attempt: impl FnMut() -> Option<T>, what: &str) -> T {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if let Some(got) = attempt() {
            return got;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("timed out waiting for {what}");
}
