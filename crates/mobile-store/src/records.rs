use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{MobileStore, StoreError};

/// Generic server-confirmed entity cached from the backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CachedEntity {
    /// Entity category.
    pub kind: String,
    /// Server identifier.
    pub id: String,
    /// Optional organization identifier.
    pub org_id: Option<String>,
    /// Optional workflow status.
    pub status: Option<String>,
    /// Full server payload.
    pub payload: serde_json::Value,
    /// Optional server update time.
    pub server_updated_at: Option<i64>,
    /// Local cache time.
    pub cached_at: i64,
}

/// Repository for server-confirmed entity snapshots.
pub struct CachedEntityRepository<'a> {
    store: &'a MobileStore,
}

impl<'a> CachedEntityRepository<'a> {
    /// Creates a repository.
    pub const fn new(store: &'a MobileStore) -> Self {
        Self { store }
    }

    /// Inserts or replaces a server-confirmed entity.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when serialization or storage fails.
    pub fn upsert(&self, entity: &CachedEntity) -> Result<(), StoreError> {
        let context = cached_entity_context(&entity.kind, &entity.id);
        self.store.connection()?.execute(
            r#"INSERT INTO cached_entities
                 (kind, id, org_id, status, payload_json, server_updated_at, cached_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
               ON CONFLICT(kind, id) DO UPDATE SET
                 org_id = excluded.org_id,
                 status = excluded.status,
                 payload_json = excluded.payload_json,
                 server_updated_at = excluded.server_updated_at,
                 cached_at = excluded.cached_at"#,
            params![
                entity.kind,
                entity.id,
                entity.org_id,
                entity.status,
                self.store.encrypt_json(&entity.payload, &context)?,
                entity.server_updated_at,
                entity.cached_at,
            ],
        )?;
        Ok(())
    }

    /// Gets one cached entity.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when loading or decoding fails.
    pub fn get(&self, kind: &str, id: &str) -> Result<Option<CachedEntity>, StoreError> {
        let row = self
            .store
            .connection()?
            .query_row(
                r#"SELECT kind, id, org_id, status, payload_json,
                          server_updated_at, cached_at
                   FROM cached_entities WHERE kind = ?1 AND id = ?2"#,
                params![kind, id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .optional()?;
        row.map(|row| {
            let context = cached_entity_context(&row.0, &row.1);
            Ok(CachedEntity {
                kind: row.0,
                id: row.1,
                org_id: row.2,
                status: row.3,
                payload: self.store.decrypt_json(&row.4, &context)?,
                server_updated_at: row.5,
                cached_at: row.6,
            })
        })
        .transpose()
    }
}

/// Durable materialized result of one realtime query subscription.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuerySnapshot {
    /// Native-selected stable subscription identity.
    pub subscription_key: String,
    /// Public backend query path.
    pub function_name: String,
    /// Arguments used by the subscription.
    pub args: serde_json::Value,
    /// Latest complete query result.
    pub result: serde_json::Value,
    /// Monotonically increasing local revision.
    pub revision: i64,
    /// Optional server update time extracted by a feature repository.
    pub server_updated_at: Option<i64>,
    /// Local materialization time.
    pub cached_at: i64,
}

/// Encrypted repository for complete realtime query results.
pub struct QuerySnapshotRepository<'a> {
    store: &'a MobileStore,
}

impl<'a> QuerySnapshotRepository<'a> {
    /// Creates a query snapshot repository.
    pub const fn new(store: &'a MobileStore) -> Self {
        Self { store }
    }

    /// Atomically stores the latest complete query result.
    ///
    /// The local revision increments for each successful materialization.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when encryption or storage fails.
    pub fn materialize(
        &self,
        subscription_key: &str,
        function_name: &str,
        args: &serde_json::Value,
        result: &serde_json::Value,
        server_updated_at: Option<i64>,
        cached_at: i64,
    ) -> Result<QuerySnapshot, StoreError> {
        let args_context = query_args_context(subscription_key);
        let result_context = query_result_context(subscription_key);
        self.store.connection()?.execute(
            r#"INSERT INTO query_snapshots (
                 subscription_key, function_name, args_json, result_json,
                 revision, server_updated_at, cached_at
               ) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6)
               ON CONFLICT(subscription_key) DO UPDATE SET
                 function_name = excluded.function_name,
                 args_json = excluded.args_json,
                 result_json = excluded.result_json,
                 revision = query_snapshots.revision + 1,
                 server_updated_at = excluded.server_updated_at,
                 cached_at = excluded.cached_at"#,
            params![
                subscription_key,
                function_name,
                self.store.encrypt_json(args, &args_context)?,
                self.store.encrypt_json(result, &result_context)?,
                server_updated_at,
                cached_at,
            ],
        )?;
        self.get(subscription_key)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows.into())
    }

    /// Returns one materialized query result.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when loading or decryption fails.
    pub fn get(&self, subscription_key: &str) -> Result<Option<QuerySnapshot>, StoreError> {
        let row = self
            .store
            .connection()?
            .query_row(
                r#"SELECT subscription_key, function_name, args_json, result_json,
                          revision, server_updated_at, cached_at
                   FROM query_snapshots WHERE subscription_key = ?1"#,
                [subscription_key],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .optional()?;
        row.map(|row| {
            Ok(QuerySnapshot {
                args: self
                    .store
                    .decrypt_json(&row.2, &query_args_context(&row.0))?,
                result: self
                    .store
                    .decrypt_json(&row.3, &query_result_context(&row.0))?,
                subscription_key: row.0,
                function_name: row.1,
                revision: row.4,
                server_updated_at: row.5,
                cached_at: row.6,
            })
        })
        .transpose()
    }

    /// Removes a materialized query result.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when deletion fails.
    pub fn remove(&self, subscription_key: &str) -> Result<(), StoreError> {
        self.store.connection()?.execute(
            "DELETE FROM query_snapshots WHERE subscription_key = ?1",
            [subscription_key],
        )?;
        Ok(())
    }
}

/// Durable GPS fix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpsFix {
    /// Stable fix identifier.
    pub id: String,
    /// Optional active trip.
    pub trip_id: Option<String>,
    /// Latitude.
    pub lat: f64,
    /// Longitude.
    pub lng: f64,
    /// Optional speed.
    pub speed: Option<f64>,
    /// Optional heading.
    pub heading: Option<f64>,
    /// Optional accuracy.
    pub accuracy: Option<f64>,
    /// Recording time.
    pub recorded_at: i64,
}

/// GPS queue repository.
pub struct GpsRepository<'a> {
    store: &'a MobileStore,
}

impl<'a> GpsRepository<'a> {
    /// Creates a repository.
    pub const fn new(store: &'a MobileStore) -> Self {
        Self { store }
    }

    /// Saves a fix idempotently.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when storage fails.
    pub fn insert(&self, fix: &GpsFix) -> Result<(), StoreError> {
        self.store.connection()?.execute(
            r#"INSERT OR IGNORE INTO gps_fixes
                 (id, trip_id, lat, lng, speed, heading, accuracy, recorded_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
            params![
                fix.id,
                fix.trip_id,
                fix.lat,
                fix.lng,
                fix.speed,
                fix.heading,
                fix.accuracy,
                fix.recorded_at
            ],
        )?;
        Ok(())
    }

    /// Lists pending fixes in deterministic order.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when loading fails.
    pub fn pending(&self, limit: i64) -> Result<Vec<GpsFix>, StoreError> {
        let connection = self.store.connection()?;
        let mut statement = connection.prepare(
            r#"SELECT id, trip_id, lat, lng, speed, heading, accuracy, recorded_at
               FROM gps_fixes
               WHERE uploaded_at IS NULL AND rejected_at IS NULL
               ORDER BY recorded_at, id LIMIT ?1"#,
        )?;
        statement
            .query_map([limit], |row| {
                Ok(GpsFix {
                    id: row.get(0)?,
                    trip_id: row.get(1)?,
                    lat: row.get(2)?,
                    lng: row.get(3)?,
                    speed: row.get(4)?,
                    heading: row.get(5)?,
                    accuracy: row.get(6)?,
                    recorded_at: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// Acknowledges uploaded fixes.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the transaction fails.
    pub fn acknowledge(&self, ids: &[String], uploaded_at: i64) -> Result<(), StoreError> {
        let mut connection = self.store.connection()?;
        let transaction = connection.transaction()?;
        for id in ids {
            transaction.execute(
                "UPDATE gps_fixes SET uploaded_at = ?1 WHERE id = ?2",
                params![uploaded_at, id],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }
}

/// Raw ELD record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EldRecord {
    /// Device identifier.
    pub device_id: String,
    /// Device sequence number.
    pub sequence: i64,
    /// Recording time.
    pub recorded_at: i64,
    /// Stable record type.
    pub record_type: String,
    /// Provider-neutral record payload.
    pub payload: serde_json::Value,
}

/// Local ELD record with upload state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalEldRecord {
    /// Raw record.
    pub record: EldRecord,
    /// Local receipt time.
    pub received_at: i64,
    /// Upload time.
    pub uploaded_at: Option<i64>,
    /// Rejection time.
    pub rejected_at: Option<i64>,
    /// Rejection reason.
    pub reject_reason: Option<String>,
}

/// Aggregate ELD queue counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EldRecordCounts {
    /// Total rows.
    pub total: i64,
    /// Pending rows.
    pub pending: i64,
    /// Uploaded rows.
    pub uploaded: i64,
    /// Rejected rows.
    pub rejected: i64,
}

/// ELD record repository.
pub struct EldRepository<'a> {
    store: &'a MobileStore,
}

impl<'a> EldRepository<'a> {
    /// Creates a repository.
    pub const fn new(store: &'a MobileStore) -> Self {
        Self { store }
    }

    /// Stores records idempotently by device and sequence.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when serialization or storage fails.
    pub fn insert_batch(&self, records: &[EldRecord], received_at: i64) -> Result<(), StoreError> {
        let mut connection = self.store.connection()?;
        let transaction = connection.transaction()?;
        for record in records {
            let context = eld_context(&record.device_id, record.sequence);
            transaction.execute(
                r#"INSERT OR IGNORE INTO eld_records
                     (device_id, sequence, recorded_at, record_type, payload_json, received_at)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6)"#,
                params![
                    record.device_id,
                    record.sequence,
                    record.recorded_at,
                    record.record_type,
                    self.store.encrypt_json(&record.payload, &context)?,
                    received_at,
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Returns queue counts.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when loading fails.
    pub fn counts(&self) -> Result<EldRecordCounts, StoreError> {
        self.store
            .connection()?
            .query_row(
                r#"SELECT COUNT(*),
                          COALESCE(SUM(uploaded_at IS NULL AND rejected_at IS NULL), 0),
                          COALESCE(SUM(uploaded_at IS NOT NULL), 0),
                          COALESCE(SUM(rejected_at IS NOT NULL), 0)
                   FROM eld_records"#,
                [],
                |row| {
                    Ok(EldRecordCounts {
                        total: row.get(0)?,
                        pending: row.get(1)?,
                        uploaded: row.get(2)?,
                        rejected: row.get(3)?,
                    })
                },
            )
            .map_err(StoreError::from)
    }
}

fn cached_entity_context(kind: &str, id: &str) -> String {
    format!("cached_entity:{kind}:{id}")
}

fn query_args_context(subscription_key: &str) -> String {
    format!("query_snapshot_args:{subscription_key}")
}

fn query_result_context(subscription_key: &str) -> String {
    format!("query_snapshot_result:{subscription_key}")
}

fn eld_context(device_id: &str, sequence: i64) -> String {
    format!("eld:{device_id}:{sequence}")
}
