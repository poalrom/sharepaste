#![cfg(any(target_os = "macos", target_os = "windows"))]

use crate::errors::AppError;
use clipboard_master::{CallbackResult, ClipboardHandler, Master};
use std::thread;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

/// A bare notification that the OS clipboard changed; the payload is read from
/// the pasteboard on the receiving side, so the event itself carries nothing.
pub(crate) struct ClipboardEvent;

struct Handler {
    sink: Sender<ClipboardEvent>,
    cancel: CancellationToken,
}

impl ClipboardHandler for Handler {
    fn on_clipboard_change(&mut self) -> CallbackResult {
        if self.cancel.is_cancelled() {
            return CallbackResult::Stop;
        }
        let _ = self.sink.try_send(ClipboardEvent);
        CallbackResult::Next
    }

    fn on_clipboard_error(&mut self, error: std::io::Error) -> CallbackResult {
        tracing::warn!(?error, "clipboard-master error");
        CallbackResult::Next
    }
}

pub(crate) fn spawn(
    sink: Sender<ClipboardEvent>,
    cancel: CancellationToken,
) -> Result<thread::JoinHandle<()>, AppError> {
    let handle = thread::Builder::new()
        .name("clipboard-master".into())
        .spawn(move || {
            let mut master = match Master::new(Handler { sink, cancel }) {
                Ok(m) => m,
                Err(e) => {
                    tracing::error!(?e, "clipboard-master Master::new() failed");
                    return;
                }
            };
            if let Err(e) = master.run() {
                tracing::error!(?e, "clipboard-master master.run() exited");
            }
        })
        .map_err(|e| AppError::Storage(e.to_string()))?;
    Ok(handle)
}
