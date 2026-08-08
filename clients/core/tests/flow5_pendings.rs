mod common;

use sharepaste_core::event::Entry;
use sharepaste_core::facade::{
    OfferOutcome, RecallSource, Sharepaste, SharepasteConfig,
};
use sharepaste_core::http::TransportPolicy;
use sharepaste_core::keychain::InMemoryKeychain;
use sharepaste_core::relay::RelayDial;
use sharepaste_core::testing::{FakeClipboard, RecordingSink};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// This effort against the real wire.
///
/// Every unit test in the crate drives a relay that answers from a script, and a
/// script is written by whoever wrote the expectation. What this effort claims is
/// a claim about *two programs agreeing*: that the id the relay assigns lands on
/// the row this device already created, that the order the device showed while
/// the queue was full is the order the relay ends up holding, and that a payload
/// the relay turns down for its size comes back as something a person can see and
/// act on rather than as a line in a log. A fake cannot prove any of the three,
/// because a fake cannot disagree.
///
/// Driven entirely through the facade's own public operations, over a real
/// session — the uploader that flushes here is the one the app runs. Skips, with a
/// notice, when no relay answers: plain `cargo test` stays green on a clean
/// machine, exactly as the four flows before this one do.
struct Rig {
    core: Arc<Sharepaste>,
    sink: Arc<RecordingSink>,
    clipboard: Arc<FakeClipboard>,
    user_id: String,
}

impl Rig {
    /// A paired device over a real relay, with the three platform seams faked.
    ///
    /// Paired through the invite claim rather than by writing rows, so what the
    /// test drives afterwards is a pairing the app could actually be holding.
    ///
    /// Every operation below goes through [`Sharepaste::block_on`] from a plain
    /// `#[test]`, which is the shape this facade is built for: it owns its own
    /// runtime, and dropping one from inside an outer `#[tokio::test]` panics.
    fn paired(server: &common::TestServer, prefix: &str) -> Rig {
        let (_username, invite) = common::create_invite(server, prefix);
        let clipboard = FakeClipboard::new();
        let sink = Arc::new(RecordingSink::default());
        let core = Sharepaste::open_in_memory(SharepasteConfig {
            db_path: PathBuf::from("ignored-by-open_in_memory"),
            keychain: Arc::new(InMemoryKeychain::default()),
            clipboard: clipboard.clone(),
            events: sink.clone(),
            relay: RelayDial::over_http(TransportPolicy::AllowCleartext),
        })
        .unwrap();
        let paired = core
            .block_on(core.pair_with_invite(&server.url, &invite, "mac-smoke"))
            .unwrap();
        Rig { core, sink, clipboard, user_id: paired.user_id }
    }

    fn offer(&self, text: &str) -> OfferOutcome {
        self.core.block_on(self.core.offer(&self.user_id, text)).unwrap()
    }

    fn history(&self) -> Vec<Entry> {
        self.core
            .block_on(self.core.list_history(&self.user_id, None, 200))
            .unwrap()
    }

    /// How many acts this device still owes the relay, as a shell reads it.
    fn pending(&self) -> i64 {
        self.core
            .block_on(self.core.list_pairings())
            .unwrap()
            .into_iter()
            .find(|p| p.user_id == self.user_id)
            .expect("the pairing under test")
            .pending
    }

    /// Poll the facade until `pred` holds.
    ///
    /// The flush happens on the session's own uploader task, so the test watches
    /// rather than awaits — the same shape the facade's own tests use.
    fn until(&self, what: &str, pred: impl Fn(&[Entry], i64) -> bool) -> Vec<Entry> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let rows = self.history();
            let pending = self.pending();
            if pred(&rows, pending) {
                return rows;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}: {pending} pending, connection {:?}, rows {:?}",
                self.core.connection_state(&self.user_id),
                rows.iter()
                    .map(|e| (e.id, e.preview.as_str(), e.pending, e.created_at))
                    .collect::<Vec<_>>()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/*
 * ADR 0016 and ADR 0014 together, end to end.
 *
 * Three captures are made before any session exists, so nothing can flush them:
 * the uploader lives on the session (ADR 0007). Before the session comes up there
 * are three rows in the History that the relay has never heard of. After it, the
 * same three rows with the same three ids in the same three places, each now
 * carrying the stamp the relay applied on arrival.
 *
 * The order is the part a fake cannot prove. This device orders un-flushed acts by
 * queue position; the relay orders entries by a stamp it applies itself. Those are
 * two programs' numbers, and that they agree is a fact about the flush being FIFO
 * and the relay stamping monotonically — only observable against a real one.
 */
#[test]
fn an_offline_burst_reaches_the_relay_in_the_order_the_device_showed_it() {
    let Some(server) = common::start() else {
        return;
    };
    let rig = Rig::paired(&server, "burst");

    for text in ["first offline", "second offline", "third offline"] {
        assert!(matches!(
            rig.offer(text),
            OfferOutcome::Queued { .. }
        ));
    }

    let before = rig.history();
    assert_eq!(
        before.iter().map(|e| e.preview.as_str()).collect::<Vec<_>>(),
        vec!["third offline", "second offline", "first offline"],
        "an offline capture is a row from the moment it is made, newest act first"
    );
    assert!(
        before.iter().all(|e| e.pending && e.created_at == 0 && e.refused_reason.is_none()),
        "and the relay has stamped none of them: there is no device clock to stand in"
    );
    assert!(
        before.iter().all(|e| e.plaintext.is_some() && !e.undecryptable),
        "each carries its own plaintext, which is what makes it findable and recallable"
    );
    assert_eq!(rig.pending(), 3);
    assert_eq!(
        rig.sink.entry_previews(),
        vec!["first offline", "second offline", "third offline"],
        "each was announced once, at capture, in the order they were made"
    );

    // The real session, with the real uploader on it.
    rig.core.block_on(rig.core.start_session(&rig.user_id)).unwrap();
    let after = rig.until("the queue to drain", |rows, pending| {
        pending == 0 && rows.iter().all(|e| !e.pending)
    });
    rig.core.stop_session(&rig.user_id);

    assert_eq!(
        after.iter().map(|e| e.id).collect::<Vec<_>>(),
        before.iter().map(|e| e.id).collect::<Vec<_>>(),
        "the flush moved nothing and renamed nothing: same rows, same ids, same places"
    );
    assert_eq!(
        after.iter().map(|e| e.preview.as_str()).collect::<Vec<_>>(),
        vec!["third offline", "second offline", "first offline"],
        "which is the order the device was already showing"
    );
    assert!(
        after.iter().all(|e| e.created_at > 0 && e.last_use > 0),
        "and each row gained the stamp the relay applied, on the row it already had"
    );
    // The relay stamps on arrival, and the queue is FIFO, so its stamps have to
    // climb in the order the device drew. This is the two programs agreeing.
    let stamps: Vec<i64> = after.iter().map(|e| e.last_use).collect();
    assert!(
        stamps.windows(2).all(|w| w[0] >= w[1]),
        "the relay's own stamps descend down the list the device drew: {stamps:?}"
    );
    assert_eq!(
        rig.sink.entry_previews().len(),
        3,
        "the flush announced nothing new: it created nothing"
    );
    assert_eq!(rig.sink.settled().len(), 3, "one settlement per act");
    assert!(rig.sink.refusals().is_empty());

    // With nothing owed, the newest is an answer every device would give.
    let recalled = rig.core.block_on(rig.core.recall_latest(&rig.user_id)).unwrap();
    assert_eq!(recalled.text, "third offline");
    assert_eq!(recalled.source, RecallSource::Relay);
    assert_eq!(
        rig.clipboard.writes().last().map(String::as_str),
        Some("third offline")
    );
}

/*
 * ADR 0015 against the relay's actual limit.
 *
 * The 413 here is the relay's own, from its own configured cap, rather than an
 * `AppError::BadInput` a fake was told to produce — which is the only way to know
 * the client reads the status the relay really sends. The two caps are set to the
 * same number and measure different things: the capture filter bounds the
 * *plaintext* at 64 KiB and the relay bounds the *ciphertext*, which is the
 * plaintext plus a nonce and a tag. A capture of exactly 64 KiB is therefore
 * admissible here and refusable there, which is the window this exercises.
 */
#[test]
fn an_over_size_capture_is_refused_and_nothing_waits_behind_it() {
    let Some(server) = common::start() else {
        return;
    };
    let rig = Rig::paired(&server, "refused");

    let too_large = "z".repeat(64 * 1024);
    assert!(
        matches!(
            rig.offer(&too_large),
            OfferOutcome::Queued { .. }
        ),
        "the capture filter admits it: it is the ciphertext the relay measures"
    );
    rig.offer("queued behind it");
    assert_eq!(
        rig.pending(),
        2,
        "two offers are two acts: rows {:?}",
        rig.history().iter().map(|e| (e.id, e.preview.len())).collect::<Vec<_>>()
    );

    rig.core.block_on(rig.core.start_session(&rig.user_id)).unwrap();
    let rows = rig
        .until("the relay to turn the over-size act down", |rows, _| {
            rows.iter().any(|e| e.refused_reason.is_some())
                && rows.iter().any(|e| e.preview == "queued behind it" && !e.pending)
        });
    // Held open for a moment first. The uploader is nudged by every reconnect, so
    // a refusal that came back to the head would be re-offered and re-refused for
    // as long as the session lived — one `EntryRefused` per reconnect for a fact
    // that has not changed.
    let settled_refusals = rig.sink.refusals().len();
    std::thread::sleep(Duration::from_secs(3));
    assert_eq!(
        rig.sink.refusals().len(),
        settled_refusals,
        "a refusal is reported once and then left alone: {:?}",
        rig.sink.refusals()
    );
    rig.core.stop_session(&rig.user_id);

    let refused = rows.iter().find(|e| e.refused_reason.is_some()).unwrap();
    let reason = refused.refused_reason.clone().unwrap();
    assert!(
        reason.to_lowercase().contains("too large") || reason.contains("413"),
        "the row carries the relay's own words rather than a log line: {reason}"
    );
    assert!(refused.pending, "a refused act is still owed to the relay");
    assert_eq!(
        rows[0].id, refused.id,
        "and a refusal leads the History: it is the one asking for something"
    );
    assert_eq!(
        rig.core.block_on(rig.core.read_entry(&rig.user_id, refused.id)).unwrap().as_deref(),
        Some(too_large.as_str()),
        "its text is still readable here, which is why deleting it is not the only way out"
    );
    assert_eq!(
        rig.sink.refusals().len(),
        1,
        "one refusal reported, and nothing was deleted to make it: {:?}",
        rig.sink.refusals()
    );

    // Nothing waited behind it. `head` skips a refusal, so the act queued after
    // one is delivered rather than blocked on something waiting cannot fix.
    let behind = rows.iter().find(|e| e.preview == "queued behind it").unwrap();
    assert!(!behind.pending && behind.created_at > 0);

    // Resend is a fresh act: back of the queue, head of the History, nothing
    // carried forward. It will be refused again, which is honest.
    rig.core.block_on(rig.core.resend(&rig.user_id, refused.id)).unwrap();
    let requeued = rig.history();
    assert_eq!(requeued[0].id, refused.id);
    assert_eq!(
        requeued[0].refused_reason, None,
        "the reason is gone until the relay answers again"
    );
    assert!(requeued[0].pending);

    // And withdrawing it takes the row and the act together, with no relay call
    // to make: the relay never took this one.
    rig.core.block_on(rig.core.delete_entry(&rig.user_id, refused.id)).unwrap();
    let left = rig.history();
    assert!(left.iter().all(|e| e.id != refused.id), "the row is gone");
    assert_eq!(
        rig.pending(),
        0,
        "and so is the act, so nothing can put it back on the next flush"
    );
}
