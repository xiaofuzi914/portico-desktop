//! Secure storage for sensitive values such as API keys.
//!
//! Production path:
//! 1. [`LocalEncryptedSecretStore`] — AES-256-GCM ciphertext beside the app DB
//! 2. [`CachingSecretStore`] — process-memory cache after the first successful read
//! 3. [`LayeredSecretStore`] — migrates legacy Keychain entries once into the vault
//!
//! Keychain is only a **legacy import** source. After a secret is promoted into
//! the local vault, later runs do not call Keychain and therefore do not show
//! repeated macOS unlock dialogs.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use app_models::AppError;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Service name used for legacy Portico keychain entries.
const KEYCHAIN_SERVICE: &str = "portico-desktop";
const MASTER_KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
const VAULT_VERSION: u8 = 1;

/// Abstract storage for secrets keyed by an account identifier.
///
/// In Portico the account is the `api_key_reference` stored in the provider
/// configuration. Secret *values* are never logged.
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

/// Process-local cache around another [`SecretStore`].
///
/// Avoids repeated OS keychain / disk decrypt calls within a single app session.
#[derive(Debug)]
pub struct CachingSecretStore<S> {
    inner: S,
    cache: Mutex<HashMap<String, Option<String>>>,
}

impl<S> CachingSecretStore<S> {
    /// Wrap `inner` with an empty memory cache.
    #[must_use]
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Access the inner store (for tests / migration helpers).
    #[must_use]
    pub const fn inner(&self) -> &S {
        &self.inner
    }
}

impl<S: SecretStore> SecretStore for CachingSecretStore<S> {
    fn get(&self, account: &str) -> Result<Option<String>, AppError> {
        let cached = {
            let cache = self.cache.lock().map_err(|_| AppError::Internal {
                message: "secret cache lock poisoned".to_owned(),
            })?;
            cache.get(account).cloned()
        };
        if let Some(hit) = cached {
            return Ok(hit);
        }
        let value = self.inner.get(account)?;
        {
            let mut cache = self.cache.lock().map_err(|_| AppError::Internal {
                message: "secret cache lock poisoned".to_owned(),
            })?;
            cache.insert(account.to_owned(), value.clone());
        }
        Ok(value)
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), AppError> {
        self.inner.set(account, secret)?;
        self.cache
            .lock()
            .map_err(|_| AppError::Internal {
                message: "secret cache lock poisoned".to_owned(),
            })?
            .insert(account.to_owned(), Some(secret.to_owned()));
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), AppError> {
        self.inner.delete(account)?;
        self.cache
            .lock()
            .map_err(|_| AppError::Internal {
                message: "secret cache lock poisoned".to_owned(),
            })?
            .insert(account.to_owned(), None);
        Ok(())
    }
}

/// AES-256-GCM vault stored as one ciphertext file per account under `secrets_dir`.
///
/// The master key lives in `master_key_path` (created once, mode 0600 when the OS
/// supports it). This keeps daily secret access off the Keychain UI path.
#[derive(Debug)]
pub struct LocalEncryptedSecretStore {
    master_key: [u8; MASTER_KEY_BYTES],
    secrets_dir: PathBuf,
}

impl LocalEncryptedSecretStore {
    /// Open or create a vault rooted at `app_data_dir`.
    ///
    /// Layout:
    /// - `{app_data_dir}/secret_master.key`
    /// - `{app_data_dir}/secrets/<hex>.bin`
    ///
    /// # Errors
    ///
    /// Returns an error if directories cannot be created or the master key is corrupt.
    pub fn open(app_data_dir: impl AsRef<Path>) -> Result<Self, AppError> {
        let app_data_dir = app_data_dir.as_ref();
        fs::create_dir_all(app_data_dir).map_err(|err| AppError::Internal {
            message: format!("failed to create app data dir for secrets: {err}"),
        })?;
        let secrets_dir = app_data_dir.join("secrets");
        fs::create_dir_all(&secrets_dir).map_err(|err| AppError::Internal {
            message: format!("failed to create secrets directory: {err}"),
        })?;
        let master_key_path = app_data_dir.join("secret_master.key");
        let master_key = load_or_create_master_key(&master_key_path)?;
        Ok(Self {
            master_key,
            secrets_dir,
        })
    }

    fn entry_path(&self, account: &str) -> PathBuf {
        let digest = Sha256::digest(account.as_bytes());
        self.secrets_dir.join(format!("{}.bin", hex::encode(digest)))
    }

    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, AppError> {
        let cipher =
            Aes256Gcm::new_from_slice(&self.master_key).map_err(|err| AppError::Internal {
                message: format!("failed to init secret cipher: {err}"),
            })?;
        let mut nonce_bytes = [0u8; NONCE_BYTES];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher.encrypt(nonce, plaintext).map_err(|err| AppError::Internal {
            message: format!("failed to encrypt secret: {err}"),
        })?;
        let mut out = Vec::with_capacity(1 + NONCE_BYTES + ciphertext.len());
        out.push(VAULT_VERSION);
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    fn decrypt(&self, blob: &[u8]) -> Result<Vec<u8>, AppError> {
        if blob.len() < 1 + NONCE_BYTES + 1 {
            return Err(AppError::Internal {
                message: "secret vault entry is truncated".to_owned(),
            });
        }
        if blob[0] != VAULT_VERSION {
            return Err(AppError::Internal {
                message: format!("unsupported secret vault version {}", blob[0]),
            });
        }
        let nonce = Nonce::from_slice(&blob[1..=NONCE_BYTES]);
        let ciphertext = &blob[1 + NONCE_BYTES..];
        let cipher =
            Aes256Gcm::new_from_slice(&self.master_key).map_err(|err| AppError::Internal {
                message: format!("failed to init secret cipher: {err}"),
            })?;
        cipher.decrypt(nonce, ciphertext).map_err(|err| AppError::Internal {
            message: format!("failed to decrypt secret: {err}"),
        })
    }
}

impl SecretStore for LocalEncryptedSecretStore {
    fn get(&self, account: &str) -> Result<Option<String>, AppError> {
        let path = self.entry_path(account);
        if !path.exists() {
            return Ok(None);
        }
        let blob = fs::read(&path).map_err(|err| AppError::Internal {
            message: format!("failed to read secret vault entry: {err}"),
        })?;
        let plaintext = self.decrypt(&blob)?;
        let secret = String::from_utf8(plaintext).map_err(|err| AppError::Internal {
            message: format!("secret vault entry is not valid UTF-8: {err}"),
        })?;
        Ok(Some(secret))
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), AppError> {
        let path = self.entry_path(account);
        let blob = self.encrypt(secret.as_bytes())?;
        write_private_file(&path, &blob)
    }

    fn delete(&self, account: &str) -> Result<(), AppError> {
        let path = self.entry_path(account);
        if path.exists() {
            fs::remove_file(&path).map_err(|err| AppError::Internal {
                message: format!("failed to delete secret vault entry: {err}"),
            })?;
        }
        Ok(())
    }
}

/// Primary local vault with one-shot Keychain import for legacy installs.
///
/// Read order: local vault → legacy Keychain (then promote into vault).
/// Write order: local vault; best-effort Keychain delete to stop future prompts.
#[derive(Debug)]
pub struct LayeredSecretStore {
    primary: LocalEncryptedSecretStore,
    legacy: KeyringSecretStore,
}

impl LayeredSecretStore {
    /// Build the production layered store under `app_data_dir`.
    ///
    /// # Errors
    ///
    /// Returns an error if the encrypted vault cannot be opened.
    pub fn open(app_data_dir: impl AsRef<Path>) -> Result<Self, AppError> {
        Ok(Self {
            primary: LocalEncryptedSecretStore::open(app_data_dir)?,
            legacy: KeyringSecretStore,
        })
    }
}

impl SecretStore for LayeredSecretStore {
    fn get(&self, account: &str) -> Result<Option<String>, AppError> {
        if let Some(secret) = self.primary.get(account)? {
            return Ok(Some(secret));
        }
        // Legacy import: may show a one-time Keychain prompt for old installs.
        match self.legacy.get(account) {
            Ok(Some(secret)) => {
                // Promote into the local vault so subsequent runs never touch Keychain.
                if self.primary.set(account, &secret).is_ok() {
                    let _ = self.legacy.delete(account);
                }
                Ok(Some(secret))
            }
            // Missing entry, user denied, or Keychain unavailable → local vault is SoT.
            Ok(None) | Err(_) => Ok(None),
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), AppError> {
        self.primary.set(account, secret)?;
        let _ = self.legacy.delete(account);
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), AppError> {
        self.primary.delete(account)?;
        let _ = self.legacy.delete(account);
        Ok(())
    }
}

/// [`SecretStore`] backed by the OS keychain via the `keyring` crate.
///
/// Kept for legacy migration and tests. New production writes go through
/// [`LayeredSecretStore`] / [`LocalEncryptedSecretStore`].
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyringSecretStore;

impl SecretStore for KeyringSecretStore {
    fn get(&self, account: &str) -> Result<Option<String>, AppError> {
        let entry =
            keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|err| AppError::Internal {
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
        let entry =
            keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|err| AppError::Internal {
                message: format!("failed to access keychain entry for '{account}': {err}"),
            })?;

        entry.set_password(secret).map_err(|err| AppError::Internal {
            message: format!("failed to write secret for '{account}' to keychain: {err}"),
        })
    }

    fn delete(&self, account: &str) -> Result<(), AppError> {
        let entry =
            keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|err| AppError::Internal {
                message: format!("failed to access keychain entry for '{account}': {err}"),
            })?;

        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(AppError::Internal {
                message: format!("failed to delete secret for '{account}' from keychain: {err}"),
            }),
        }
    }
}

/// In-memory [`SecretStore`] for tests and local debugging.
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

fn load_or_create_master_key(path: &Path) -> Result<[u8; MASTER_KEY_BYTES], AppError> {
    if path.exists() {
        let bytes = fs::read(path).map_err(|err| AppError::Internal {
            message: format!("failed to read secret master key: {err}"),
        })?;
        if bytes.len() != MASTER_KEY_BYTES {
            return Err(AppError::Internal {
                message: format!(
                    "secret master key must be {MASTER_KEY_BYTES} bytes, found {}",
                    bytes.len()
                ),
            });
        }
        let mut key = [0u8; MASTER_KEY_BYTES];
        key.copy_from_slice(&bytes);
        return Ok(key);
    }

    let mut key = [0u8; MASTER_KEY_BYTES];
    rand::thread_rng().fill_bytes(&mut key);
    write_private_file(path, &key)?;
    Ok(key)
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| AppError::Internal {
            message: format!("failed to create secret parent directory: {err}"),
        })?;
    }
    let mut file =
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(|err| AppError::Internal {
                message: format!("failed to open secret file for write: {err}"),
            })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    file.write_all(bytes).map_err(|err| AppError::Internal {
        message: format!("failed to write secret file: {err}"),
    })?;
    file.sync_all().map_err(|err| AppError::Internal {
        message: format!("failed to sync secret file: {err}"),
    })?;
    Ok(())
}

/// Build the production secret stack: encrypted vault + keychain migration + cache.
///
/// # Errors
///
/// Returns an error if the vault cannot be opened under `app_data_dir`.
pub fn open_production_secret_store(
    app_data_dir: impl AsRef<Path>,
) -> Result<CachingSecretStore<LayeredSecretStore>, AppError> {
    let layered = LayeredSecretStore::open(app_data_dir)?;
    Ok(CachingSecretStore::new(layered))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

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

    #[test]
    fn local_encrypted_store_roundtrips_and_persists() {
        let dir = tempdir().unwrap();
        let store = LocalEncryptedSecretStore::open(dir.path()).unwrap();
        store.set("provider:deepseek", "sk-test-123").unwrap();
        assert_eq!(
            store.get("provider:deepseek").unwrap(),
            Some("sk-test-123".to_owned())
        );

        let reopened = LocalEncryptedSecretStore::open(dir.path()).unwrap();
        assert_eq!(
            reopened.get("provider:deepseek").unwrap(),
            Some("sk-test-123".to_owned())
        );

        reopened.delete("provider:deepseek").unwrap();
        assert_eq!(reopened.get("provider:deepseek").unwrap(), None);
    }

    #[test]
    fn caching_store_serves_memory_after_first_read() {
        let dir = tempdir().unwrap();
        let vault = LocalEncryptedSecretStore::open(dir.path()).unwrap();
        vault.set("k", "v1").unwrap();
        let cached = CachingSecretStore::new(vault);
        assert_eq!(cached.get("k").unwrap(), Some("v1".to_owned()));

        // Corrupt the on-disk entry; cache should still return the old value.
        let entry = cached.inner().entry_path("k");
        fs::write(&entry, b"not-valid-ciphertext").unwrap();
        assert_eq!(cached.get("k").unwrap(), Some("v1".to_owned()));

        cached.set("k", "v2").unwrap();
        assert_eq!(cached.get("k").unwrap(), Some("v2".to_owned()));
    }

    #[test]
    fn ciphertext_is_not_plaintext_on_disk() {
        let dir = tempdir().unwrap();
        let store = LocalEncryptedSecretStore::open(dir.path()).unwrap();
        store.set("provider:x", "super-secret-value").unwrap();
        let entry = store.entry_path("provider:x");
        let raw = fs::read(entry).unwrap();
        let as_text = String::from_utf8_lossy(&raw);
        assert!(!as_text.contains("super-secret-value"));
        assert_eq!(raw[0], VAULT_VERSION);
    }
}
