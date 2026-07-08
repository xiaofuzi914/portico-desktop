//! Secure storage for sensitive values such as API keys.
//!
//! The canonical implementation, [`KeyringSecretStore`], persists secrets in the
//! platform keychain (macOS Keychain on Apple platforms). Tests and other
//! non-production contexts can use [`InMemorySecretStore`].

use app_models::AppError;
use std::collections::HashMap;
use std::sync::Mutex;

/// Service name used for all Portico keychain entries.
const KEYCHAIN_SERVICE: &str = "portico-desktop";

/// Abstract storage for secrets keyed by an account identifier.
///
/// In Portico the account is the `api_key_reference` stored in the provider
/// configuration; the secret itself is never persisted in `SQLite` or logs.
pub trait SecretStore: Send + Sync {
    /// Retrieve the secret for `account`, returning `None` if it is not present.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying secure store cannot be accessed.
    fn get(&self, account: &str) -> Result<Option<String>, AppError>;

    /// Store `secret` under `account`.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying secure store cannot be written.
    fn set(&self, account: &str, secret: &str) -> Result<(), AppError>;

    /// Remove the secret stored under `account`.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying secure store cannot be accessed.
    fn delete(&self, account: &str) -> Result<(), AppError>;
}

/// [`SecretStore`] backed by the OS keychain via the `keyring` crate.
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyringSecretStore;

impl SecretStore for KeyringSecretStore {
    fn get(&self, account: &str) -> Result<Option<String>, AppError> {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|err| AppError::Internal {
            message: format!("failed to access keychain entry for '{account}': {err}"),
        })?;

        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(AppError::Internal {
                message: format!("failed to read secret for '{account}' from keychain: {err}"),
            }),
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), AppError> {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|err| AppError::Internal {
            message: format!("failed to access keychain entry for '{account}': {err}"),
        })?;

        entry.set_password(secret).map_err(|err| AppError::Internal {
            message: format!("failed to write secret for '{account}' to keychain: {err}"),
        })
    }

    fn delete(&self, account: &str) -> Result<(), AppError> {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|err| AppError::Internal {
            message: format!("failed to access keychain entry for '{account}': {err}"),
        })?;

        entry.delete_credential().map_err(|err| AppError::Internal {
            message: format!("failed to delete secret for '{account}' from keychain: {err}"),
        })
    }
}

/// In-memory [`SecretStore`] for tests and local debugging.
///
/// Secrets are held in a mutex-protected hash map; they are never written to
/// disk or to the OS keychain.
#[derive(Debug, Default)]
pub struct InMemorySecretStore {
    secrets: Mutex<HashMap<String, String>>,
}

impl InMemorySecretStore {
    /// Create a new empty in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl SecretStore for InMemorySecretStore {
    fn get(&self, account: &str) -> Result<Option<String>, AppError> {
        let secrets = self.secrets.lock().map_err(|err| AppError::Internal {
            message: format!("in-memory secret store lock poisoned: {err}"),
        })?;
        Ok(secrets.get(account).cloned())
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), AppError> {
        self.secrets
            .lock()
            .map_err(|err| AppError::Internal {
                message: format!("in-memory secret store lock poisoned: {err}"),
            })?
            .insert(account.to_owned(), secret.to_owned());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), AppError> {
        self.secrets
            .lock()
            .map_err(|err| AppError::Internal {
                message: format!("in-memory secret store lock poisoned: {err}"),
            })?
            .remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_store_roundtrips() {
        let store = InMemorySecretStore::new();
        assert_eq!(store.get("missing").unwrap(), None);

        store.set("account", "shhh").unwrap();
        assert_eq!(store.get("account").unwrap(), Some("shhh".to_owned()));

        store.delete("account").unwrap();
        assert_eq!(store.get("account").unwrap(), None);
    }

    #[test]
    fn in_memory_store_is_isolated_by_account() {
        let store = InMemorySecretStore::new();
        store.set("a", "one").unwrap();
        store.set("b", "two").unwrap();

        assert_eq!(store.get("a").unwrap(), Some("one".to_owned()));
        assert_eq!(store.get("b").unwrap(), Some("two".to_owned()));
    }
}
