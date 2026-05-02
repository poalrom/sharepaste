use crate::core::http::ServerClient;
use crate::errors::AppError;
use eventsource_stream::Eventsource;
use futures::stream::StreamExt;
use serde::Deserialize;
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

pub async fn run(
    server: ServerClient,
    sink: Sender<ServerEvent>,
    cancel: CancellationToken,
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
    let mut stream = resp.bytes_stream().eventsource();
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
}
