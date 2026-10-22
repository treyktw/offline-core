use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng, Payload},
};
use base64::{Engine, engine::general_purpose::STANDARD_NO_PAD};
use zeroize::Zeroizing;

use crate::StoreError;

const PREFIX: &str = "enc:v1:";
const KEY_LENGTH: usize = 32;
const NONCE_LENGTH: usize = 12;

/// Invalid database encryption key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidDatabaseKey;

/// Native-generated 256-bit database encryption key.
pub struct DatabaseKey(Zeroizing<[u8; KEY_LENGTH]>);

impl DatabaseKey {
    /// Validates a native Keychain/Keystore-backed key.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidDatabaseKey`] unless exactly 32 bytes are provided.
    pub fn new(bytes: &[u8]) -> Result<Self, InvalidDatabaseKey> {
        let key: [u8; KEY_LENGTH] = bytes.try_into().map_err(|_| InvalidDatabaseKey)?;
        Ok(Self(Zeroizing::new(key)))
    }

    #[cfg(test)]
    pub(crate) fn testing() -> Self {
        Self(Zeroizing::new([0x5A; KEY_LENGTH]))
    }
}

impl std::fmt::Debug for DatabaseKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("DatabaseKey([REDACTED])")
    }
}

pub(crate) struct PayloadCipher {
    key: DatabaseKey,
}

impl PayloadCipher {
    pub(crate) const fn new(key: DatabaseKey) -> Self {
        Self { key }
    }

    pub(crate) fn encrypt(&self, plaintext: &[u8], aad: &[u8]) -> Result<String, StoreError> {
        let cipher =
            Aes256Gcm::new_from_slice(self.key.0.as_ref()).map_err(|_| StoreError::Encryption)?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|_| StoreError::Encryption)?;
        let mut encoded = Vec::with_capacity(NONCE_LENGTH + ciphertext.len());
        encoded.extend_from_slice(&nonce);
        encoded.extend_from_slice(&ciphertext);
        Ok(format!("{PREFIX}{}", STANDARD_NO_PAD.encode(encoded)))
    }

    pub(crate) fn decrypt(&self, value: &str, aad: &[u8]) -> Result<Vec<u8>, StoreError> {
        // Allows a controlled migration from pre-encryption development builds.
        let Some(encoded) = value.strip_prefix(PREFIX) else {
            return Ok(value.as_bytes().to_vec());
        };
        let bytes = STANDARD_NO_PAD
            .decode(encoded)
            .map_err(|_| StoreError::Decryption)?;
        if bytes.len() <= NONCE_LENGTH {
            return Err(StoreError::Decryption);
        }
        let (nonce, ciphertext) = bytes.split_at(NONCE_LENGTH);
        let cipher =
            Aes256Gcm::new_from_slice(self.key.0.as_ref()).map_err(|_| StoreError::Decryption)?;
        cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .map_err(|_| StoreError::Decryption)
    }
}

impl std::fmt::Debug for PayloadCipher {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PayloadCipher([REDACTED])")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{DatabaseKey, PayloadCipher};

    #[test]
    fn encrypts_and_authenticates_payloads() {
        let cipher = PayloadCipher::new(DatabaseKey::testing());
        let plaintext =
            serde_json::to_vec(&json!({"secret": "value"})).expect("JSON should encode");
        let encrypted = cipher
            .encrypt(&plaintext, b"entity:1")
            .expect("payload should encrypt");
        assert!(!encrypted.contains("secret"));
        assert_eq!(
            cipher
                .decrypt(&encrypted, b"entity:1")
                .expect("payload should decrypt"),
            plaintext
        );
        assert!(cipher.decrypt(&encrypted, b"entity:2").is_err());
    }
}
