mod common;

use sharepaste_core::http::{ServerClient, TransportPolicy};

/// `GET /me` across the real wire.
///
/// The relay route and this client's `MeResp` were written against a written
/// contract rather than against each other, so the deserialiser is the one
/// place a field-name or nullability disagreement hides until runtime. The unit
/// tests on both sides pin their own idea of the shape; only this test proves
/// the two ideas are the same one.
#[tokio::test]
async fn me_reports_the_user_and_every_paired_device() {
    let Some(server) = common::start() else {
        return;
    };
    let (username, invite) = common::create_invite(&server, "alice");
    let api = ServerClient::new(server.url.clone(), TransportPolicy::AllowCleartext).unwrap();
    let claimed = api.claim_invite(&invite, "mac-1").await.unwrap();
    let api = api.with_token(claimed.device_token);

    let me = api.me().await.unwrap();

    assert_eq!(me.user.id, claimed.user_id);
    assert_eq!(me.user.username, username);

    let device = me
        .devices
        .iter()
        .find(|d| d.device_id == claimed.device_id)
        .expect("the device that just paired is in its own device list");
    // The Device Label is the whole reason this endpoint exists: it is what a
    // row's Origin column renders, and it is reachable no other way.
    assert_eq!(device.label.as_deref(), Some("mac-1"));
    assert_eq!(device.revoked_at, None);
    assert!(device.created_at > 0);
}
