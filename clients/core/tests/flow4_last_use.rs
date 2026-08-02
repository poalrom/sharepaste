mod common;

use sharepaste_core::crypto::{encrypt, random_user_key};
use sharepaste_core::http::{ServerClient, TransportPolicy};
use sharepaste_core::pairing::payload::base64_encode;

/// A **Use** across the real wire.
///
/// The relay's `POST /entries/:id/use` and this client's `use_entry` were
/// written against a written contract rather than against each other, and the
/// sequence is the part that has to agree exactly: a use that did not raise the
/// entry above the fetching device's watermark would leave every other device
/// holding the right entries in the wrong order, with nothing to detect it by.
/// Both sides pin their own idea of the shape; only this test proves the two
/// ideas are the same one.
#[tokio::test]
async fn a_use_brings_an_entry_back_past_a_watermark_that_had_passed_it() {
    let Some(server) = common::start() else {
        return;
    };
    let (_username, invite) = common::create_invite(&server, "alice");
    let api = ServerClient::new(server.url.clone(), TransportPolicy::AllowCleartext).unwrap();
    let claimed = api.claim_invite(&invite, "mac-1").await.unwrap();
    let api = api.with_token(claimed.device_token);

    let key = random_user_key();
    let seal = |text: &[u8]| base64_encode(&encrypt(&key, &claimed.user_id, text).unwrap());
    let older = api.post_entry(&seal(b"ssh admin@10.0.0.4")).await.unwrap();
    let newer = api.post_entry(&seal(b"captured since")).await.unwrap();

    assert_eq!(
        older.last_use, older.created_at,
        "an entry never used since capture was last used at capture"
    );
    assert!(newer.seq > older.seq, "sequences climb with capture");

    // A device that has fetched everything: its watermark is past both.
    let watermark = newer.seq;
    assert!(api.list_entries(watermark, 10).await.unwrap().is_empty());

    let used = api.use_entry(older.id).await.unwrap();

    assert!(
        used.seq > watermark,
        "a use has to rise above every watermark, or the reorder reaches nobody"
    );
    assert!(used.last_use >= older.created_at);

    let caught_up = api.list_entries(watermark, 10).await.unwrap();
    assert_eq!(
        caught_up.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![older.id],
        "the used entry, and only it, comes back down the one pipe there is"
    );
    let row = &caught_up[0];
    assert_eq!(row.seq, used.seq);
    assert_eq!(row.last_use, used.last_use);
    assert_eq!(
        row.created_at, older.created_at,
        "and its identity is untouched: same id, same created_at"
    );
}

/// A use of an entry this user does not have is a 404, exactly as a delete is.
#[tokio::test]
async fn a_use_of_someone_elses_entry_is_refused() {
    let Some(server) = common::start() else {
        return;
    };
    let api = ServerClient::new(server.url.clone(), TransportPolicy::AllowCleartext).unwrap();

    let (_, mine) = common::create_invite(&server, "alice");
    let mine = api.claim_invite(&mine, "mac-1").await.unwrap();
    let (_, theirs) = common::create_invite(&server, "bob");
    let theirs = api.claim_invite(&theirs, "mac-2").await.unwrap();

    let key = random_user_key();
    let sealed = base64_encode(&encrypt(&key, &theirs.user_id, b"not yours").unwrap());
    let hidden = api
        .clone()
        .with_token(theirs.device_token)
        .post_entry(&sealed)
        .await
        .unwrap();

    let err = api
        .with_token(mine.device_token)
        .use_entry(hidden.id)
        .await
        .unwrap_err();

    assert!(
        matches!(err, sharepaste_core::errors::AppError::NotFound(_)),
        "got {err:?}"
    );
}
