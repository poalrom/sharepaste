use crate::errors::AppError;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

#[cfg(any(target_os = "macos", target_os = "windows"))]
const SERVICE: &str = "sharepaste";

pub trait Keychain: Send + Sync {
    fn put(&self, account: &str, secret: &str) -> Result<(), AppError>;
    fn get(&self, account: &str) -> Result<Option<String>, AppError>;
    fn delete(&self, account: &str) -> Result<(), AppError>;
}

pub fn user_key_account(user_id: &str) -> String { format!("{user_id}:key") }
pub fn token_account(user_id: &str)    -> String { format!("{user_id}:token") }

// `SystemKeychain` is the only construction site of a `Keychain`, which is what makes
// it cheap to confine to the targets that declare `keyring`; every other caller in the
// core goes through the trait object.
#[cfg(any(target_os = "macos", target_os = "windows"))]
#[derive(Default)]
pub struct SystemKeychain;

#[cfg(any(target_os = "macos", target_os = "windows"))]
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

// Shipped rather than `#[cfg(test)]`: it is the fallback `Keychain` for any target with
// no system keychain to gate `SystemKeychain` in, so it has to exist in a normal build.
// On the two desktop targets that do have one it is only ever built by tests, hence the
// narrowly-scoped allow — the alternative is a second fake written in the mobile crate.
#[cfg_attr(any(target_os = "macos", target_os = "windows"), allow(dead_code))]
#[derive(Clone, Default)]
pub struct InMemoryKeychain {
    inner: Arc<Mutex<HashMap<String, String>>>,
}

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
