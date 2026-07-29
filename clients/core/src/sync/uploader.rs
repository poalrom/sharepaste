use crate::crypto::UserKey;
use crate::errors::AppError;
use crate::event::{CoreEvent, Entry};
use crate::http::dto::EntryRow;
use crate::pairing::payload::base64_encode;
use crate::platform::EventSink;
use crate::storage::{accounts, devices, pending};
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
/// number every other device will see it ordered and dated by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Uploaded {
    pub id: i64,
    pub created_at: i64,
}

#[async_trait]
pub trait UploadTransport: Send + Sync {
    async fn upload(&self, ciphertext_b64: &str) -> Result<Uploaded, AppError>;
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

    async fn flush_once(&self) -> Result<(), AppError> {
        loop {
            let head = {
                let conn = self.conn.lock().await;
                pending::head(&conn, &self.user_id)?
            };
            let Some(item) = head else { break; };
            let b64 = base64_encode(&item.ciphertext);
            match self.transport.upload(&b64).await {
                Ok(uploaded) => {
                    // Scoped so the database guard is gone before anything
                    // reaches the sink: see [`EventSink`].
                    let (count, cached) = {
                        let conn = self.conn.lock().await;
                        pending::ack(&conn, item.rowid)?;
                        let cached = self.cache_own_entry(&conn, &b64, uploaded);
                        (pending::count(&conn, &self.user_id)?, cached)
                    };
                    if let Some(entry) = cached {
                        self.events.emit(CoreEvent::EntryAdded {
                            user_id: self.user_id.clone(),
                            entry,
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
    /// **The watermark is deliberately not advanced.** `last_seen_id` means
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
        };
        match decryptor::ingest(conn, &self.user_key, &self.user_id, &row, crate::now_ms()) {
            // Only on a genuine first insert. The relay's echo ingests the same
            // id a moment later, and two `EntryAdded`s for one Entry is a
            // duplicate row on screen — a more visible bug than the missing row
            // this method exists to prevent.
            Ok(out) if out.first_insert => {
                let device_label =
                    devices::label_for(conn, &self.user_id, &account.device_id).unwrap_or_default();
                Some(Entry::new(
                    uploaded.id,
                    self.user_id.clone(),
                    out.plaintext,
                    uploaded.created_at,
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

    struct OkTransport { count: AtomicUsize }

    #[async_trait]
    impl UploadTransport for OkTransport {
        async fn upload(&self, _ct: &str) -> Result<Uploaded, AppError> {
            let n = self.count.fetch_add(1, Ordering::SeqCst) as i64;
            // `now_ms`, not a fixed date: `upsert_and_prune` drops anything older
            // than 30 days, so a cached Entry stamped in 2023 vanishes as it is written.
            Ok(Uploaded { id: 42 + n, created_at: crate::now_ms() + n })
        }
    }

    struct AuthFail;
    #[async_trait]
    impl UploadTransport for AuthFail {
        async fn upload(&self, _ct: &str) -> Result<Uploaded, AppError> {
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
                last_seen_id: 0, created_at: 1, username: None, last_contact_at: None,
            },
        )
        .unwrap();
    }

    #[tokio::test]
    async fn flush_drains_in_fifo_order() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        for i in 0..3i64 {
            let c = conn.lock().await;
            pending::enqueue(&c, "u", &[i as u8], i).unwrap();
        }
        let transport = Arc::new(OkTransport { count: AtomicUsize::new(0) });
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

    #[tokio::test]
    async fn auth_failure_propagates_and_keeps_row() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        {
            let c = conn.lock().await;
            pending::enqueue(&c, "u", b"x", 1).unwrap();
        }
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
        {
            let c = conn.lock().await;
            pending::enqueue(&c, "u", b"x", 1).unwrap();
        }
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
        let key = crate::testing::test_user_key();
        let sealed = crate::crypto::encrypt(&key, "u", b"copied on this phone").unwrap();
        {
            let c = conn.lock().await;
            pending::enqueue(&c, "u", &sealed, 1).unwrap();
        }

        let (up, sink) = uploader(conn.clone(), Arc::new(OkTransport { count: AtomicUsize::new(0) }));
        up.flush_once().await.unwrap();

        let c = conn.lock().await;
        let cached = crate::storage::entries_cache::list_recent(&c, "u", None, 10).unwrap();
        assert_eq!(cached.len(), 1, "the uploaded Entry is not in this device's own cache");
        assert_eq!(cached[0].id, 42, "cached under the id the relay assigned");
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
            crate::storage::accounts::find(&c, "u").unwrap().unwrap().last_seen_id,
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
        {
            let c = conn.lock().await;
            pending::enqueue(&c, "u", &sealed, 1).unwrap();
        }
        let (up, sink) = uploader(conn.clone(), Arc::new(OkTransport { count: AtomicUsize::new(0) }));
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
            },
            crate::now_ms(),
        )
        .unwrap();

        assert!(
            !echoed.first_insert,
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
        let (up, _sink) = uploader(conn, Arc::new(OkTransport { count: AtomicUsize::new(0) }));
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert_eq!(up.run(cancel).await, UploaderExit::Cancelled);
    }
}
