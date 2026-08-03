use crate::errors::AppError;
use crate::event::CoreEvent;
use crate::pairing::payload::base64_encode;
use crate::platform::EventSink;
use crate::storage::{entries_cache, pending};
use async_trait::async_trait;
use rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;

/// What the relay recorded for an Entry this device just uploaded.
///
/// The id alone used to be enough, and it was thrown away. It is not enough
/// now: the uploader caches the Entry itself rather than waiting for the relay
/// to echo it back, and a cached Entry needs the relay's `created_at` — the
/// number every other device will see it ordered and dated by — its `seq` and
/// its `last_use`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Uploaded {
    pub id: i64,
    pub created_at: i64,
    pub seq: i64,
    pub last_use: i64,
}

/// What the relay recorded for a **Use**.
///
/// `seq` is carried because the route answers it and this type mirrors the
/// wire, and it is deliberately never applied: a sequence is a *watermark*
/// value, and the watermark means "everything up to here has been fetched".
/// This device fetched nothing — it wrote — and the relay may hold entries from
/// other devices below this sequence that it has never seen. The same rule
/// [`Uploader::cache_own_entry`] states at length, for the same reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Used {
    pub seq: i64,
    pub last_use: i64,
}

#[async_trait]
pub trait UploadTransport: Send + Sync {
    async fn upload(&self, ciphertext_b64: &str) -> Result<Uploaded, AppError>;
    /// Record a **Use** of an entry the relay already holds.
    ///
    /// `AppError::NotFound` covers both an entry that is gone and a relay too
    /// old to have the route. Neither is worth distinguishing: skew is not
    /// handled here (the relay is updated first), and a use nobody can record
    /// is a use with nothing to reorder.
    async fn use_entry(&self, entry_id: i64) -> Result<Used, AppError>;
    /// Take one entry off the relay, for an act withdrawn while it was being
    /// uploaded.
    async fn delete_entry(&self, entry_id: i64) -> Result<(), AppError>;
}

/// Why the uploader stopped.
///
/// A rejected device token is not an uploader event, which is why it is a
/// return value and not something on the sink: `AuthFailed` is a *session*
/// state, and only the session's `set_conn_state` flushes Contact on the way
/// out of `Online`. Emitting it from here would leave the connection-state map
/// and the database disagreeing with the UI.
#[derive(Debug, PartialEq, Eq)]
pub enum UploaderExit {
    /// The session was cancelled.
    Cancelled,
    /// The relay rejected this device's token.
    AuthFailed,
}

/// What the relay did with one pending, carried from the network call to the
/// database write that records it.
///
/// A value rather than two branches doing their own thing, so the ack, the
/// queue-depth read and the sink emission happen once for both kinds and
/// cannot drift apart.
enum Sent {
    Capture { uploaded: Uploaded },
    Use { entry_id: i64, used: Used },
    /// A queued use naming an entry the relay no longer has.
    UseVanished { entry_id: i64 },
}

pub struct Uploader {
    pub user_id: String,
    pub conn: Arc<Mutex<Connection>>,
    pub transport: Arc<dyn UploadTransport>,
    pub trigger: Arc<Notify>,
    pub events: Arc<dyn EventSink>,
}

impl Uploader {
    pub async fn run(self, cancel: CancellationToken) -> UploaderExit {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return UploaderExit::Cancelled,
                _ = self.trigger.notified() => {},
            }
            if let Err(e) = self.flush_once().await {
                if matches!(e, AppError::Auth(_)) {
                    return UploaderExit::AuthFailed;
                }
                tracing::warn!(err = %e, "uploader flush errored; will retry on next trigger");
            }
        }
    }

    fn pending_count(&self, count: i64) {
        self.events.emit(CoreEvent::PendingCount {
            user_id: self.user_id.clone(),
            count,
        });
    }

    /// Drain the queue head-first, whatever kind each act is.
    ///
    /// One loop and one set of failure arms for both kinds, because the failure
    /// rules are the same rules: a rejected token stops everything, malformed
    /// input is dropped rather than retried forever, and anything else leaves
    /// the row where it is for the next trigger. What differs is only what the
    /// relay is asked to do and what the answer is recorded against.
    pub(crate) async fn flush_once(&self) -> Result<(), AppError> {
        loop {
            let head = {
                let conn = self.conn.lock().await;
                pending::head(&conn, &self.user_id)?
            };
            let Some(item) = head else { break; };
            let sent = match &item.kind {
                pending::PendingKind::Capture(ciphertext) => {
                    let b64 = base64_encode(ciphertext);
                    self.transport
                        .upload(&b64)
                        .await
                        .map(|uploaded| Sent::Capture { uploaded })
                }
                pending::PendingKind::Use(entry_id) => {
                    let entry_id = *entry_id;
                    match self.transport.use_entry(entry_id).await {
                        Ok(used) => Ok(Sent::Use { entry_id, used }),
                        // The entry no longer exists, so there is nothing to
                        // reorder and nothing to tell anyone. Ack it and drop it.
                        Err(AppError::NotFound(_)) => Ok(Sent::UseVanished { entry_id }),
                        Err(e) => Err(e),
                    }
                }
            };
            match sent {
                Ok(sent) => {
                    // Scoped so the database guard is gone before anything
                    // reaches the sink: see [`EventSink`].
                    let (count, reordered, withdrawn) = {
                        let conn = self.conn.lock().await;
                        // **The withdrawal race, decided here.** The upload above
                        // awaited with this lock released, and a delete inside
                        // that window took the queued act with it. Zero acked
                        // rows is that, and the only evidence of it there is:
                        // reconciling would attach a relay id to a row somebody
                        // deleted, or re-create it.
                        let acked = pending::ack(&conn, item.rowid)? > 0;
                        let mut withdrawn = None;
                        let reordered = match sent {
                            Sent::Capture { uploaded } if acked => {
                                self.settle(&conn, item.local_entry_id, uploaded);
                                false
                            }
                            Sent::Capture { uploaded } => {
                                withdrawn = Some(uploaded.id);
                                false
                            }
                            Sent::Use { entry_id, used } => {
                                entries_cache::set_last_use(
                                    &conn, &self.user_id, entry_id, used.last_use,
                                )? > 0
                            }
                            Sent::UseVanished { entry_id } => {
                                tracing::info!(
                                    entry_id,
                                    "dropped a queued use of an entry the relay no longer has"
                                );
                                false
                            }
                        };
                        (pending::count(&conn, &self.user_id)?, reordered, withdrawn)
                    };
                    // Safe to ask for, because the upload that just returned is
                    // proof the relay is reachable. A failure here leaves one
                    // entry on the relay that nothing local refers to; the next
                    // backfill will bring it down, and it is a far smaller lie
                    // than a deleted row reappearing.
                    if let Some(relay_id) = withdrawn {
                        tracing::info!(
                            relay_id,
                            "this act was withdrawn while it was uploading; deleting it again"
                        );
                        if let Err(e) = self.transport.delete_entry(relay_id).await {
                            tracing::warn!(
                                err = %e, relay_id,
                                "could not take back a withdrawn act the relay had already taken"
                            );
                        }
                    }
                    // A capture raises no `EntryAdded`: the Entry has existed
                    // since it was captured and the shells have had it all along.
                    // A use raises no `EntryAdded` either, and nothing reorders
                    // at a flush — the relay stamps a pending act exactly where
                    // this device already showed it.
                    if reordered {
                        self.events.emit(CoreEvent::HistoryChanged {
                            user_id: self.user_id.clone(),
                        });
                    }
                    self.pending_count(count);
                }
                Err(AppError::Auth(s)) => {
                    let conn = self.conn.lock().await;
                    pending::record_failure(&conn, item.rowid, &s)?;
                    return Err(AppError::Auth(s));
                }
                Err(AppError::BadInput(s)) => {
                    let count = {
                        let conn = self.conn.lock().await;
                        pending::ack(&conn, item.rowid)?;
                        tracing::warn!(err = %s, rowid = item.rowid, "dropped malformed pending entry");
                        pending::count(&conn, &self.user_id)?
                    };
                    self.pending_count(count);
                }
                Err(e) => {
                    let conn = self.conn.lock().await;
                    pending::record_failure(&conn, item.rowid, &e.to_string())?;
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    /// Attach what the relay recorded to the Entry the capture already created.
    ///
    /// **Reconciliation, not insertion.** The Entry exists from the moment of
    /// capture (ADR 0013), so this flush has nothing to create and nothing to
    /// announce: it hands the row the relay's id, the `created_at` every other
    /// device will see it dated by, and its `last_use`. The `local_id` does not
    /// move, which is what lets both shells keep a row's selection and keyboard
    /// cursor across a flush.
    ///
    /// **`seq` is deliberately not applied.** `last_seen_seq` means "everything
    /// up to here has been fetched", and the relay may hold Entries from other
    /// devices with *lower* sequences that this device has never seen — moving
    /// the watermark past them skips them for good, because the next `since=`
    /// fetch starts after it. This device wrote; it did not fetch.
    ///
    /// A failure is logged and swallowed. The act is on the relay, which is the
    /// part that matters, and the relay's echo carries the same id back.
    fn settle(&self, conn: &Connection, local_entry_id: Option<i64>, uploaded: Uploaded) {
        let Some(local_entry_id) = local_entry_id else {
            tracing::warn!(
                relay_id = uploaded.id,
                "a queued capture named no local entry; the relay's echo will insert it"
            );
            return;
        };
        match entries_cache::attach_relay_id(
            conn,
            &self.user_id,
            local_entry_id,
            uploaded.id,
            uploaded.created_at,
            uploaded.last_use,
            crate::now_ms(),
        ) {
            Ok(0) => tracing::info!(
                local_entry_id,
                relay_id = uploaded.id,
                "the entry this act created is gone; nothing to reconcile"
            ),
            Ok(_) => {}
            Err(e) => tracing::warn!(
                err = %e, local_entry_id, relay_id = uploaded.id,
                "could not attach the relay's id; the next backfill will"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::dto::EntryRow;
    use crate::storage::{open_in_memory, pending};
    use crate::testing::RecordingSink;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct OkTransport {
        count: AtomicUsize,
        uses: Mutex<Vec<i64>>,
        deleted: Mutex<Vec<i64>>,
        entry_gone: bool,
    }

    impl OkTransport {
        fn new() -> Self {
            OkTransport {
                count: AtomicUsize::new(0),
                uses: Mutex::new(Vec::new()),
                deleted: Mutex::new(Vec::new()),
                entry_gone: false,
            }
        }

        /// A relay that no longer has whatever entry a queued use names — the
        /// same answer a relay too old for the route gives.
        fn without_the_entry() -> Self {
            OkTransport { entry_gone: true, ..Self::new() }
        }
    }

    #[async_trait]
    impl UploadTransport for OkTransport {
        async fn upload(&self, _ct: &str) -> Result<Uploaded, AppError> {
            let n = self.count.fetch_add(1, Ordering::SeqCst) as i64;
            // `now_ms`, not a fixed date: `upsert_and_prune` drops anything older
            // than 30 days, so a cached Entry stamped in 2023 vanishes as it is written.
            let at = crate::now_ms() + n;
            Ok(Uploaded { id: 42 + n, created_at: at, seq: 42 + n, last_use: at })
        }

        async fn use_entry(&self, entry_id: i64) -> Result<Used, AppError> {
            self.uses.lock().await.push(entry_id);
            if self.entry_gone {
                return Err(AppError::NotFound("entry not found".into()));
            }
            Ok(Used { seq: 900, last_use: crate::now_ms() })
        }

        async fn delete_entry(&self, entry_id: i64) -> Result<(), AppError> {
            self.deleted.lock().await.push(entry_id);
            Ok(())
        }
    }

    /// A relay that keeps what it was given, so a test can replay the fan-out
    /// the real one performs.
    ///
    /// [`OkTransport`] discards the ciphertext, which is enough for the queue's
    /// own behaviour and not enough to drive an echo: the echo carries the same
    /// bytes back under the same id, and a test that re-encrypted them would be
    /// proving something about a nonce.
    #[derive(Default)]
    struct EchoingRelay {
        /// `(id, ciphertext_b64)` in the order the relay took them.
        accepted: Mutex<Vec<(i64, String)>>,
    }

    #[async_trait]
    impl UploadTransport for EchoingRelay {
        async fn upload(&self, ct: &str) -> Result<Uploaded, AppError> {
            let mut accepted = self.accepted.lock().await;
            let id = 101 + accepted.len() as i64;
            accepted.push((id, ct.to_string()));
            let at = crate::now_ms();
            Ok(Uploaded { id, created_at: at, seq: id, last_use: at })
        }

        async fn use_entry(&self, _entry_id: i64) -> Result<Used, AppError> {
            Ok(Used { seq: 900, last_use: crate::now_ms() })
        }

        async fn delete_entry(&self, _entry_id: i64) -> Result<(), AppError> {
            Ok(())
        }
    }

    /// A relay whose upload blocks until a test releases it, and which records
    /// what it was later asked to delete.
    ///
    /// The only way to drive the withdrawal race deliberately rather than by
    /// timing luck: `parked` says the flush is provably inside its `await`, and
    /// `release` lets it out again once the test has withdrawn the act.
    struct BlockingRelay {
        parked: tokio::sync::Semaphore,
        release: tokio::sync::Semaphore,
        deleted: Mutex<Vec<i64>>,
    }

    impl BlockingRelay {
        fn new() -> Self {
            BlockingRelay {
                parked: tokio::sync::Semaphore::new(0),
                release: tokio::sync::Semaphore::new(0),
                deleted: Mutex::new(Vec::new()),
            }
        }

        /// Wait until an upload is inside the await, and nothing else.
        async fn wait_until_parked(&self) {
            self.parked.acquire().await.expect("the upload parks").forget();
        }
    }

    #[async_trait]
    impl UploadTransport for BlockingRelay {
        async fn upload(&self, _ct: &str) -> Result<Uploaded, AppError> {
            self.parked.add_permits(1);
            self.release.acquire().await.expect("the test releases the upload").forget();
            let at = crate::now_ms();
            Ok(Uploaded { id: 500, created_at: at, seq: 500, last_use: at })
        }

        async fn use_entry(&self, _entry_id: i64) -> Result<Used, AppError> {
            Ok(Used { seq: 900, last_use: crate::now_ms() })
        }

        async fn delete_entry(&self, entry_id: i64) -> Result<(), AppError> {
            self.deleted.lock().await.push(entry_id);
            Ok(())
        }
    }

    struct AuthFail;
    #[async_trait]
    impl UploadTransport for AuthFail {
        async fn upload(&self, _ct: &str) -> Result<Uploaded, AppError> {
            Err(AppError::Auth("revoked".into()))
        }

        async fn use_entry(&self, _entry_id: i64) -> Result<Used, AppError> {
            Err(AppError::Auth("revoked".into()))
        }

        async fn delete_entry(&self, _entry_id: i64) -> Result<(), AppError> {
            Err(AppError::Auth("revoked".into()))
        }
    }

    fn uploader(
        conn: Arc<Mutex<Connection>>,
        transport: Arc<dyn UploadTransport>,
    ) -> (Uploader, Arc<RecordingSink>) {
        let sink = Arc::new(RecordingSink::default());
        let up = Uploader {
            user_id: "u".into(),
            conn,
            transport,
            trigger: Arc::new(Notify::new()),
            events: sink.clone(),
        };
        (up, sink)
    }

    /// A Pairing for the rows a capture creates to name as their Origin.
    async fn paired(conn: &Arc<Mutex<Connection>>) {
        let c = conn.lock().await;
        crate::storage::accounts::upsert(
            &c,
            &crate::storage::accounts::Account {
                user_id: "u".into(), device_id: "this-phone".into(),
                device_label: "this phone".into(), server_url: "https://srv".into(),
                last_seen_seq: 0, created_at: 1, username: None, last_contact_at: None,
            },
        )
        .unwrap();
    }

    /// Capture, exactly as the facade does it: the Entry and the act together,
    /// with the act naming the row. Hands back the row's `local_id`.
    async fn queue_capture(conn: &Arc<Mutex<Connection>>, text: &str) -> i64 {
        let sealed = crate::crypto::encrypt(&crate::testing::test_user_key(), "u", text.as_bytes()).unwrap();
        let hash = crate::storage::entries_cache::plaintext_sha256(text);
        let c = conn.lock().await;
        let local_id = crate::storage::entries_cache::insert_captured(
            &c, "u", &sealed, text, &hash, "this-phone",
        )
        .unwrap();
        pending::enqueue_capture(&c, "u", local_id, &sealed, &hash, 1).unwrap();
        local_id
    }

    /// What the cache holds for one Pairing, newest first.
    async fn cached(
        conn: &Arc<Mutex<Connection>>,
    ) -> Vec<crate::storage::entries_cache::CachedEntry> {
        let c = conn.lock().await;
        crate::storage::entries_cache::list_recent(&c, "u", None, 200).unwrap()
    }

    #[tokio::test]
    async fn flush_drains_in_fifo_order() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        for i in 0..3i64 {
            queue_capture(&conn, &format!("t{i}")).await;
        }
        let transport = Arc::new(OkTransport::new());
        let (up, sink) = uploader(conn.clone(), transport.clone());
        up.flush_once().await.unwrap();
        assert_eq!(transport.count.load(Ordering::SeqCst), 3);
        {
            let c = conn.lock().await;
            assert_eq!(pending::count(&c, "u").unwrap(), 0);
        }
        assert_eq!(
            sink.pending_counts(),
            vec![2, 1, 0],
            "the queue shrinking is the only thing that surfaces Pending draining"
        );
        assert!(
            sink.entries().is_empty(),
            "a flush creates nothing, so it announces nothing"
        );
    }

    /*
     * The mid-flight withdrawal. `flush_once` awaits the upload with the database
     * lock released, and a delete inside that window takes the queued act with
     * it. The relay has the act by then, so the only honest outcome is to take it
     * back off the relay — reconciling would attach a relay id to a row somebody
     * deleted, or re-create it.
     *
     * Driven against a transport that blocks rather than by timing luck: the
     * delete happens while the flush is provably inside its `await`.
     */
    #[tokio::test]
    async fn an_act_withdrawn_during_its_upload_is_taken_back_off_the_relay() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        paired(&conn).await;
        let local_id = queue_capture(&conn, "withdrawn mid-flight").await;

        let transport = Arc::new(BlockingRelay::new());
        let (up, _sink) = uploader(conn.clone(), transport.clone());
        let flush = tokio::spawn(async move { up.flush_once().await });

        transport.wait_until_parked().await;
        // The upload is now inside `transport.upload`. Withdraw the act and its
        // row exactly as `delete_entry` does.
        {
            let c = conn.lock().await;
            assert_eq!(
                crate::storage::entries_cache::delete_one(&c, "u", local_id).unwrap(),
                1
            );
            assert_eq!(pending::delete_for_entry(&c, "u", local_id).unwrap(), 1);
        }
        transport.release.add_permits(1);
        flush.await.unwrap().unwrap();

        assert_eq!(
            *transport.deleted.lock().await,
            vec![500],
            "the relay took the act, so the relay has to be told it is unwanted"
        );
        assert!(cached(&conn).await.is_empty(), "and nothing came back locally");
        let c = conn.lock().await;
        assert_eq!(pending::count(&c, "u").unwrap(), 0);
    }

    /// The reason there is one queue: a use made during an outage reaches the
    /// relay in the position it was made in, between the two captures.
    #[tokio::test]
    async fn a_queued_use_is_sent_in_the_order_it_was_made() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        paired(&conn).await;
        queue_capture(&conn, "before").await;
        // Dated inside the 30-day window, or `upsert_and_prune` deletes it with
        // the very write that stores it.
        let captured_at = crate::now_ms() - 60_000;
        let older = {
            let c = conn.lock().await;
            let stored = crate::storage::entries_cache::upsert_and_prune(
                &c,
                crate::storage::entries_cache::NewCachedEntry {
                    user_id: "u", relay_id: Some(7), ciphertext: b"ct", plaintext: Some("older"),
                    plaintext_sha256: None, created_at: captured_at, last_use: captured_at,
                    device_id: "d",
                },
                crate::now_ms(),
            )
            .unwrap();
            pending::enqueue_use(&c, "u", stored.local_id, 7, 2).unwrap();
            stored.local_id
        };
        queue_capture(&conn, "after").await;

        let transport = Arc::new(OkTransport::new());
        let (up, sink) = uploader(conn.clone(), transport.clone());
        up.flush_once().await.unwrap();

        assert_eq!(transport.count.load(Ordering::SeqCst), 2, "both captures uploaded");
        assert_eq!(*transport.uses.lock().await, vec![7], "and the use went with them");
        {
            let c = conn.lock().await;
            assert_eq!(pending::count(&c, "u").unwrap(), 0);
        }
        let used = cached(&conn)
            .await
            .into_iter()
            .find(|e| e.local_id == older)
            .expect("the used entry is still cached");
        assert!(
            used.last_use > captured_at,
            "the relay's stamp reached the cached row: {}",
            used.last_use
        );
        assert_eq!(used.created_at, captured_at, "and left its identity alone");
        assert!(
            sink.entries().is_empty(),
            "a flush creates nothing whichever kind of act it sends"
        );
    }

    /// A queued use whose entry is gone by the time the queue drains: there is
    /// nothing to reorder and nothing to tell anyone, so it leaves quietly.
    #[tokio::test]
    async fn a_queued_use_of_a_vanished_entry_is_acked_and_dropped() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        paired(&conn).await;
        {
            let c = conn.lock().await;
            pending::enqueue_use(&c, "u", 99, 7, 2).unwrap();
        }
        let (up, sink) = uploader(conn.clone(), Arc::new(OkTransport::without_the_entry()));
        up.flush_once().await.unwrap();

        let c = conn.lock().await;
        assert_eq!(pending::count(&c, "u").unwrap(), 0, "it must not retry forever");
        assert!(sink.entries().is_empty());
    }

    #[tokio::test]
    async fn auth_failure_propagates_and_keeps_row() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        queue_capture(&conn, "x").await;
        let (up, sink) = uploader(conn.clone(), Arc::new(AuthFail));
        let err = up.flush_once().await.unwrap_err();
        assert!(matches!(err, AppError::Auth(_)));
        let c = conn.lock().await;
        assert_eq!(pending::count(&c, "u").unwrap(), 1);
        let head = pending::head(&c, "u").unwrap().unwrap();
        assert_eq!(head.attempts, 1);
        assert!(sink.pending_counts().is_empty(), "nothing was uploaded, so nothing drained");
    }

    /*
     * A revoked token stops the uploader and is reported to the session rather
     * than to the sink: only the session can turn it into `AuthFailed` with the
     * Contact flush that state change owes the database.
     */
    #[tokio::test]
    async fn a_revoked_token_stops_the_uploader_and_is_reported_to_its_session() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        queue_capture(&conn, "x").await;
        let (up, sink) = uploader(conn.clone(), Arc::new(AuthFail));
        up.trigger.notify_one();
        assert_eq!(up.run(CancellationToken::new()).await, UploaderExit::AuthFailed);
        assert!(
            sink.events().is_empty(),
            "the uploader must not emit a connection state of its own"
        );
    }

    /*
     * An Entry exists from the moment of capture, so the flush has nothing to
     * insert: it hands the row the relay's id and leaves everything else about it
     * alone. The `local_id` in particular, which is what both shells key on —
     * a row that changed identity at the flush would remount and lose whatever
     * selection or keyboard cursor was on it.
     *
     * No SSE frame is delivered anywhere in this test. The Entry is on screen
     * from capture and owes the relay nothing to become real.
     */
    #[tokio::test]
    async fn a_flush_attaches_the_relays_id_without_moving_the_row() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        paired(&conn).await;
        let local_id = queue_capture(&conn, "copied on this phone").await;

        let before = cached(&conn).await;
        assert_eq!(before.len(), 1, "the Entry is cached from capture, not from the flush");
        assert_eq!(before[0].relay_id, None, "and the relay has not named it");
        assert_eq!(before[0].plaintext.as_deref(), Some("copied on this phone"));
        assert_eq!(
            before[0].device_id, "this-phone",
            "an Entry captured here has this device as its Origin"
        );

        let (up, sink) = uploader(conn.clone(), Arc::new(OkTransport::new()));
        up.flush_once().await.unwrap();

        let after = cached(&conn).await;
        assert_eq!(after.len(), 1, "reconciliation, not insertion");
        assert_eq!(after[0].local_id, local_id, "the row keeps the id the shells hold");
        assert_eq!(after[0].relay_id, Some(42), "and gains the one the relay assigned");
        assert!(after[0].last_use > 0, "and its Last Use");
        assert!(after[0].created_at > 0, "the relay's stamp replaced the un-stamped zero");
        assert_eq!(after[0].plaintext.as_deref(), Some("copied on this phone"));

        // And the watermark stays where it was. It means "everything up to here
        // has been fetched", and the relay may hold Entries from other devices
        // with lower sequences that this device has never seen: advancing past
        // them skips them for good, because the next `since=` fetch starts after
        // it.
        {
            let c = conn.lock().await;
            assert_eq!(
                crate::storage::accounts::find(&c, "u").unwrap().unwrap().last_seen_seq,
                0,
                "settling an act is not the same as having fetched everything before it"
            );
        }
        assert!(
            sink.entries().is_empty(),
            "the Entry was announced at capture; the flush announces nothing"
        );
    }

    /*
     * Anomaly A of `.scratch/mobile-client/issues/06`, reproduced twice on a
     * Windows smoke run: an offline burst of three flushes, `pending_uploads`
     * goes 3 -> 0, the relay gains three rows, and local history holds one.
     *
     * The cause was above the core (see that ticket's `## Answer`) and this is
     * the pin that keeps it there: three captures in, three rows out either side
     * of the flush, and the relay's echo of all three adding no fourth.
     */
    #[tokio::test]
    async fn an_offline_burst_of_three_leaves_three_rows_and_the_echo_adds_none() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        paired(&conn).await;
        let texts = ["offline-burst-one", "offline-burst-two", "offline-burst-three"];
        for t in texts {
            queue_capture(&conn, t).await;
        }

        let transport = Arc::new(EchoingRelay::default());
        let (up, sink) = uploader(conn.clone(), transport.clone());
        up.flush_once().await.unwrap();

        let accepted = transport.accepted.lock().await.clone();
        assert_eq!(accepted.len(), 3, "the relay took all three");
        {
            let c = conn.lock().await;
            assert_eq!(pending::count(&c, "u").unwrap(), 0, "the queue drained");
            let held: Vec<String> = crate::storage::entries_cache::list_recent(&c, "u", None, 10)
                .unwrap()
                .into_iter()
                .filter_map(|e| e.plaintext)
                .collect();
            assert_eq!(held.len(), 3, "every flushed capture is in local history: {held:?}");
            for t in texts {
                assert!(held.iter().any(|h| h == t), "{t} is missing: {held:?}");
            }
        }
        assert!(
            sink.entries().is_empty(),
            "the three were announced at capture; the flush announces nothing"
        );

        // The relay's fan-out of the same three, ingested exactly as
        // `run_sse_loop` ingests one.
        let key = crate::testing::test_user_key();
        let c = conn.lock().await;
        for (id, b64) in &accepted {
            let out = crate::sync::decryptor::ingest(
                &c,
                &key,
                "u",
                &EntryRow {
                    id: *id,
                    ciphertext: b64.clone(),
                    created_at: crate::now_ms(),
                    device_id: "this-phone".into(),
                    seq: *id,
                    last_use: crate::now_ms(),
                },
                crate::now_ms(),
            )
            .unwrap();
            assert!(
                !out.stored.first_insert,
                "the echo of entry {id} was treated as new, so the shell gets a duplicate row"
            );
        }
        assert_eq!(
            crate::storage::entries_cache::list_recent(&c, "u", None, 10).unwrap().len(),
            3,
            "the echo added a fourth row"
        );
    }

    /*
     * The relay echoes the Entry back a moment later and the session ingests the
     * same relay id. It has to resolve to the row the capture created and
     * announce nothing: a second `EntryAdded` is a duplicate row on screen, and a
     * second *row* is the same content twice. Both paths are driven here for one
     * entry.
     */
    #[tokio::test]
    async fn the_relays_echo_of_our_own_entry_does_not_add_it_twice() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        paired(&conn).await;
        let key = crate::testing::test_user_key();
        let sealed = crate::crypto::encrypt(&key, "u", b"offered here").unwrap();
        let local_id = queue_capture(&conn, "offered here").await;
        let (up, sink) = uploader(conn.clone(), Arc::new(OkTransport::new()));
        up.flush_once().await.unwrap();

        // The echo, ingested exactly as `run_sse_loop` ingests it.
        let c = conn.lock().await;
        let echoed = crate::sync::decryptor::ingest(
            &c,
            &key,
            "u",
            &EntryRow {
                id: 42,
                ciphertext: base64_encode(&sealed),
                created_at: crate::now_ms(),
                device_id: "this-phone".into(),
                seq: 42,
                last_use: crate::now_ms(),
            },
            crate::now_ms(),
        )
        .unwrap();

        assert!(
            !echoed.stored.first_insert,
            "the cache must report the echo as a repeat, or the session emits a second EntryAdded"
        );
        assert_eq!(
            echoed.stored.local_id, local_id,
            "and resolve to the row the capture created, not a new one"
        );
        assert!(
            sink.entries().is_empty(),
            "the Entry was announced at capture, and neither the flush nor the echo repeats it"
        );
        assert_eq!(
            crate::storage::entries_cache::list_recent(&c, "u", None, 10).unwrap().len(),
            1,
            "and exactly one row"
        );
    }

    #[tokio::test]
    async fn cancelling_a_session_stops_its_uploader() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        let (up, _sink) = uploader(conn, Arc::new(OkTransport::new()));
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert_eq!(up.run(cancel).await, UploaderExit::Cancelled);
    }
}
