//! Pluggable encryption for sensitive long-term memories.
//!
//! Implemented by the composition root (Tauri) using the app vault master key so
//! `app-memory` does not depend on `app-security`.

use app_models::AppError;

/// Encrypt / decrypt sensitive memory values.
pub trait MemoryCipher: Send + Sync {
    /// Encrypt plaintext bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the cipher cannot be initialized or encryption fails.
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, AppError>;

    /// Decrypt ciphertext produced by [`Self::encrypt`].
    ///
    /// # Errors
    ///
    /// Returns an error if decryption fails (fail closed — do not inject).
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, AppError>;
}

/// No-op cipher used in tests and when vault is unavailable.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopMemoryCipher;

impl MemoryCipher for NoopMemoryCipher {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, AppError> {
        Ok(plaintext.to_vec())
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, AppError> {
        Ok(ciphertext.to_vec())
    }
}

/// Current encryption format version written to `memories.encryption_version`.
pub const MEMORY_ENCRYPTION_VERSION: i64 = 1;

/// Placeholder stored in the plaintext `value` column for encrypted rows.
pub const ENCRYPTED_VALUE_PLACEHOLDER: &str = "";
