use crate::core::pairing::qr::base64_encode;
use crate::core::storage::pending;
use crate::errors::AppError;
use async_trait::async_trait;
use rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;

#[async_trait]
pub trait UploadTransport: Send + Sync {
    async fn upload(&self, ciphertext_b64: &str) -> Result<i64, AppError>;
}

pub struct UploaderEvents {
    pub on_pending_count: Box<dyn Fn(i64) + Send + Sync>,
    pub on_auth_failed: Box<dyn Fn() + Send + Sync>,
}

pub struct Uploader {
    pub user_id: String,
    pub conn: Arc<Mutex<Connection>>,
    pub transport: Arc<dyn UploadTransport>,
    pub trigger: Arc<Notify>,
    pub events: UploaderEvents,
}

impl Uploader {
    pub async fn run(self, cancel: CancellationToken) {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = self.trigger.notified() => {},
            }
            if let Err(e) = self.flush_once().await {
                if matches!(e, AppError::Auth(_)) {
                    (self.events.on_auth_failed)();
                    return;
                }
                tracing::warn!(err = %e, "uploader flush errored; will retry on next trigger");
            }
        }
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
                Ok(_id) => {
                    let conn = self.conn.lock().await;
                    pending::ack(&conn, item.rowid)?;
                    let count = pending::count(&conn, &self.user_id)?;
                    (self.events.on_pending_count)(count);
                }
                Err(AppError::Auth(s)) => {
                    let conn = self.conn.lock().await;
                    pending::record_failure(&conn, item.rowid, &s)?;
                    return Err(AppError::Auth(s));
                }
                Err(AppError::BadInput(s)) => {
                    let conn = self.conn.lock().await;
                    pending::ack(&conn, item.rowid)?;
                    tracing::warn!(err = %s, rowid = item.rowid, "dropped malformed pending entry");
                    let count = pending::count(&conn, &self.user_id)?;
                    (self.events.on_pending_count)(count);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::storage::{open_in_memory, pending};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct OkTransport { count: AtomicUsize }

    #[async_trait]
    impl UploadTransport for OkTransport {
        async fn upload(&self, _ct: &str) -> Result<i64, AppError> {
            self.count.fetch_add(1, Ordering::SeqCst);
            Ok(42)
        }
    }

    struct AuthFail;
    #[async_trait]
    impl UploadTransport for AuthFail {
        async fn upload(&self, _ct: &str) -> Result<i64, AppError> {
            Err(AppError::Auth("revoked".into()))
        }
    }

    fn events() -> (UploaderEvents, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let pc = Arc::new(AtomicUsize::new(0));
        let af = Arc::new(AtomicUsize::new(0));
        let pc2 = pc.clone();
        let af2 = af.clone();
        let ev = UploaderEvents {
            on_pending_count: Box::new(move |_| { pc2.fetch_add(1, Ordering::SeqCst); }),
            on_auth_failed: Box::new(move || { af2.fetch_add(1, Ordering::SeqCst); }),
        };
        (ev, pc, af)
    }

    #[tokio::test]
    async fn flush_drains_in_fifo_order() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        for i in 0..3i64 {
            let c = conn.lock().await;
            pending::enqueue(&c, "u", &[i as u8], i).unwrap();
        }
        let (ev, _pc, _af) = events();
        let transport = Arc::new(OkTransport { count: AtomicUsize::new(0) });
        let up = Uploader {
            user_id: "u".into(),
            conn: conn.clone(),
            transport: transport.clone(),
            trigger: Arc::new(Notify::new()),
            events: ev,
        };
        up.flush_once().await.unwrap();
        assert_eq!(transport.count.load(Ordering::SeqCst), 3);
        let c = conn.lock().await;
        assert_eq!(pending::count(&c, "u").unwrap(), 0);
    }

    #[tokio::test]
    async fn auth_failure_propagates_and_keeps_row() {
        let conn = Arc::new(Mutex::new(open_in_memory().unwrap()));
        {
            let c = conn.lock().await;
            pending::enqueue(&c, "u", b"x", 1).unwrap();
        }
        let (ev, _pc, _af) = events();
        let up = Uploader {
            user_id: "u".into(),
            conn: conn.clone(),
            transport: Arc::new(AuthFail),
            trigger: Arc::new(Notify::new()),
            events: ev,
        };
        let err = up.flush_once().await.unwrap_err();
        assert!(matches!(err, AppError::Auth(_)));
        let c = conn.lock().await;
        assert_eq!(pending::count(&c, "u").unwrap(), 1);
        let head = pending::head(&c, "u").unwrap().unwrap();
        assert_eq!(head.attempts, 1);
    }
}
