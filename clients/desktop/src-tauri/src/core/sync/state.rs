// Implemented in Task 16.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Online,
    AuthFailed,
}

pub struct SyncTask;
