mod common;

use sharepaste_desktop_lib::core::crypto::{decrypt, encrypt, random_user_key};
use sharepaste_desktop_lib::core::http::ServerClient;
use sharepaste_desktop_lib::core::pairing::payload::{
    base64_decode, base64_encode, fetch_and_decrypt_pair_payload, secret_proof_hex, start_pair,
    upload_pair_payload, PairPayload,
};
use sharepaste_desktop_lib::core::pairing::shortcode::decode as decode_shortcode;

#[tokio::test]
async fn pair_second_device_via_shortcode() {
    let Some(server) = common::start() else {
        return;
    };
    let (_username, invite) = common::create_invite(&server, "alice");
    let api = ServerClient::new(server.url.clone()).unwrap();
    let claimed = api.claim_invite(&invite, "mac-A").await.unwrap();
    let user_key = random_user_key();
    let inviter = api.with_token(claimed.device_token);

    let started = start_pair(&inviter).await.unwrap();
    let payload = decode_shortcode(&started.shortcode).unwrap();

    let claimer = ServerClient::new(server.url.clone()).unwrap();
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
        sharepaste_desktop_lib::core::pairing::invite::hex::decode_user_key(&pp.user_key).unwrap();
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
