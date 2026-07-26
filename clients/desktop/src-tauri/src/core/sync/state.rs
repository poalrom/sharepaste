// Implemented in Task 16.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) enum ConnectionState {
    Disconnected,
    Connecting,
    Online,
    AuthFailed,
}

/// Whether a connection-state transition should flush Contact to the database.
///
/// True on the `Online` -> not-`Online` edge and nowhere else. While the
/// session is up the live cell absorbs one atomic store per relay heartbeat and
/// never touches SQLite; leaving the edge is the only moment the reading stops
/// moving and becomes worth keeping, so an outage costs exactly one write.
pub(crate) fn should_persist_contact(prev: Option<ConnectionState>, next: ConnectionState) -> bool {
    prev == Some(ConnectionState::Online) && next != ConnectionState::Online
}

pub(crate) struct BackoffPlan {
    schedule: &'static [u64],
    cap_secs: u64,
    cursor: usize,
}

impl BackoffPlan {
    pub(crate) fn new() -> Self {
        Self {
            schedule: &[1, 2, 4, 8, 16, 30],
            cap_secs: 30,
            cursor: 0,
        }
    }

    pub(crate) fn next_delay_secs(&mut self) -> u64 {
        let pick = if self.cursor >= self.schedule.len() {
            self.cap_secs
        } else {
            self.schedule[self.cursor]
        };
        self.cursor += 1;
        pick
    }

    pub(crate) fn reset(&mut self) {
        self.cursor = 0;
    }
}

impl Default for BackoffPlan {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_progresses_then_caps_at_30() {
        let mut b = BackoffPlan::new();
        assert_eq!(b.next_delay_secs(), 1);
        assert_eq!(b.next_delay_secs(), 2);
        assert_eq!(b.next_delay_secs(), 4);
        assert_eq!(b.next_delay_secs(), 8);
        assert_eq!(b.next_delay_secs(), 16);
        assert_eq!(b.next_delay_secs(), 30);
        assert_eq!(b.next_delay_secs(), 30);
        b.reset();
        assert_eq!(b.next_delay_secs(), 1);
    }

    #[test]
    fn contact_persists_only_when_leaving_online() {
        use ConnectionState::*;
        for next in [Connecting, Disconnected, AuthFailed] {
            assert!(should_persist_contact(Some(Online), next), "Online -> {next:?}");
        }
        // A heartbeat re-asserts Online; it must not reach the database.
        assert!(!should_persist_contact(Some(Online), Online));
        for prev in [None, Some(Connecting), Some(Disconnected), Some(AuthFailed)] {
            assert!(!should_persist_contact(prev, Disconnected), "{prev:?} -> Disconnected");
        }
    }
}
