mod common;

use sharepaste_desktop_lib::core::crypto::{encrypt, random_user_key};
use sharepaste_desktop_lib::core::http::ServerClient;
use sharepaste_desktop_lib::core::pairing::payload::{base64_decode, base64_encode};

#[tokio::test]
async fn invite_then_post_and_list() {
    let Some(server) = common::start() else {
        return;
    };
    let (_username, invite) = common::create_invite(&server, "alice");
    let api = ServerClient::new(server.url.clone()).unwrap();
    let claimed = api.claim_invite(&invite, "mac-1").await.unwrap();
    let api = api.with_token(claimed.device_token);

    let key = random_user_key();
    let ct = encrypt(&key, &claimed.user_id, b"hello sharepaste").unwrap();
    let posted = api.post_entry(&base64_encode(&ct)).await.unwrap();
    assert!(posted.id > 0);

    let rows = api.list_entries(0, 10).await.unwrap();
    assert_eq!(rows.len(), 1);
    let body = base64_decode(&rows[0].ciphertext).unwrap();
    let pt = sharepaste_desktop_lib::core::crypto::decrypt(&key, &claimed.user_id, &body).unwrap();
    assert_eq!(pt, b"hello sharepaste");
}
