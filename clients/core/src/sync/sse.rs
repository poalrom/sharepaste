use crate::http::ServerClient;
use crate::errors::AppError;
use eventsource_stream::Eventsource;
use futures::stream::{Stream, StreamExt};
use serde::Deserialize;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ServerEvent {
    Entry {
        id: i64,
        ciphertext: String,
        created_at: i64,
        device_id: String,
    },
    Delete { id: i64 },
}

/// Stamp Contact for every chunk the relay delivers — invariant 3's tap half,
/// whose other half is `SessionCtx::set_conn_state`.
///
/// This wraps the byte stream **below** the SSE parser, and that placement is
/// the whole point. The relay writes `: heartbeat` every 15s, but a comment
/// line dispatches no event under the WHATWG rules `Eventsource` implements,
/// so above the parser a healthy idle stream looks identical to a dead one.
/// At the byte level every chunk is proof the connection is live, comments
/// included — and the `: connected` preamble stamps the instant it opens.
///
/// Cost while healthy is one relaxed store per heartbeat; nothing reaches the
/// database until the session goes offline.
fn stamp_contact<S, T, E>(stream: S, contact: Arc<AtomicI64>) -> impl Stream<Item = Result<T, E>> + Unpin
where
    S: Stream<Item = Result<T, E>> + Unpin,
{
    stream.inspect(move |chunk| {
        if chunk.is_ok() {
            contact.store(crate::now_ms(), Ordering::Relaxed);
        }
    })
}

pub async fn run(
    server: ServerClient,
    sink: Sender<ServerEvent>,
    cancel: CancellationToken,
    contact: Arc<AtomicI64>,
) -> Result<(), AppError> {
    let url = format!("{}/events", server.base().trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .read_timeout(Duration::from_secs(45))
        .build()
        .map_err(|e| AppError::Network(e.to_string()))?;
    let mut req = client.get(url);
    if let Some(t) = server.token() {
        req = req.bearer_auth(t);
    }
    let resp = req.send().await?;
    if !resp.status().is_success() {
        return Err(AppError::Network(format!("SSE status {}", resp.status())));
    }
    let mut stream = stamp_contact(resp.bytes_stream(), contact).eventsource();
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            ev = stream.next() => match ev {
                None => return Err(AppError::Network("SSE stream ended".into())),
                Some(Err(e)) => return Err(AppError::Network(format!("SSE error: {e}"))),
                Some(Ok(msg)) => {
                    if msg.event == "entry" || msg.event == "delete" {
                        let parsed: Result<ServerEvent, _> = serde_json::from_str(&msg.data);
                        match parsed {
                            Ok(p) => {
                                if sink.send(p).await.is_err() { return Ok(()); }
                            }
                            Err(e) => tracing::warn!(err = %e, "ignoring unparseable SSE payload"),
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_entry_event() {
        let data = json!({
            "type": "entry",
            "id": 7,
            "ciphertext": "AAAA",
            "created_at": 100,
            "device_id": "d1",
        });
        let parsed: ServerEvent = serde_json::from_value(data).unwrap();
        match parsed {
            ServerEvent::Entry { id, .. } => assert_eq!(id, 7),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parse_delete_event() {
        let data = json!({ "type": "delete", "id": 9 });
        let parsed: ServerEvent = serde_json::from_value(data).unwrap();
        match parsed {
            ServerEvent::Delete { id } => assert_eq!(id, 9),
            _ => panic!("wrong variant"),
        }
    }

    /// Drives [`stamp_contact`] over a canned stream — no relay, no socket.
    async fn drain(chunks: Vec<Result<&'static str, std::io::Error>>, cell: &Arc<AtomicI64>) -> usize {
        stamp_contact(futures::stream::iter(chunks), cell.clone())
            .filter(|c| futures::future::ready(c.is_ok()))
            .count()
            .await
    }

    #[tokio::test]
    async fn a_byte_from_the_relay_stamps_contact() {
        let cell = Arc::new(AtomicI64::new(0));
        let before = crate::now_ms();
        assert_eq!(drain(vec![Ok(": connected\n\n")], &cell).await, 1);
        let stamped = cell.load(Ordering::Relaxed);
        assert!(stamped >= before, "expected a stamp at or after {before}, got {stamped}");
    }

    #[tokio::test]
    async fn a_heartbeat_comment_stamps_contact_even_though_it_dispatches_no_event() {
        // The whole reason the tap sits below the parser: `: heartbeat` is a
        // comment, so `Eventsource` yields nothing for it.
        let cell = Arc::new(AtomicI64::new(0));
        let comment = ": heartbeat\n\n";
        assert_eq!(drain(vec![Ok(comment)], &cell).await, 1);
        assert_ne!(cell.load(Ordering::Relaxed), 0);

        let dispatched = futures::stream::iter(vec![Ok::<_, std::io::Error>(comment)])
            .eventsource()
            .count()
            .await;
        assert_eq!(dispatched, 0, "a comment must dispatch no event");
    }

    #[tokio::test]
    async fn a_stream_error_does_not_stamp_contact() {
        let cell = Arc::new(AtomicI64::new(0));
        let err = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset");
        assert_eq!(drain(vec![Err(err)], &cell).await, 0);
        assert_eq!(cell.load(Ordering::Relaxed), 0, "a failed read is not Contact");
    }

    #[tokio::test]
    async fn the_tap_passes_every_chunk_through_to_the_parser() {
        let cell = Arc::new(AtomicI64::new(0));
        let wire = "event: delete\ndata: {\"type\":\"delete\",\"id\":9}\n\n";
        let events: Vec<_> = stamp_contact(
            futures::stream::iter(vec![Ok::<_, std::io::Error>(wire)]),
            cell.clone(),
        )
        .eventsource()
        .collect()
        .await;
        let msg = events[0].as_ref().unwrap();
        assert_eq!(msg.event, "delete");
        assert_ne!(cell.load(Ordering::Relaxed), 0);
    }
}
