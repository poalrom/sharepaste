use crate::errors::AppError;
use crate::event::CoreEvent;
use crate::pairing::payload::base64_encode;
use crate::platform::EventSink;
use crate::relay::Relay;
use crate::storage::history::{self, ActKind, Taken};
use rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;

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
    /// The Relay this device's acts are owed to.
    pub relay: Arc<dyn Relay>,
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
                // A rejected token and an expired Pairing are both facts about
                // the Pairing rather than about the act, and both end the
                // session: nothing this uploader retries can fix either.
                if matches!(e, AppError::Auth(_) | AppError::PairExpired(_)) {
                    return UploaderExit::AuthFailed;
                }
                tracing::warn!(err = %e, "uploader flush errored; will retry on next trigger");
            }
        }
    }

    /// Drain the queue head-first, whatever kind each act is.
    ///
    /// One loop and one set of failure arms for both kinds, because the failure
    /// rules are the same rules: a rejected token or an expired Pairing stops
    /// everything, an act the relay refuses for what it *is* leaves the queue so
    /// nothing waits behind it, and anything else leaves the row where it is for
    /// the next trigger. What differs is only what the relay is asked to do and
    /// what the answer is recorded against.
    ///
    /// **`seq` is deliberately not applied.** `last_seen_seq` means "everything
    /// up to here has been fetched", and the relay may hold Entries from other
    /// devices with *lower* sequences that this device has never seen — moving
    /// the watermark past them skips them for good, because the next `since=`
    /// fetch starts after it. This device wrote; it did not fetch, so the answer
    /// the relay gives an act carries its stamps into the History and its `seq`
    /// nowhere.
    pub(crate) async fn flush_once(&self) -> Result<(), AppError> {
        loop {
            let act = {
                let conn = self.conn.lock().await;
                history::next_act(&conn, &self.user_id)?
            };
            let Some(act) = act else { break; };
            let sent = match &act.kind {
                ActKind::Capture(ciphertext) => {
                    let b64 = base64_encode(ciphertext);
                    self.relay.upload(&b64).await.map(|u| Taken::Capture {
                        relay_id: u.id,
                        created_at: u.created_at,
                        last_use: u.last_use,
                    })
                }
                ActKind::Use(relay_id) => {
                    let relay_id = *relay_id;
                    match self.relay.use_entry(relay_id).await {
                        Ok(used) => Ok(Taken::Use { relay_id, last_use: used.last_use }),
                        // The entry no longer exists, so there is nothing to
                        // reorder and nothing to tell anyone. Take the act off
                        // the queue and drop it.
                        Err(AppError::NotFound(_)) => Ok(Taken::UseVanished { relay_id }),
                        Err(e) => Err(e),
                    }
                }
            };
            match sent {
                Ok(taken) => {
                    // Scoped so the database guard is gone before anything
                    // reaches the sink: see [`EventSink`]. One value out of it
                    // and not four loose ones — what the relay stamped, what it
                    // took that nobody wants, and what the History now is.
                    let settled = {
                        let conn = self.conn.lock().await;
                        history::settle(&conn, &self.user_id, &act, taken, crate::now_ms())?
                    };
                    // Safe to ask for, because the upload that just returned is
                    // proof the relay is reachable. A failure here leaves one
                    // entry on the relay that nothing local refers to; the next
                    // backfill will bring it down, and it is a far smaller lie
                    // than a deleted row reappearing.
                    if let Some(relay_id) = settled.withdrawn {
                        tracing::info!(
                            relay_id,
                            "this act was withdrawn while it was uploading; deleting it again"
                        );
                        if let Err(e) = self.relay.delete_entry(relay_id).await {
                            tracing::warn!(
                                err = %e, relay_id,
                                "could not take back a withdrawn act the relay had already taken"
                            );
                        }
                    }
                    // One act left the queue, so its row may have stopped
                    // waiting. `EntrySettled` and not `HistoryChanged`: nothing
                    // reorders at a flush — the relay stamps a pending act exactly
                    // where this device already showed it — so a shell has one row
                    // to update in place and no reason to refetch a hundred.
                    //
                    // It carries what the relay just decided, because avoiding a
                    // refetch and withholding the answer are not the same economy:
                    // told only that the waiting is over, a shell drops the tint and
                    // goes on saying the relay has never stamped the row — the lie
                    // this effort removed, in the other direction, and for as long
                    // as nothing else happens to refetch.
                    //
                    // Nothing for a withdrawn act: its row is gone, and
                    // `EntryDeleted` already said so.
                    if let (Some(stamp), Some(entry_id)) = (settled.stamp, act.entry_id) {
                        self.events.emit(CoreEvent::EntrySettled {
                            user_id: self.user_id.clone(),
                            entry_id,
                            created_at: stamp.created_at,
                            last_use: stamp.last_use,
                        });
                    }
                    history::announce(self.events.as_ref(), &self.user_id, &settled.change);
                }
                // A fact about the Pairing rather than about the act: nothing this
                // uploader can retry will change either, and the session has to
                // come down and say so.
                Err(e @ (AppError::Auth(_) | AppError::PairExpired(_))) => {
                    let conn = self.conn.lock().await;
                    history::record_failure(&conn, &act, &e.to_string())?;
                    return Err(e);
                }
                // **Refused, not dropped.** 400 and 413 are facts about *this act*
                // and will be answered identically forever, so it leaves the
                // deliverable queue and stays on this device with its reason
                // (ADR 0015). This arm used to delete the row and write a warning
                // — clipboard content destroyed, and only the log told.
                Err(AppError::BadInput(reason)) => {
                    let refusal = {
                        let conn = self.conn.lock().await;
                        history::refuse(&conn, &self.user_id, &act, crate::now_ms(), &reason)?
                    };
                    // An unclaimed refusal means another uploader was told the
                    // same thing about the same act: a session being replaced
                    // leaves the outgoing one mid-flush while the incoming one
                    // starts, so two of them can read one head. One act earns one
                    // refusal.
                    match (refusal.claimed, act.entry_id) {
                        (true, Some(entry_id)) => self.events.emit(CoreEvent::EntryRefused {
                            user_id: self.user_id.clone(),
                            entry_id,
                            reason,
                        }),
                        (false, _) => tracing::info!(
                            entry_id = ?act.entry_id,
                            "this act had already been refused; the refusal is not repeated"
                        ),
                        (true, None) => {}
                    }
                    history::announce(self.events.as_ref(), &self.user_id, &refusal.change);
                }
                Err(e) => {
                    let conn = self.conn.lock().await;
                    history::record_failure(&conn, &act, &e.to_string())?;
                    return Err(e);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::AppError;
    use crate::http::dto::{ClaimInviteResp, DevicesResp, EntryRow, MeResp};
    use crate::pairing::payload::PairClaim;
    use crate::relay::{Uploaded, Used};
    use crate::storage::history::RelayEntry;
    use crate::storage::open_in_memory;
    use crate::sync::sse;
    use crate::testing::RecordingSink;
    use async_trait::async_trait;
    use std::sync::atomic::AtomicI64;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    /// What the relay under test does with whatever it is offered.
    #[derive(Clone, Copy)]
    enum Answer {
        /// Take it and stamp it, the way a reachable relay does.
        Takes,
        /// Answer one fixed error, whatever it is asked.
        Fails(fn() -> AppError),
        /// Park inside the upload until the test lets it out again.
        ///
        /// The only way to drive the withdrawal race deliberately rather than
        /// by timing luck: [`QueueRelay::wait_until_parked`] says the flush is
        /// provably inside its `await`, and `release` lets it finish once the
        /// test has withdrawn the act.
        Blocks,
    }

    /// The Relay a queue is drained over.
    ///
    /// One double for every test below, because the queue's rules do not vary
    /// with which relay is on the other end: what varies is the answer, and
    /// that is a field. It serves the three routes an uploader can reach and
    /// states as much for the rest — a flush that fetched, streamed or paired
    /// would be the defect, not the fixture.
    struct QueueRelay {
        answer: Answer,
        /// `(relay id, ciphertext)` in the order the relay took them.
        ///
        /// The ciphertext is kept because the relay's fan-out carries the same
        /// bytes back under the same id, and a test that re-encrypted them
        /// would be proving something about a nonce.
        accepted: Mutex<Vec<(i64, String)>>,
        uses: Mutex<Vec<i64>>,
        deleted: Mutex<Vec<i64>>,
        /// This relay no longer holds whatever entry a queued use names — the
        /// same answer a relay too old for the route gives.
        entry_gone: bool,
        parked: tokio::sync::Semaphore,
        release: tokio::sync::Semaphore,
    }

    impl QueueRelay {
        fn with(answer: Answer) -> Arc<Self> {
            Arc::new(Self::unwrapped(answer))
        }

        fn unwrapped(answer: Answer) -> Self {
            QueueRelay {
                answer,
                accepted: Mutex::new(Vec::new()),
                uses: Mutex::new(Vec::new()),
                deleted: Mutex::new(Vec::new()),
                entry_gone: false,
                parked: tokio::sync::Semaphore::new(0),
                release: tokio::sync::Semaphore::new(0),
            }
        }

        fn taking() -> Arc<Self> {
            Self::with(Answer::Takes)
        }

        fn failing(err: fn() -> AppError) -> Arc<Self> {
            Self::with(Answer::Fails(err))
        }

        fn blocking() -> Arc<Self> {
            Self::with(Answer::Blocks)
        }

        fn without_the_entry() -> Arc<Self> {
            let mut relay = QueueRelay::unwrapped(Answer::Takes);
            relay.entry_gone = true;
            Arc::new(relay)
        }

        /// How many acts this relay has taken.
        async fn uploads(&self) -> usize {
            self.accepted.lock().await.len()
        }

        /// Wait until an upload is inside the await, and nothing else.
        async fn wait_until_parked(&self) {
            self.parked.acquire().await.expect("the upload parks").forget();
        }
    }

    #[async_trait]
    impl Relay for QueueRelay {
        fn base_url(&self) -> String {
            "https://queue.invalid".to_string()
        }

        async fn upload(&self, ct: &str) -> Result<Uploaded, AppError> {
            match self.answer {
                Answer::Takes => {}
                Answer::Fails(err) => return Err(err()),
                Answer::Blocks => {
                    self.parked.add_permits(1);
                    self.release.acquire().await.expect("the test releases the upload").forget();
                }
            }
            let mut accepted = self.accepted.lock().await;
            let n = accepted.len() as i64;
            let id = 42 + n;
            // `now_ms`, not a fixed date: the History prunes anything older than
            // 30 days, so a cached Entry stamped in 2023 vanishes as it is written.
            let at = crate::now_ms() + n;
            accepted.push((id, ct.to_string()));
            Ok(Uploaded { id, created_at: at, seq: id, last_use: at })
        }

        async fn use_entry(&self, entry_id: i64) -> Result<Used, AppError> {
            self.uses.lock().await.push(entry_id);
            if let Answer::Fails(err) = self.answer {
                return Err(err());
            }
            if self.entry_gone {
                return Err(AppError::NotFound("entry not found".into()));
            }
            Ok(Used { seq: 900, last_use: crate::now_ms() })
        }

        async fn delete_entry(&self, entry_id: i64) -> Result<(), AppError> {
            self.deleted.lock().await.push(entry_id);
            if let Answer::Fails(err) = self.answer {
                return Err(err());
            }
            Ok(())
        }

        // The rest of the seam belongs to a session or to a handshake. Nothing a
        // flush does can reach one, so each says so rather than answering.
        async fn list_entries(&self, _since: i64, _limit: u32) -> Result<Vec<EntryRow>, AppError> {
            unreachable!("a flush never fetches")
        }

        async fn me(&self) -> Result<MeResp, AppError> {
            unreachable!("a flush never mirrors devices")
        }

        async fn stream(
            &self,
            _sink: mpsc::Sender<sse::ServerEvent>,
            _cancel: CancellationToken,
            _contact: Arc<AtomicI64>,
        ) -> Result<(), AppError> {
            unreachable!("a flush never streams")
        }

        async fn delete_all_entries(&self) -> Result<(), AppError> {
            unreachable!("a flush never clears the History")
        }

        async fn claim_invite(&self, _t: &str, _l: &str) -> Result<ClaimInviteResp, AppError> {
            unreachable!("a flush never pairs")
        }

        async fn pair_start(&self, _secret_hash: &str) -> Result<String, AppError> {
            unreachable!("a flush never pairs")
        }

        async fn pair_payload_put(&self, _id: &str, _payload: &str) -> Result<(), AppError> {
            unreachable!("a flush never pairs")
        }

        async fn pair_poll(&self, _id: &str, _timeout_ms: u32) -> Result<PairClaim, AppError> {
            unreachable!("a flush never pairs")
        }

        async fn pair_claim(&self, _id: &str, _proof: &str) -> Result<(), AppError> {
            unreachable!("a flush never pairs")
        }

        async fn pair_payload(&self, _id: &str, _proof: &str) -> Result<String, AppError> {
            unreachable!("a flush never pairs")
        }

        async fn pair_devices(
            &self,
            _id: &str,
            _proof: &str,
            _label: &str,
        ) -> Result<DevicesResp, AppError> {
            unreachable!("a flush never pairs")
        }
    }

    fn uploader(
        conn: Arc<Mutex<Connection>>,
        relay: Arc<dyn Relay>,
    ) -> (Uploader, Arc<RecordingSink>) {
        let sink = Arc::new(RecordingSink::default());
        let up = Uploader {
            user_id: "u".into(),
            conn,
            relay,
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

    /// A capture, through the one door there is. Answers this device's id for
    /// the Entry it made.
    ///
    /// Real ciphertext because the relay's echo is decrypted downstream, and
    /// because the queue is what the uploader actually sends.
    async fn copied(conn: &Arc<Mutex<Connection>>, text: &str) -> i64 {
        let sealed =
            crate::crypto::encrypt(&crate::testing::test_user_key(), "u", text.as_bytes()).unwrap();
        let c = conn.lock().await;
        history::capture(&c, "u", &sealed, text, "this-phone", 1).unwrap().local_id
    }

    /// The History for one Pairing, newest first.
    async fn cached(conn: &Arc<Mutex<Connection>>) -> Vec<history::CachedEntry> {
        let c = conn.lock().await;
        history::page(&c, "u", None, 200).unwrap()
    }

    /// The text of whatever the relay would be offered next — asked of the
    /// module rather than decrypted out of the queue.
    async fn next_text(conn: &Arc<Mutex<Connection>>) -> Option<String> {
        let c = conn.lock().await;
        let act = history::next_act(&c, "u").unwrap()?;
        history::plaintext_of(&c, "u", act.entry_id?).unwrap()
    }

    #[tokio::test]
    async fn flush_drains_in_fifo_order() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        for i in 0..3i64 {
            copied(&conn, &format!("t{i}")).await;
        }
        let relay = QueueRelay::taking();
        let (up, sink) = uploader(conn.clone(), relay.clone());
        up.flush_once().await.unwrap();
        assert_eq!(relay.uploads().await, 3);
        {
            let c = conn.lock().await;
            assert_eq!(history::depth(&c, "u").unwrap(), 0);
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
     * ADR 0015. A 413 is a fact about *this act* and will be answered identically
     * forever, so the act leaves the deliverable queue and stays on this device
     * with its reason. This arm used to delete the row and write a warning: a
     * person's clipboard content destroyed, and only the log told.
     *
     * And nothing waits behind it. `head` skips a refusal, so the act queued after
     * one is delivered rather than blocked on something waiting cannot fix.
     */
    #[tokio::test]
    async fn a_refused_act_keeps_its_row_and_lets_the_queue_past_it() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        paired(&conn).await;
        let refused = copied(&conn, "too large for this relay").await;
        let behind = copied(&conn, "queued behind it").await;

        // First flush: the relay refuses everything it is offered.
        let (up, sink) = uploader(
            conn.clone(),
            QueueRelay::failing(|| AppError::BadInput("payload too large".into())),
        );
        up.flush_once().await.unwrap();

        {
            let c = conn.lock().await;
            assert_eq!(
                history::depth(&c, "u").unwrap(),
                2,
                "nothing is deleted: both acts are still on this device"
            );
            assert!(
                history::next_act(&c, "u").unwrap().is_none(),
                "and neither is deliverable, because this relay refused both"
            );
        }
        assert_eq!(
            sink.refusals(),
            vec![
                (refused, "payload too large".to_string()),
                (behind, "payload too large".to_string()),
            ],
            "one refusal per refused act, each carrying what the relay said"
        );
        assert!(sink.settled().is_empty(), "and nothing settled");
        assert!(!sink.saw_history_changed("u"), "nor did anything reorder");
        assert_eq!(cached(&conn).await.len(), 2, "both rows are still here");
    }

    /*
     * The half a live relay caught: a refusal must not be offered again on the
     * *next* trigger either. The uploader is nudged by every reconnect, so a
     * refusal that came back to the head would be re-offered — and re-refused —
     * for as long as the session lived, which is one `EntryRefused` per reconnect
     * for a fact that has not changed.
     */
    #[tokio::test]
    async fn a_refusal_is_not_offered_again_on_the_next_flush() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        paired(&conn).await;
        let refused = copied(&conn, "too large for this relay").await;
        let taking = QueueRelay::taking();
        let refusing = QueueRelay::failing(|| AppError::BadInput("payload too large".into()));

        let (up, sink) = uploader(conn.clone(), refusing);
        up.flush_once().await.unwrap();
        assert_eq!(sink.refusals(), vec![(refused, "payload too large".to_string())]);

        // The next trigger, over a relay that would take anything it was offered.
        // It must be offered nothing.
        let (up, later) = uploader(conn.clone(), taking.clone());
        up.flush_once().await.unwrap();

        assert_eq!(
            taking.uploads().await,
            0,
            "a refused act is not deliverable, so nothing was sent"
        );
        assert!(later.refusals().is_empty(), "and nothing was refused a second time");
        let c = conn.lock().await;
        assert_eq!(history::depth(&c, "u").unwrap(), 1, "the act is still here");
    }

    /*
     * Two uploaders, one head. Found against a live relay, which is where it can
     * happen: `start_session` cancels the session it replaces, but a cancellation
     * is only noticed at the `select!`, so an outgoing uploader already inside
     * `flush_once` runs to the end of it while the incoming one starts. Both read
     * the same head, both are told 413, and the shell would be handed the same
     * refusal twice for one act.
     *
     * `refused_at` is what settles it: the second `refuse` claims nothing, and it
     * is the claim rather than the relay's answer that earns the event. The same
     * contract `ack`'s rowcount already carries for the withdrawal race.
     */
    #[tokio::test]
    async fn one_act_earns_one_refusal_however_many_uploaders_are_told() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        paired(&conn).await;
        let entry = copied(&conn, "too large for this relay").await;
        let refusing = || QueueRelay::failing(|| AppError::BadInput("payload too large".into()));

        // Both hold the head at the same moment, exactly as the two flushes do.
        let head = {
            let c = conn.lock().await;
            history::next_act(&c, "u").unwrap().unwrap()
        };
        let (outgoing, from_outgoing) = uploader(conn.clone(), refusing());
        let (incoming, from_incoming) = uploader(conn.clone(), refusing());
        let (a, b) = tokio::join!(outgoing.flush_once(), incoming.flush_once());
        a.unwrap();
        b.unwrap();

        let reported: Vec<(i64, String)> = from_outgoing
            .refusals()
            .into_iter()
            .chain(from_incoming.refusals())
            .collect();
        assert_eq!(
            reported,
            vec![(entry, "payload too large".to_string())],
            "one act, one refusal, whichever uploader claimed it"
        );
        let c = conn.lock().await;
        assert_eq!(history::depth(&c, "u").unwrap(), 1, "and the act is kept, once");
        assert!(
            history::next_act(&c, "u").unwrap().is_none(),
            "and is no longer deliverable"
        );
        assert!(
            !history::refuse(&c, "u", &head, 99, "again").unwrap().claimed,
            "a refusal is claimed once and only once"
        );
    }

    /*
     * The half that makes skipping a refusal safe: the act behind one is still
     * delivered. A refusal was never going to be delivered by waiting, so nothing
     * about the order is loosened by stepping over it.
     */
    #[tokio::test]
    async fn the_act_behind_a_refusal_is_still_delivered() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        paired(&conn).await;
        let refused = copied(&conn, "the relay will not take this").await;
        let behind = copied(&conn, "but it will take this").await;
        {
            let c = conn.lock().await;
            let head = history::next_act(&c, "u").unwrap().unwrap();
            assert_eq!(head.entry_id, Some(refused), "the oldest act is the head");
            history::refuse(&c, "u", &head, 1, "payload too large").unwrap();
        }

        let (up, sink) = uploader(conn.clone(), QueueRelay::taking());
        up.flush_once().await.unwrap();

        let c = conn.lock().await;
        assert_eq!(history::depth(&c, "u").unwrap(), 1, "the refusal is what is left");
        assert_eq!(sink.settled(), vec![behind]);
        let rows = history::page(&c, "u", None, 50).unwrap();
        assert_eq!(
            rows.iter().find(|r| r.local_id == behind).unwrap().relay_id,
            Some(42),
            "the act behind the refusal reached the relay"
        );
        assert_eq!(
            rows.iter().find(|r| r.local_id == refused).unwrap().relay_id,
            None,
            "and the refused one did not"
        );
    }

    /*
     * A 500 is not a refusal. The queue exists to survive a relay that is not
     * there, and a relay restarting mid-flush must not shred the ordering.
     */
    #[tokio::test]
    async fn a_server_error_leaves_the_act_queued_and_blocking() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        paired(&conn).await;
        copied(&conn, "first").await;
        copied(&conn, "second").await;

        let (up, sink) = uploader(
            conn.clone(),
            QueueRelay::failing(|| AppError::Network("status 500: restarting".into())),
        );
        let err = up.flush_once().await.unwrap_err();
        assert!(matches!(err, AppError::Network(_)), "got {err:?}");

        let c = conn.lock().await;
        assert_eq!(history::depth(&c, "u").unwrap(), 2);
        let head = history::next_act(&c, "u").unwrap().unwrap();
        drop(c);
        assert_eq!(
            next_text(&conn).await.as_deref(),
            Some("first"),
            "still at the head, still blocking"
        );
        assert_eq!(head.attempts, 1, "and counted as an attempt rather than a refusal");
        assert!(sink.refusals().is_empty());
    }

    /*
     * A 410 is a fact about the **Pairing**, not about the act. It used to fall
     * through to the generic retry arm and be attempted forever; it now brings the
     * session down, where the connection chrome can say what is wrong.
     */
    #[tokio::test]
    async fn an_expired_pairing_brings_the_session_down_and_refuses_nothing() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        paired(&conn).await;
        copied(&conn, "owed to a pairing that has expired").await;

        let (up, sink) = uploader(
            conn.clone(),
            QueueRelay::failing(|| AppError::PairExpired("gone".into())),
        );
        up.trigger.notify_one();
        assert_eq!(up.run(CancellationToken::new()).await, UploaderExit::AuthFailed);

        let c = conn.lock().await;
        assert_eq!(history::depth(&c, "u").unwrap(), 1, "the act is kept");
        assert!(
            history::next_act(&c, "u").unwrap().is_some(),
            "and stays deliverable: nothing about it was refused"
        );
        assert!(sink.refusals().is_empty());
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
        let local_id = copied(&conn, "withdrawn mid-flight").await;

        let relay = QueueRelay::blocking();
        let (up, _sink) = uploader(conn.clone(), relay.clone());
        let flush = tokio::spawn(async move { up.flush_once().await });

        relay.wait_until_parked().await;
        // The upload is now inside `QueueRelay::upload`. Withdraw the act and its
        // row exactly as `delete_entry` does.
        {
            let c = conn.lock().await;
            assert_eq!(
                history::forget_entry(&c, "u", local_id).unwrap().depth,
                Some(0),
                "the row and its act went together"
            );
        }
        relay.release.add_permits(1);
        flush.await.unwrap().unwrap();

        assert_eq!(
            *relay.deleted.lock().await,
            vec![42],
            "the relay took the act, so the relay has to be told it is unwanted"
        );
        assert!(cached(&conn).await.is_empty(), "and nothing came back locally");
        let c = conn.lock().await;
        assert_eq!(history::depth(&c, "u").unwrap(), 0);
    }

    /// The reason there is one queue: a use made during an outage reaches the
    /// relay in the position it was made in, between the two captures.
    #[tokio::test]
    async fn a_queued_use_is_sent_in_the_order_it_was_made() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        paired(&conn).await;
        copied(&conn, "before").await;
        // Dated inside the 30-day window, or the prune deletes it with the very
        // write that stores it.
        let captured_at = crate::now_ms() - 60_000;
        let older = {
            let c = conn.lock().await;
            let stored = history::store(
                &c,
                RelayEntry {
                    user_id: "u", relay_id: 7, ciphertext: b"ct", plaintext: Some("older"),
                    created_at: captured_at, last_use: captured_at, device_id: "d",
                },
                crate::now_ms(),
            )
            .unwrap();
            history::queue_use(&c, "u", stored.local_id, 7, 2).unwrap();
            stored.local_id
        };
        copied(&conn, "after").await;

        let relay = QueueRelay::taking();
        let (up, sink) = uploader(conn.clone(), relay.clone());
        up.flush_once().await.unwrap();

        assert_eq!(relay.uploads().await, 2, "both captures uploaded");
        assert_eq!(*relay.uses.lock().await, vec![7], "and the use went with them");
        {
            let c = conn.lock().await;
            assert_eq!(history::depth(&c, "u").unwrap(), 0);
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
            history::queue_use(&c, "u", 99, 7, 2).unwrap();
        }
        let (up, sink) = uploader(conn.clone(), QueueRelay::without_the_entry());
        up.flush_once().await.unwrap();

        let c = conn.lock().await;
        assert_eq!(history::depth(&c, "u").unwrap(), 0, "it must not retry forever");
        assert!(sink.entries().is_empty());
    }

    /*
     * What the settlement carries, per kind of act, because a shell draws the row
     * from it and there is nothing else coming: no refetch follows, so a number
     * missing here is a number the row goes on being wrong about.
     *
     * The three cases differ, and `None` is the relay saying nothing rather than
     * this device not knowing: a capture is stamped for the first time, a use moves
     * only the last use, and a use of an entry the relay has dropped moves neither
     * yet still takes the act out of the queue.
     */
    #[tokio::test]
    async fn a_settlement_carries_what_the_relay_stamped_and_no_more() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        paired(&conn).await;
        let local_id = copied(&conn, "captured offline").await;
        let (up, sink) = uploader(conn.clone(), QueueRelay::taking());
        up.flush_once().await.unwrap();
        // The relay's own clock, so the value is whatever it said — what matters is
        // that both numbers are present and are a date a row can be ordered by,
        // rather than the zero a never-stamped row carries.
        let settled = sink.settlements();
        assert_eq!(settled.len(), 1);
        let (id, created_at, last_use) = settled[0];
        assert_eq!(id, local_id);
        assert!(
            created_at.is_some_and(|c| c > 0) && created_at == last_use,
            "a capture is stamped for the first time, so both numbers are the relay's: {settled:?}"
        );

        {
            let c = conn.lock().await;
            history::queue_use(&c, "u", local_id, 7, 2).unwrap();
        }
        let (up, sink) = uploader(conn.clone(), QueueRelay::taking());
        up.flush_once().await.unwrap();
        let settled = sink.settlements();
        assert_eq!(settled.len(), 1);
        let (id, created_at, last_use) = settled[0];
        assert_eq!((id, created_at), (local_id, None), "a use does not restamp the creation");
        assert!(last_use.is_some_and(|u| u > 0), "and it does move the last use: {settled:?}");

        {
            let c = conn.lock().await;
            history::queue_use(&c, "u", local_id, 7, 2).unwrap();
        }
        let (up, sink) = uploader(conn.clone(), QueueRelay::without_the_entry());
        up.flush_once().await.unwrap();
        assert_eq!(
            sink.settlements(),
            vec![(local_id, None, None)],
            "a vanished use stamps nothing, and the row has still stopped waiting"
        );
    }

    #[tokio::test]
    async fn auth_failure_propagates_and_keeps_row() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        copied(&conn, "x").await;
        let (up, sink) = uploader(conn.clone(), QueueRelay::failing(|| AppError::Auth("revoked".into())));
        let err = up.flush_once().await.unwrap_err();
        assert!(matches!(err, AppError::Auth(_)));
        let c = conn.lock().await;
        assert_eq!(history::depth(&c, "u").unwrap(), 1);
        let head = history::next_act(&c, "u").unwrap().unwrap();
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
        copied(&conn, "x").await;
        let (up, sink) = uploader(conn.clone(), QueueRelay::failing(|| AppError::Auth("revoked".into())));
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
        let local_id = copied(&conn, "copied on this phone").await;

        let before = cached(&conn).await;
        assert_eq!(before.len(), 1, "the Entry is cached from capture, not from the flush");
        assert_eq!(before[0].relay_id, None, "and the relay has not named it");
        assert_eq!(before[0].plaintext.as_deref(), Some("copied on this phone"));
        assert_eq!(
            before[0].device_id, "this-phone",
            "an Entry captured here has this device as its Origin"
        );

        let (up, sink) = uploader(conn.clone(), QueueRelay::taking());
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
            copied(&conn, t).await;
        }

        let relay = QueueRelay::taking();
        let (up, sink) = uploader(conn.clone(), relay.clone());
        up.flush_once().await.unwrap();

        let accepted = relay.accepted.lock().await.clone();
        assert_eq!(accepted.len(), 3, "the relay took all three");
        {
            let c = conn.lock().await;
            assert_eq!(history::depth(&c, "u").unwrap(), 0, "the queue drained");
            let held: Vec<String> = history::page(&c, "u", None, 10)
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
            history::page(&c, "u", None, 10).unwrap().len(),
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
        let local_id = copied(&conn, "offered here").await;
        let (up, sink) = uploader(conn.clone(), QueueRelay::taking());
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
            history::page(&c, "u", None, 10).unwrap().len(),
            1,
            "and exactly one row"
        );
    }

    #[tokio::test]
    async fn cancelling_a_session_stops_its_uploader() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        let (up, _sink) = uploader(conn, QueueRelay::taking());
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert_eq!(up.run(cancel).await, UploaderExit::Cancelled);
    }
}
