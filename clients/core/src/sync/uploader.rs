use crate::crypto::UserKey;
use crate::errors::AppError;
use crate::event::{CoreEvent, Entry};
use crate::http::dto::EntryRow;
use crate::pairing::payload::base64_encode;
use crate::platform::EventSink;
use crate::storage::{accounts, devices, entries_cache, pending};
use crate::sync::decryptor;
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
    Capture { b64: String, uploaded: Uploaded },
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
    /// This pairing's key, to read back what was just sent.
    ///
    /// The uploader holds the ciphertext, not the plaintext — an Offer encrypts
    /// on the way into the queue — so caching the Entry means decrypting it
    /// again through the same path every other Entry takes. That is deliberate:
    /// one ingest function means the cache cannot end up holding a row that a
    /// relay-delivered Entry would have stored differently.
    pub user_key: UserKey,
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
    async fn flush_once(&self) -> Result<(), AppError> {
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
                        .map(|uploaded| Sent::Capture { b64, uploaded })
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
                    let (count, cached, reordered) = {
                        let conn = self.conn.lock().await;
                        pending::ack(&conn, item.rowid)?;
                        let (cached, reordered) = match sent {
                            Sent::Capture { b64, uploaded } => {
                                (self.cache_own_entry(&conn, &b64, uploaded), false)
                            }
                            Sent::Use { entry_id, used } => {
                                let moved = entries_cache::set_last_use(
                                    &conn, &self.user_id, entry_id, used.last_use,
                                )? > 0;
                                (None, moved)
                            }
                            Sent::UseVanished { entry_id } => {
                                tracing::info!(
                                    entry_id,
                                    "dropped a queued use of an entry the relay no longer has"
                                );
                                (None, false)
                            }
                        };
                        (pending::count(&conn, &self.user_id)?, cached, reordered)
                    };
                    if let Some(entry) = cached {
                        self.events.emit(CoreEvent::EntryAdded {
                            user_id: self.user_id.clone(),
                            entry,
                        });
                    }
                    // A use raises no `EntryAdded`: nothing was created, and the
                    // only thing that changed is where the row sits.
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

    /// Put an Entry this device just uploaded into its own cache, and hand back
    /// the Entry the caller owes the sink.
    ///
    /// **A device must not depend on a network echo to learn about content it
    /// created itself.** Before this, the only way an offered Entry reached the
    /// local cache was the relay's SSE fan-out — and the session nudges this
    /// uploader the moment it comes online, *before* the stream task has
    /// subscribed, so an Entry uploaded in that window is published to nobody
    /// and appears only if some later reconnect happens to re-run the backfill.
    /// That is the offline-burst-then-reconnect path, which is exactly when a
    /// person is most likely to be watching.
    ///
    /// **The watermark is deliberately not advanced.** `last_seen_seq` means
    /// "everything up to here has been fetched", and the relay may hold Entries
    /// from other devices with *lower* ids that this device has never seen —
    /// moving it past them skips them for good, because the next `since=` fetch
    /// starts after it. The next backfill re-fetches this id and advances the
    /// watermark the ordinary way; [`crate::storage::entries_cache::upsert_and_prune`]
    /// is idempotent, so ingesting it a second time costs nothing.
    ///
    /// **It returns the Entry rather than emitting it**, because its caller holds
    /// the database guard and [`EventSink`] forbids reaching a shell from under
    /// one. `Some` is the caller's instruction to announce it once the guard is
    /// gone.
    ///
    /// A failure here is logged and swallowed. The Entry is on the relay, which
    /// is the part that matters; the cache catches up on the next backfill.
    fn cache_own_entry(
        &self,
        conn: &Connection,
        ciphertext_b64: &str,
        uploaded: Uploaded,
    ) -> Option<Entry> {
        let Ok(Some(account)) = accounts::find(conn, &self.user_id) else {
            tracing::warn!(user_id = %self.user_id, "no pairing to attribute this device's own Entry to");
            return None;
        };
        let row = EntryRow {
            id: uploaded.id,
            ciphertext: ciphertext_b64.to_string(),
            created_at: uploaded.created_at,
            device_id: account.device_id.clone(),
            seq: uploaded.seq,
            last_use: uploaded.last_use,
        };
        match decryptor::ingest(conn, &self.user_key, &self.user_id, &row, crate::now_ms()) {
            // Only on a genuine first insert. The relay's echo ingests the same
            // id a moment later, and two `EntryAdded`s for one Entry is a
            // duplicate row on screen — a more visible bug than the missing row
            // this method exists to prevent.
            Ok(out) if out.stored.first_insert => {
                let device_label =
                    devices::label_for(conn, &self.user_id, &account.device_id).unwrap_or_default();
                Some(Entry::new(
                    uploaded.id,
                    self.user_id.clone(),
                    out.plaintext,
                    uploaded.created_at,
                    uploaded.last_use,
                    account.device_id,
                    device_label,
                ))
            }
            Ok(_) => None,
            Err(e) => {
                tracing::warn!(
                    err = %e, entry_id = uploaded.id,
                    "could not cache this device's own Entry; the next backfill will fetch it"
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{open_in_memory, pending};
    use crate::testing::RecordingSink;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct OkTransport { count: AtomicUsize, uses: Mutex<Vec<i64>>, entry_gone: bool }

    impl OkTransport {
        fn new() -> Self {
            OkTransport { count: AtomicUsize::new(0), uses: Mutex::new(Vec::new()), entry_gone: false }
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
            user_key: crate::testing::test_user_key(),
        };
        (up, sink)
    }

    /// A Pairing to attribute this device's own Entries to.
    ///
    /// `cache_own_entry` reads the Device id off it, so without one the uploader
    /// has nothing to record an Entry against and says so rather than guessing.
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

    /// Queue a capture, encrypting it as the facade does.
    async fn queue_capture(conn: &Arc<Mutex<Connection>>, text: &str) {
        let sealed = crate::crypto::encrypt(&crate::testing::test_user_key(), "u", text.as_bytes()).unwrap();
        let c = conn.lock().await;
        pending::enqueue_capture(
            &c, "u", &sealed,
            &crate::storage::entries_cache::plaintext_sha256(text),
            1,
        )
        .unwrap();
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
        let c = conn.lock().await;
        assert_eq!(pending::count(&c, "u").unwrap(), 0);
        assert_eq!(
            sink.pending_counts(),
            vec![2, 1, 0],
            "the queue shrinking is the only thing that surfaces Pending draining"
        );
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
        {
            let c = conn.lock().await;
            crate::storage::entries_cache::upsert_and_prune(
                &c,
                crate::storage::entries_cache::NewCachedEntry {
                    user_id: "u", relay_id: Some(7), ciphertext: b"ct", plaintext: Some("older"),
                    plaintext_sha256: None, created_at: captured_at, last_use: captured_at,
                    device_id: "d",
                },
                crate::now_ms(),
            )
            .unwrap();
            pending::enqueue_use(&c, "u", 7, 2).unwrap();
        }
        queue_capture(&conn, "after").await;

        let transport = Arc::new(OkTransport::new());
        let (up, sink) = uploader(conn.clone(), transport.clone());
        up.flush_once().await.unwrap();

        assert_eq!(transport.count.load(Ordering::SeqCst), 2, "both captures uploaded");
        assert_eq!(*transport.uses.lock().await, vec![7], "and the use went with them");
        let c = conn.lock().await;
        assert_eq!(pending::count(&c, "u").unwrap(), 0);
        let used = crate::storage::entries_cache::list_recent(&c, "u", None, 10)
            .unwrap()
            .into_iter()
            .find(|e| e.relay_id == Some(7))
            .expect("the used entry is still cached");
        assert!(
            used.last_use > captured_at,
            "the relay's stamp reached the cached row: {}",
            used.last_use
        );
        assert_eq!(used.created_at, captured_at, "and left its identity alone");
        assert_eq!(
            sink.entries().len(),
            2,
            "two captures are two EntryAddeds; a use creates nothing and announces none"
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
            pending::enqueue_use(&c, "u", 7, 2).unwrap();
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
     * The bug this defends: a device learning about its own content only from
     * the relay's echo. The session nudges the uploader the moment it comes
     * online, before the SSE task has subscribed, so an Entry uploaded in that
     * window was published to nobody and reached the cache only if some later
     * reconnect happened to re-run the backfill. Nothing forced one, and the
     * window is the offline-burst-then-reconnect path — precisely when somebody
     * is watching.
     *
     * No SSE frame is delivered anywhere in this test. That is the point.
     */
    #[tokio::test]
    async fn an_entry_this_device_uploaded_is_cached_without_any_relay_echo() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        paired(&conn).await;
        queue_capture(&conn, "copied on this phone").await;

        let (up, sink) = uploader(conn.clone(), Arc::new(OkTransport::new()));
        up.flush_once().await.unwrap();

        let c = conn.lock().await;
        let cached = crate::storage::entries_cache::list_recent(&c, "u", None, 10).unwrap();
        assert_eq!(cached.len(), 1, "the uploaded Entry is not in this device's own cache");
        assert_eq!(cached[0].relay_id, Some(42), "cached under the id the relay assigned");
        assert_eq!(cached[0].plaintext.as_deref(), Some("copied on this phone"));
        assert_eq!(
            cached[0].device_id, "this-phone",
            "an Entry captured here has this device as its Origin"
        );

        // And the watermark stays where it was. It means "everything up to here
        // has been fetched", and the relay may hold Entries from other devices
        // with lower ids that this device has never seen: advancing past them
        // skips them for good, because the next `since=` fetch starts after it.
        assert_eq!(
            crate::storage::accounts::find(&c, "u").unwrap().unwrap().last_seen_seq,
            0,
            "caching an Entry is not the same as having fetched everything before it"
        );

        assert_eq!(
            sink.entries().len(),
            1,
            "one Entry uploaded is one EntryAdded"
        );
    }

    /*
     * Anomaly A of `.scratch/mobile-client/issues/06`, reproduced twice on a
     * Windows smoke run: an offline burst of three flushes, `pending_uploads`
     * goes 3 -> 0, the relay gains three rows, and local history holds one.
     *
     * Driven through the uploader rather than by hand, because the claim is
     * about the ack-then-cache path and nothing else: three captures in, three
     * rows out, and the relay's echo of all three adding no fourth.
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
            let cached = crate::storage::entries_cache::list_recent(&c, "u", None, 10).unwrap();
            let held: Vec<&str> = cached.iter().filter_map(|e| e.plaintext.as_deref()).collect();
            assert_eq!(cached.len(), 3, "every flushed capture is in local history: {held:?}");
            for t in texts {
                assert!(held.contains(&t), "{t} is missing from local history: {held:?}");
            }
        }
        assert_eq!(sink.entries().len(), 3, "three captures are three EntryAddeds");

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
     * The other half, and the one that would turn this fix into a worse bug:
     * the relay echoes the Entry back a moment later, the session ingests the
     * same id, and a naive port emits a second `EntryAdded` — a duplicate row on
     * screen. Both paths are driven here for one id.
     */
    #[tokio::test]
    async fn the_relays_echo_of_our_own_entry_does_not_add_it_twice() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        paired(&conn).await;
        let key = crate::testing::test_user_key();
        let sealed = crate::crypto::encrypt(&key, "u", b"offered here").unwrap();
        queue_capture(&conn, "offered here").await;
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
            sink.entries().len(),
            1,
            "one Entry must produce exactly one EntryAdded however many paths deliver it"
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
