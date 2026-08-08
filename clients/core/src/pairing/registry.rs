use crate::keychain::{token_account, user_key_account, Keychain};
use crate::pairing::invite::hex::decode_user_key;
use crate::relay::{Relay, RelayDial};
use crate::storage::accounts::{self, Account};
use crate::crypto::UserKey;
use crate::errors::AppError;
use parking_lot::RwLock;
use rusqlite::Connection;
use std::sync::Arc;

/// One Pairing, unlocked: the Relay it syncs to, as this device, and the key
/// its entries are sealed with.
pub struct ActiveMembership {
    pub relay: Arc<dyn Relay>,
    pub user_key: UserKey,
}

/// Names the relay and nothing else.
///
/// A derived `Debug` would print the user key, and a struct that formats itself
/// is one `tracing::debug!` away from putting a User's whole History in a log
/// file — the same reason `ShortCode` has none.
impl std::fmt::Debug for ActiveMembership {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActiveMembership")
            .field("relay", &self.relay.base_url())
            .finish_non_exhaustive()
    }
}

pub struct PairingRegistry {
    pub(crate) conn: Arc<tokio::sync::Mutex<Connection>>,
    pub(crate) keychain: Arc<dyn Keychain>,
    pub(crate) active: RwLock<Option<String>>,
    /// How a stored Pairing becomes a live Relay — the shell's own dial, shared
    /// with the facade.
    ///
    /// This is the half of the choke point nobody exercises by hand: a pairing
    /// stored while the shell permitted cleartext, resumed by a shell that does
    /// not, must fail here with [`AppError::InsecureRelay`] rather than limping
    /// on over plain HTTP or dying as an unexplained transport error.
    relay: RelayDial,
}

impl PairingRegistry {
    pub fn new(
        conn: Arc<tokio::sync::Mutex<Connection>>,
        keychain: Arc<dyn Keychain>,
        relay: RelayDial,
    ) -> Self {
        Self { conn, keychain, active: RwLock::new(None), relay }
    }

    pub async fn list(&self) -> Result<Vec<Account>, AppError> {
        let c = self.conn.lock().await;
        accounts::list(&c)
    }

    pub fn active_user_id(&self) -> Option<String> {
        self.active.read().clone()
    }

    /// Test-only: production always goes through [`Self::set_active_persisted`]
    /// so the choice survives a restart.
    #[cfg(test)]
    pub(crate) fn set_active(&self, user_id: Option<String>) {
        *self.active.write() = user_id;
    }

    pub(crate) fn set_active_persisted_with(
        &self,
        conn: &Connection,
        user_id: Option<String>,
    ) -> Result<(), AppError> {
        let mut s = crate::storage::settings::load(conn)?;
        s.last_active_user_id = user_id.clone();
        crate::storage::settings::save(conn, &s)?;
        *self.active.write() = user_id;
        Ok(())
    }

    pub async fn set_active_persisted(&self, user_id: Option<String>) -> Result<(), AppError> {
        let conn = self.conn.lock().await;
        self.set_active_persisted_with(&conn, user_id)
    }

    pub async fn load_persisted_active(&self) -> Result<Option<String>, AppError> {
        let conn = self.conn.lock().await;
        let s = crate::storage::settings::load(&conn)?;
        let Some(uid) = s.last_active_user_id else { return Ok(None) };
        match accounts::find(&conn, &uid)? {
            Some(_) => Ok(Some(uid)),
            None => Ok(None),
        }
    }

    pub async fn load_active_membership(&self, user_id: &str) -> Result<ActiveMembership, AppError> {
        let acct = {
            let c = self.conn.lock().await;
            accounts::find(&c, user_id)?
                .ok_or_else(|| AppError::NotFound(format!("pairing {user_id}")))?
        };
        let token = self
            .keychain
            .get(&token_account(user_id))?
            .ok_or_else(|| AppError::Keychain(format!("missing token for {user_id}")))?;
        let key_hex = self
            .keychain
            .get(&user_key_account(user_id))?
            .ok_or_else(|| AppError::Keychain(format!("missing user_key for {user_id}")))?;
        let user_key = decode_user_key(&key_hex)?;
        let relay = self.relay.at(&acct.server_url, Some(&token))?;
        Ok(ActiveMembership { relay, user_key })
    }

    pub async fn forget(&self, user_id: &str) -> Result<Option<String>, AppError> {
        self.keychain.delete(&user_key_account(user_id))?;
        self.keychain.delete(&token_account(user_id))?;
        let conn = self.conn.lock().await;
        crate::storage::history::forget_entries(&conn, user_id)?;
        crate::storage::devices::delete_all(&conn, user_id)?;
        accounts::delete(&conn, user_id)?;
        let was_active = self.active.read().as_deref() == Some(user_id);
        if !was_active {
            return Ok(None);
        }
        let next = accounts::list(&conn)?.into_iter().next().map(|a| a.user_id);
        if let Err(e) = self.set_active_persisted_with(&conn, next.clone()) {
            *self.active.write() = None;
            return Err(e);
        }
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::TransportPolicy;
    use crate::keychain::InMemoryKeychain;
    use crate::storage::open_in_memory;
    use std::sync::Arc;

    fn registry_with(policy: TransportPolicy) -> PairingRegistry {
        let conn = Arc::new(tokio::sync::Mutex::new(open_in_memory().unwrap()));
        PairingRegistry::new(
            conn,
            Arc::new(InMemoryKeychain::default()),
            RelayDial::over_http(policy),
        )
    }

    fn registry() -> PairingRegistry {
        registry_with(TransportPolicy::AllowCleartext)
    }

    /// Store a complete, unlockable pairing against `server_url`.
    async fn pairing_at(r: &PairingRegistry, server_url: &str) {
        r.keychain.put("u:key", &"ab".repeat(32)).unwrap();
        r.keychain.put("u:token", "tok").unwrap();
        let c = r.conn.lock().await;
        accounts::upsert(&c, &Account {
            user_id: "u".into(), device_id: "d".into(), device_label: "phone".into(),
            server_url: server_url.into(), last_seen_seq: 0, created_at: 1,
            username: None, last_contact_at: None,
        }).unwrap();
    }

    /// The half of the transport policy no one exercises by hand.
    ///
    /// Pairing is the visible path; *this* is every request after it. A pairing
    /// stored while cleartext was permitted, resumed under a shell that requires
    /// HTTPS, must be refused with the explanation and not with a bare transport
    /// error from somewhere inside the session loop.
    #[tokio::test]
    async fn a_session_request_to_a_cleartext_pairing_is_refused_under_require_https() {
        let r = registry_with(TransportPolicy::RequireHttps);
        pairing_at(&r, "http://relay.example:8443").await;
        let err = r.load_active_membership("u").await.unwrap_err();
        let AppError::InsecureRelay(detail) = err else {
            panic!("expected InsecureRelay from the session client, got {err:?}");
        };
        assert!(detail.contains("http://relay.example:8443"), "names the relay: {detail}");
    }

    #[tokio::test]
    async fn a_tls_pairing_unlocks_under_require_https() {
        let r = registry_with(TransportPolicy::RequireHttps);
        pairing_at(&r, "https://relay.example").await;
        let m = r.load_active_membership("u").await.unwrap();
        assert_eq!(m.relay.base_url(), "https://relay.example");
    }

    /// The desktop's reading of the same rows, unchanged.
    #[tokio::test]
    async fn a_cleartext_pairing_still_unlocks_when_the_shell_permits_it() {
        let r = registry();
        pairing_at(&r, "http://relay.example:8443").await;
        assert!(r.load_active_membership("u").await.is_ok());
    }

    #[tokio::test]
    async fn forget_clears_keychain_and_db_and_active() {
        let r = registry();
        let kc = r.keychain.clone();
        kc.put("u:key", &"ab".repeat(32)).unwrap();
        kc.put("u:token", "tok").unwrap();
        {
            let c = r.conn.lock().await;
            accounts::upsert(&c, &Account {
                user_id: "u".into(), device_id: "d".into(), device_label: "mac".into(),
                server_url: "https://srv".into(), last_seen_seq: 0, created_at: 1,
                username: None, last_contact_at: None,
            }).unwrap();
        }
        r.set_active(Some("u".into()));
        let new_active = r.forget("u").await.unwrap();
        assert!(new_active.is_none());
        assert!(kc.get("u:token").unwrap().is_none());
        assert!(r.active_user_id().is_none());
    }

    #[tokio::test]
    async fn load_active_membership_errors_on_missing_secret() {
        let r = registry();
        {
            let c = r.conn.lock().await;
            accounts::upsert(&c, &Account {
                user_id: "u".into(), device_id: "d".into(), device_label: "mac".into(),
                server_url: "https://srv".into(), last_seen_seq: 0, created_at: 1,
                username: None, last_contact_at: None,
            }).unwrap();
        }
        let err = r.load_active_membership("u").await.unwrap_err();
        assert!(matches!(err, AppError::Keychain(_)));
    }

    #[tokio::test]
    async fn set_active_persisted_writes_settings_row() {
        let r = registry();
        {
            let c = r.conn.lock().await;
            accounts::upsert(&c, &Account {
                user_id: "u".into(), device_id: "d".into(), device_label: "mac".into(),
                server_url: "https://srv".into(), last_seen_seq: 0, created_at: 1,
                username: None, last_contact_at: None,
            }).unwrap();
        }
        r.set_active_persisted(Some("u".into())).await.unwrap();
        let c = r.conn.lock().await;
        let s = crate::storage::settings::load(&c).unwrap();
        assert_eq!(s.last_active_user_id, Some("u".into()));
        assert_eq!(r.active_user_id(), Some("u".into()));
    }

    #[tokio::test]
    async fn set_active_persisted_none_clears_settings_row() {
        let r = registry();
        {
            let c = r.conn.lock().await;
            accounts::upsert(&c, &Account {
                user_id: "u".into(), device_id: "d".into(), device_label: "mac".into(),
                server_url: "https://srv".into(), last_seen_seq: 0, created_at: 1,
                username: None, last_contact_at: None,
            }).unwrap();
        }
        r.set_active_persisted(Some("u".into())).await.unwrap();
        r.set_active_persisted(None).await.unwrap();
        let c = r.conn.lock().await;
        let s = crate::storage::settings::load(&c).unwrap();
        assert!(s.last_active_user_id.is_none());
        assert!(r.active_user_id().is_none());
    }

    #[tokio::test]
    async fn load_persisted_active_returns_id_when_account_exists() {
        let r = registry();
        {
            let c = r.conn.lock().await;
            accounts::upsert(&c, &Account {
                user_id: "u".into(), device_id: "d".into(), device_label: "mac".into(),
                server_url: "https://srv".into(), last_seen_seq: 0, created_at: 1,
                username: None, last_contact_at: None,
            }).unwrap();
        }
        r.set_active_persisted(Some("u".into())).await.unwrap();
        assert_eq!(r.load_persisted_active().await.unwrap(), Some("u".into()));
    }

    #[tokio::test]
    async fn load_persisted_active_returns_none_when_account_missing() {
        let r = registry();
        {
            let c = r.conn.lock().await;
            let mut s = crate::storage::settings::Settings::default();
            s.last_active_user_id = Some("ghost".into());
            crate::storage::settings::save(&c, &s).unwrap();
        }
        assert!(r.load_persisted_active().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn forget_active_promotes_oldest_remaining() {
        let r = registry();
        let kc = r.keychain.clone();
        for (uid, created_at) in [("oldest", 1i64), ("middle", 2), ("newest", 3)] {
            kc.put(&format!("{uid}:key"), &"ab".repeat(32)).unwrap();
            kc.put(&format!("{uid}:token"), "tok").unwrap();
            let c = r.conn.lock().await;
            accounts::upsert(&c, &Account {
                user_id: uid.into(), device_id: "d".into(), device_label: uid.into(),
                server_url: "https://srv".into(), last_seen_seq: 0, created_at,
                username: None, last_contact_at: None,
            }).unwrap();
        }
        r.set_active_persisted(Some("middle".into())).await.unwrap();
        let new_active = r.forget("middle").await.unwrap();
        assert_eq!(new_active, Some("oldest".into()));
        assert_eq!(r.active_user_id(), Some("oldest".into()));
        let c = r.conn.lock().await;
        let s = crate::storage::settings::load(&c).unwrap();
        assert_eq!(s.last_active_user_id, Some("oldest".into()));
    }

    #[tokio::test]
    async fn forget_inactive_keeps_active_unchanged() {
        let r = registry();
        let kc = r.keychain.clone();
        for uid in ["a", "b"] {
            kc.put(&format!("{uid}:key"), &"ab".repeat(32)).unwrap();
            kc.put(&format!("{uid}:token"), "tok").unwrap();
            let c = r.conn.lock().await;
            accounts::upsert(&c, &Account {
                user_id: uid.into(), device_id: "d".into(), device_label: uid.into(),
                server_url: "https://srv".into(), last_seen_seq: 0, created_at: 1,
                username: None, last_contact_at: None,
            }).unwrap();
        }
        r.set_active_persisted(Some("a".into())).await.unwrap();
        let new_active = r.forget("b").await.unwrap();
        assert!(new_active.is_none());
        assert_eq!(r.active_user_id(), Some("a".into()));
        let c = r.conn.lock().await;
        let s = crate::storage::settings::load(&c).unwrap();
        assert_eq!(s.last_active_user_id, Some("a".into()));
    }

    #[tokio::test]
    async fn forget_only_active_account_clears_persisted_id() {
        let r = registry();
        let kc = r.keychain.clone();
        kc.put("u:key", &"ab".repeat(32)).unwrap();
        kc.put("u:token", "tok").unwrap();
        {
            let c = r.conn.lock().await;
            accounts::upsert(&c, &Account {
                user_id: "u".into(), device_id: "d".into(), device_label: "u".into(),
                server_url: "https://srv".into(), last_seen_seq: 0, created_at: 1,
                username: None, last_contact_at: None,
            }).unwrap();
        }
        r.set_active_persisted(Some("u".into())).await.unwrap();
        let new_active = r.forget("u").await.unwrap();
        assert!(new_active.is_none());
        assert!(r.active_user_id().is_none());
        let c = r.conn.lock().await;
        let s = crate::storage::settings::load(&c).unwrap();
        assert!(s.last_active_user_id.is_none());
    }
}
