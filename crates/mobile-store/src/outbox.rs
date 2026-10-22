use std::collections::HashSet;

use mobile_contracts::{FunctionKind, IdempotencyStrategy, OfflinePolicy, find_function};
use mobile_models::{OperationId, OutboxState};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{MobileStore, StoreError};

/// New durable backend function intent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewOutboxItem {
    /// Optional caller-provided operation ID.
    pub operation_id: Option<OperationId>,
    /// Public backend function name.
    pub function_name: String,
    /// Optional entity kind for ordering and diagnostics.
    pub entity_kind: Option<String>,
    /// Optional entity identifier.
    pub entity_id: Option<String>,
    /// Optional organization identifier.
    pub org_id: Option<String>,
    /// Serialized function arguments.
    pub payload: serde_json::Value,
    /// Creation time in epoch milliseconds.
    pub created_at: i64,
}

/// Stored outbox entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboxItem {
    /// Stable idempotency key.
    pub operation_id: OperationId,
    /// Public backend function name.
    pub function_name: String,
    /// Backend execution kind.
    pub function_kind: FunctionKind,
    /// Optional entity kind.
    pub entity_kind: Option<String>,
    /// Optional entity identifier.
    pub entity_id: Option<String>,
    /// Optional organization identifier.
    pub org_id: Option<String>,
    /// Serialized arguments.
    pub payload: serde_json::Value,
    /// Current processing state.
    pub state: OutboxState,
    /// Number of failed attempts.
    pub attempt_count: i64,
    /// Earliest retry time.
    pub next_attempt_at: i64,
    /// Creation time.
    pub created_at: i64,
    /// Last update time.
    pub updated_at: i64,
}

/// Durable backend function outbox repository.
pub struct OutboxRepository<'a> {
    store: &'a MobileStore,
}

impl<'a> OutboxRepository<'a> {
    /// Creates a repository.
    pub const fn new(store: &'a MobileStore) -> Self {
        Self { store }
    }

    /// Persists an approved idempotent function intent before network delivery.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when serialization or storage fails.
    pub fn enqueue(&self, mut item: NewOutboxItem) -> Result<OutboxItem, StoreError> {
        let contract = find_function(&item.function_name)
            .ok_or_else(|| StoreError::UnknownFunction(item.function_name.clone()))?;
        if contract.offline_policy != OfflinePolicy::Queueable {
            return Err(StoreError::FunctionNotQueueable(item.function_name.clone()));
        }
        let operation_id = item.operation_id.unwrap_or_default();
        prepare_idempotency(
            contract.idempotency,
            &operation_id,
            &mut item.payload,
            &item.function_name,
        )?;
        let payload = self
            .store
            .encrypt_json(&item.payload, &outbox_context(&operation_id))?;
        self.store.connection()?.execute(
            r#"INSERT INTO function_outbox (
                 operation_id, function_name, function_kind, entity_kind, entity_id, org_id,
                 payload_json, state, attempt_count, next_attempt_at,
                 created_at, updated_at
               ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', 0, ?8, ?8, ?8)
               ON CONFLICT(operation_id) DO NOTHING"#,
            params![
                operation_id.as_str(),
                item.function_name,
                contract.kind.as_str(),
                item.entity_kind,
                item.entity_id,
                item.org_id,
                payload,
                item.created_at,
            ],
        )?;
        self.get(&operation_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows.into())
    }

    /// Claims ready work under a bounded lease.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the transaction fails.
    pub fn claim_ready(
        &self,
        now: i64,
        limit: i64,
        lease_owner: &str,
        lease_duration_ms: i64,
    ) -> Result<Vec<OutboxItem>, StoreError> {
        let mut connection = self.store.connection()?;
        let transaction = connection.transaction()?;
        let ids = {
            let mut statement = transaction.prepare(
                // A `sending` row whose lease expired belongs to a worker that
                // died mid-delivery. Reclaiming it is safe because only
                // server-idempotent operations can enter this table.
                r#"SELECT operation_id FROM function_outbox
                   WHERE (
                       (state IN ('pending', 'retry_wait') AND next_attempt_at <= ?1)
                       OR (state = 'sending' AND lease_expires_at <= ?1)
                     )
                     AND (lease_expires_at IS NULL OR lease_expires_at <= ?1)
                   ORDER BY created_at, operation_id
                   LIMIT ?2"#,
            )?;
            statement
                .query_map(params![now, limit], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        for id in &ids {
            transaction.execute(
                r#"UPDATE function_outbox
                   SET state = 'sending', lease_owner = ?1, lease_expires_at = ?2,
                       updated_at = ?3
                   WHERE operation_id = ?4"#,
                params![lease_owner, now + lease_duration_ms, now, id],
            )?;
        }
        transaction.commit()?;
        drop(connection);
        ids.iter()
            .filter_map(|id| OperationId::parse(id).ok())
            .map(|id| {
                self.get(&id)?
                    .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
            })
            .collect()
    }

    /// Marks an operation as acknowledged by the backend.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the update fails.
    pub fn acknowledge(&self, id: &OperationId, now: i64) -> Result<(), StoreError> {
        self.store.connection()?.execute(
            r#"UPDATE function_outbox
               SET state = 'acknowledged', acknowledged_at = ?1, updated_at = ?1,
                   lease_owner = NULL, lease_expires_at = NULL,
                   last_error_code = NULL, last_error_message = NULL
               WHERE operation_id = ?2"#,
            params![now, id.as_str()],
        )?;
        Ok(())
    }

    /// Schedules a retry with a caller-computed retry time.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the update fails.
    pub fn retry(
        &self,
        id: &OperationId,
        now: i64,
        next_attempt_at: i64,
        error_code: &str,
        error_message: &str,
    ) -> Result<(), StoreError> {
        self.store.connection()?.execute(
            r#"UPDATE function_outbox
               SET state = 'retry_wait', attempt_count = attempt_count + 1,
                   next_attempt_at = ?1, updated_at = ?2,
                   lease_owner = NULL, lease_expires_at = NULL,
                   last_error_code = ?3, last_error_message = ?4
               WHERE operation_id = ?5"#,
            params![next_attempt_at, now, error_code, error_message, id.as_str()],
        )?;
        Ok(())
    }

    /// Marks an authoritative non-retryable rejection.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the update fails.
    pub fn reject(
        &self,
        id: &OperationId,
        now: i64,
        error_code: &str,
        error_message: &str,
    ) -> Result<(), StoreError> {
        self.store.connection()?.execute(
            r#"UPDATE function_outbox
               SET state = 'rejected_terminal', attempt_count = attempt_count + 1,
                   updated_at = ?1, lease_owner = NULL, lease_expires_at = NULL,
                   last_error_code = ?2, last_error_message = ?3
               WHERE operation_id = ?4"#,
            params![now, error_code, error_message, id.as_str()],
        )?;
        Ok(())
    }

    /// Counts operations that still need delivery.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query fails.
    pub fn undelivered_count(&self) -> Result<i64, StoreError> {
        self.store
            .connection()?
            .query_row(
                r#"SELECT COUNT(*) FROM function_outbox
                   WHERE state IN ('pending', 'sending', 'retry_wait')"#,
                [],
                |row| row.get(0),
            )
            .map_err(StoreError::from)
    }

    /// Gets one operation.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when loading or decoding fails.
    pub fn get(&self, id: &OperationId) -> Result<Option<OutboxItem>, StoreError> {
        self.store
            .connection()?
            .query_row(
                r#"SELECT operation_id, function_name, function_kind,
                          entity_kind, entity_id, org_id, payload_json, state, attempt_count,
                          next_attempt_at, created_at, updated_at
                   FROM function_outbox WHERE operation_id = ?1"#,
                [id.as_str()],
                map_outbox_row,
            )
            .optional()?
            .map(|row| decode_outbox(self.store, row))
            .transpose()
    }
}

fn prepare_idempotency(
    strategy: IdempotencyStrategy,
    operation_id: &OperationId,
    payload: &mut serde_json::Value,
    function_name: &str,
) -> Result<(), StoreError> {
    let object = payload
        .as_object_mut()
        .ok_or_else(|| StoreError::InvalidIdempotencyArguments(function_name.to_owned()))?;
    match strategy {
        IdempotencyStrategy::None => {
            return Err(StoreError::InvalidIdempotencyArguments(
                function_name.to_owned(),
            ));
        }
        IdempotencyStrategy::Intrinsic => {}
        IdempotencyStrategy::Argument(field) => {
            let value = object
                .entry(field)
                .or_insert_with(|| serde_json::Value::String(operation_id.as_str().to_owned()));
            if !is_non_empty_scalar(value) {
                return Err(StoreError::InvalidIdempotencyArguments(
                    function_name.to_owned(),
                ));
            }
        }
        IdempotencyStrategy::CollectionItem { collection, field } => {
            validate_unique_collection_ids(object.get(collection), field, function_name)?;
        }
        IdempotencyStrategy::RootAndCollectionItem {
            root_field,
            collection,
            item_field,
        } => {
            if !object.get(root_field).is_some_and(is_non_empty_scalar) {
                return Err(StoreError::InvalidIdempotencyArguments(
                    function_name.to_owned(),
                ));
            }
            validate_unique_collection_ids(object.get(collection), item_field, function_name)?;
        }
    }
    Ok(())
}

fn validate_unique_collection_ids(
    collection: Option<&serde_json::Value>,
    field: &str,
    function_name: &str,
) -> Result<(), StoreError> {
    let values = collection
        .and_then(serde_json::Value::as_array)
        .filter(|values| !values.is_empty())
        .ok_or_else(|| StoreError::InvalidIdempotencyArguments(function_name.to_owned()))?;
    let mut identities = HashSet::new();
    for item in values {
        let identity = item
            .as_object()
            .and_then(|item| item.get(field))
            .filter(|value| is_non_empty_scalar(value))
            .map(serde_json::Value::to_string)
            .ok_or_else(|| StoreError::InvalidIdempotencyArguments(function_name.to_owned()))?;
        if !identities.insert(identity) {
            return Err(StoreError::InvalidIdempotencyArguments(
                function_name.to_owned(),
            ));
        }
    }
    Ok(())
}

fn is_non_empty_scalar(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(value) => !value.trim().is_empty(),
        serde_json::Value::Number(_) => true,
        _ => false,
    }
}

type OutboxRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    String,
    i64,
    i64,
    i64,
    i64,
);

fn map_outbox_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<OutboxRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
    ))
}

fn decode_outbox(store: &MobileStore, row: OutboxRow) -> Result<OutboxItem, StoreError> {
    let operation_id = OperationId::parse(&row.0).map_err(|_| rusqlite::Error::InvalidQuery)?;
    Ok(OutboxItem {
        payload: store.decrypt_json(&row.6, &outbox_context(&operation_id))?,
        operation_id,
        function_name: row.1,
        function_kind: FunctionKind::parse(&row.2).ok_or(StoreError::InvalidFunctionKind)?,
        entity_kind: row.3,
        entity_id: row.4,
        org_id: row.5,
        state: OutboxState::parse(&row.7).ok_or(rusqlite::Error::InvalidQuery)?,
        attempt_count: row.8,
        next_attempt_at: row.9,
        created_at: row.10,
        updated_at: row.11,
    })
}

fn outbox_context(id: &OperationId) -> String {
    format!("outbox:{}", id.as_str())
}
