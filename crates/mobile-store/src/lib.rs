//! Versioned local persistence and repository boundaries.

#![allow(clippy::needless_raw_string_hashes)]

mod migrations;
mod outbox;
mod records;
mod security;

use std::{
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

pub use outbox::{NewOutboxItem, OutboxItem, OutboxRepository};
pub use records::{
    CachedEntity, CachedEntityRepository, EldRecord, EldRecordCounts, EldRepository, GpsFix,
    GpsRepository, LocalEldRecord, QuerySnapshot, QuerySnapshotRepository,
};
use rusqlite::Connection;
pub use security::{DatabaseKey, InvalidDatabaseKey};
use thiserror::Error;

/// Local storage failures safe to propagate to the SDK layer.
#[derive(Debug, Error)]
pub enum StoreError {
    /// `SQLite` operation failed.
    #[error("local database operation failed")]
    Database(#[from] rusqlite::Error),
    /// Stored JSON was malformed.
    #[error("stored JSON payload is invalid")]
    InvalidJson(#[from] serde_json::Error),
    /// The function is absent from the reviewed mobile contract catalog.
    #[error("unknown mobile backend function: {0}")]
    UnknownFunction(String),
    /// The function cannot safely be retried from the durable outbox.
    #[error("backend function is not approved for offline delivery: {0}")]
    FunctionNotQueueable(String),
    /// Arguments do not satisfy the reviewed server deduplication contract.
    #[error("backend function arguments lack a valid idempotency identity: {0}")]
    InvalidIdempotencyArguments(String),
    /// The stored function kind is invalid.
    #[error("stored backend function kind is invalid")]
    InvalidFunctionKind,
    /// Native-provided database key was invalid.
    #[error("database encryption key is invalid")]
    InvalidDatabaseKey,
    /// Sensitive payload could not be encrypted.
    #[error("local payload encryption failed")]
    Encryption,
    /// Sensitive payload could not be authenticated or decrypted.
    #[error("local payload decryption failed")]
    Decryption,
    /// Internal database lock was poisoned.
    #[error("local database is unavailable")]
    LockPoisoned,
}

/// Thread-safe local database used by the SDK.
#[derive(Debug)]
pub struct MobileStore {
    path: PathBuf,
    connection: Mutex<Connection>,
    cipher: security::PayloadCipher,
}

impl MobileStore {
    /// Opens the database and applies every pending migration.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when `SQLite` cannot initialize or migrate.
    pub fn open(path: impl AsRef<Path>, database_key: DatabaseKey) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        let mut connection = Connection::open(&path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        migrations::migrate(&mut connection)?;
        Ok(Self {
            path,
            connection: Mutex::new(connection),
            cipher: security::PayloadCipher::new(database_key),
        })
    }

    /// Returns the database path for redacted diagnostics.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the applied schema version.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the database cannot be queried.
    pub fn schema_version(&self) -> Result<i64, StoreError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .map_err(StoreError::from)
    }

    /// Verifies that the database connection is usable.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the health-check query fails.
    pub fn health_check(&self) -> Result<(), StoreError> {
        self.connection()?.execute_batch("SELECT 1;")?;
        Ok(())
    }

    pub(crate) fn connection(&self) -> Result<MutexGuard<'_, Connection>, StoreError> {
        self.connection.lock().map_err(|_| StoreError::LockPoisoned)
    }

    pub(crate) fn encrypt_json(
        &self,
        value: &serde_json::Value,
        context: &str,
    ) -> Result<String, StoreError> {
        let plaintext = serde_json::to_vec(value)?;
        self.cipher.encrypt(&plaintext, context.as_bytes())
    }

    pub(crate) fn decrypt_json(
        &self,
        value: &str,
        context: &str,
    ) -> Result<serde_json::Value, StoreError> {
        let plaintext = self.cipher.decrypt(value, context.as_bytes())?;
        Ok(serde_json::from_slice(&plaintext)?)
    }
}

#[cfg(test)]
mod tests {
    use super::{DatabaseKey, MobileStore};

    #[test]
    fn opens_and_migrates_an_in_memory_database() {
        let store =
            MobileStore::open(":memory:", DatabaseKey::testing()).expect("database should open");
        store.health_check().expect("database should respond");
        assert!(store.schema_version().expect("version should load") >= 1);
    }
}
