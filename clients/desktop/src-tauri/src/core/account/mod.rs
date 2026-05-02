use crate::core::http::ServerClient;
use crate::core::keychain::{token_account, user_key_account, Keychain};
use crate::core::pairing::invite::hex::decode_user_key;
use crate::core::storage::accounts::{self, Account};
use crate::core::crypto::UserKey;
use crate::errors::AppError;
use parking_lot::RwLock;
use rusqlite::Connection;
use std::sync::Arc;

#[derive(Debug)]
pub struct ActiveMembership {
    pub account: Account,
    pub server: ServerClient,
    pub user_key: UserKey,
}

pub struct AccountRegistry {
    pub conn: Arc<tokio::sync::Mutex<Connection>>,
    pub keychain: Arc<dyn Keychain>,
    pub active: RwLock<Option<String>>,
}

impl AccountRegistry {
    pub fn new(conn: Arc<tokio::sync::Mutex<Connection>>, keychain: Arc<dyn Keychain>) -> Self {
        Self { conn, keychain, active: RwLock::new(None) }
    }

    pub async fn list(&self) -> Result<Vec<Account>, AppError> {
        let c = self.conn.lock().await;
        accounts::list(&c)
    }

    pub fn active_user_id(&self) -> Option<String> {
        self.active.read().clone()
    }

    pub fn set_active(&self, user_id: Option<String>) {
        *self.active.write() = user_id;
    }

    pub async fn load_active_membership(&self, user_id: &str) -> Result<ActiveMembership, AppError> {
        let acct = {
            let c = self.conn.lock().await;
            accounts::find(&c, user_id)?
                .ok_or_else(|| AppError::NotFound(format!("account {user_id}")))?
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
        let server = ServerClient::new(&acct.server_url)?.with_token(token);
        Ok(ActiveMembership { account: acct, server, user_key })
    }

    pub async fn forget(&self, user_id: &str) -> Result<(), AppError> {
        self.keychain.delete(&user_key_account(user_id))?;
        self.keychain.delete(&token_account(user_id))?;
        let c = self.conn.lock().await;
        crate::core::storage::entries_cache::delete_all(&c, user_id)?;
        accounts::delete(&c, user_id)?;
        if self.active.read().as_deref() == Some(user_id) {
            *self.active.write() = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::keychain::InMemoryKeychain;
    use crate::core::storage::open_in_memory;
    use std::sync::Arc;

    fn registry() -> AccountRegistry {
        let conn = Arc::new(tokio::sync::Mutex::new(open_in_memory().unwrap()));
        AccountRegistry::new(conn, Arc::new(InMemoryKeychain::default()))
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
                server_url: "https://srv".into(), last_seen_id: 0, created_at: 1,
            }).unwrap();
        }
        r.set_active(Some("u".into()));
        r.forget("u").await.unwrap();
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
                server_url: "https://srv".into(), last_seen_id: 0, created_at: 1,
            }).unwrap();
        }
        let err = r.load_active_membership("u").await.unwrap_err();
        assert!(matches!(err, AppError::Keychain(_)));
    }
}
