//! AES-GCM memory cipher adapter backed by the app secret vault master key.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use app_memory::MemoryCipher;
use app_models::AppError;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

const MASTER_KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
const MEMORY_CIPHER_VERSION: u8 = 1;

/// Encrypts sensitive memory values with the same master key material as the secret vault.
pub struct VaultMemoryCipher {
    master_key: [u8; MASTER_KEY_BYTES],
}

impl VaultMemoryCipher {
    /// Open or create a master key under `app_data_dir` (shared with secret vault when present).
    pub fn open(app_data_dir: &Path) -> Result<Self, AppError> {
        let master_key_path = app_data_dir.join("secret_master.key");
        let master_key = load_or_create_master_key(&master_key_path)?;
        // Domain-separate memory encryption from secret vault entries.
        let mut hasher = Sha256::new();
        hasher.update(b"portico-memory-cipher-v1");
        hasher.update(master_key);
        let derived = hasher.finalize();
        let mut key = [0u8; MASTER_KEY_BYTES];
        key.copy_from_slice(&derived[..MASTER_KEY_BYTES]);
        Ok(Self { master_key: key })
    }
}

impl MemoryCipher for VaultMemoryCipher {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, AppError> {
        let cipher =
            Aes256Gcm::new_from_slice(&self.master_key).map_err(|err| AppError::Internal {
                message: format!("failed to init memory cipher: {err}"),
            })?;
        let mut nonce_bytes = [0u8; NONCE_BYTES];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher.encrypt(nonce, plaintext).map_err(|err| AppError::Internal {
            message: format!("failed to encrypt memory: {err}"),
        })?;
        let mut out = Vec::with_capacity(1 + NONCE_BYTES + ciphertext.len());
        out.push(MEMORY_CIPHER_VERSION);
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    fn decrypt(&self, blob: &[u8]) -> Result<Vec<u8>, AppError> {
        if blob.len() < 1 + NONCE_BYTES + 1 {
            return Err(AppError::Internal {
                message: "encrypted memory blob is truncated".to_owned(),
            });
        }
        if blob[0] != MEMORY_CIPHER_VERSION {
            return Err(AppError::Internal {
                message: format!("unsupported memory cipher version {}", blob[0]),
            });
        }
        let nonce = Nonce::from_slice(&blob[1..=NONCE_BYTES]);
        let ciphertext = &blob[1 + NONCE_BYTES..];
        let cipher =
            Aes256Gcm::new_from_slice(&self.master_key).map_err(|err| AppError::Internal {
                message: format!("failed to init memory cipher: {err}"),
            })?;
        cipher.decrypt(nonce, ciphertext).map_err(|err| AppError::Internal {
            message: format!("failed to decrypt memory (fail closed): {err}"),
        })
    }
}

fn load_or_create_master_key(path: &Path) -> Result<[u8; MASTER_KEY_BYTES], AppError> {
    if path.exists() {
        let bytes = fs::read(path).map_err(|err| AppError::Internal {
            message: format!("failed to read secret master key: {err}"),
        })?;
        if bytes.len() != MASTER_KEY_BYTES {
            return Err(AppError::Internal {
                message: "secret master key has unexpected length".to_owned(),
            });
        }
        let mut key = [0u8; MASTER_KEY_BYTES];
        key.copy_from_slice(&bytes);
        return Ok(key);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| AppError::Internal {
            message: format!("failed to create app data dir for master key: {err}"),
        })?;
    }
    let mut key = [0u8; MASTER_KEY_BYTES];
    rand::thread_rng().fill_bytes(&mut key);
    write_private_file(path, &key)?;
    Ok(key)
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|err| AppError::Internal {
            message: format!("failed to create master key file: {err}"),
        })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    file.write_all(bytes).map_err(|err| AppError::Internal {
        message: format!("failed to write master key: {err}"),
    })?;
    Ok(())
}
