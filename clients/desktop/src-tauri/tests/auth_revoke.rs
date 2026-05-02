mod common;

use sharepaste_desktop_lib::core::http::ServerClient;
use sharepaste_desktop_lib::errors::AppError;

#[tokio::test]
async fn revoking_a_device_returns_401_on_subsequent_calls() {
    let server = common::start();
    let (_, invite_a) = common::create_invite(&server, "alice");
    let api = ServerClient::new(server.url.clone()).unwrap();
    let a = api.claim_invite(&invite_a, "mac-A").await.unwrap();

    let (_, invite_b) = common::create_invite(&server, "bob");
    let _b = api.claim_invite(&invite_b, "mac-B").await.unwrap();

    let auth_a = api.clone().with_token(a.device_token.clone());
    auth_a.revoke_device(&a.device_id).await.unwrap();

    let err = auth_a.list_entries(0, 1).await.unwrap_err();
    assert!(matches!(err, AppError::Auth(_)), "expected Auth, got {err:?}");
}
