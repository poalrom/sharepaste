use crate::errors::AppError;
#[cfg(test)]
use parking_lot::Mutex;
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::Arc;

const SERVICE: &str = "sharepaste";

pub trait Keychain: Send + Sync {
    fn put(&self, account: &str, secret: &str) -> Result<(), AppError>;
    fn get(&self, account: &str) -> Result<Option<String>, AppError>;
    fn delete(&self, account: &str) -> Result<(), AppError>;
}

pub fn user_key_account(user_id: &str) -> String { format!("{user_id}:key") }
pub fn token_account(user_id: &str)    -> String { format!("{user_id}:token") }

#[derive(Default)]
pub struct SystemKeychain;

impl Keychain for SystemKeychain {
    fn put(&self, account: &str, secret: &str) -> Result<(), AppError> {
        let entry = keyring::Entry::new(SERVICE, account)?;
        entry.set_password(secret)?;
        Ok(())
    }

    fn get(&self, account: &str) -> Result<Option<String>, AppError> {
        let entry = keyring::Entry::new(SERVICE, account)?;
        match entry.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn delete(&self, account: &str) -> Result<(), AppError> {
        let entry = keyring::Entry::new(SERVICE, account)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
#[derive(Clone, Default)]
pub struct InMemoryKeychain {
    inner: Arc<Mutex<HashMap<String, String>>>,
}

#[cfg(test)]
impl Keychain for InMemoryKeychain {
    fn put(&self, account: &str, secret: &str) -> Result<(), AppError> {
        self.inner.lock().insert(account.into(), secret.into());
        Ok(())
    }
    fn get(&self, account: &str) -> Result<Option<String>, AppError> {
        Ok(self.inner.lock().get(account).cloned())
    }
    fn delete(&self, account: &str) -> Result<(), AppError> {
        self.inner.lock().remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_helpers_format_consistently() {
        assert_eq!(user_key_account("u-1"), "u-1:key");
        assert_eq!(token_account("u-1"), "u-1:token");
    }

    #[test]
    fn in_memory_keychain_round_trips() {
        let k = InMemoryKeychain::default();
        assert!(k.get("foo").unwrap().is_none());
        k.put("foo", "bar").unwrap();
        assert_eq!(k.get("foo").unwrap().as_deref(), Some("bar"));
        k.delete("foo").unwrap();
        assert!(k.get("foo").unwrap().is_none());
    }

    #[test]
    fn delete_missing_is_no_op() {
        let k = InMemoryKeychain::default();
        k.delete("absent").unwrap();
    }
}
