//! Typed offline-write intents exposed to Swift and Kotlin.
//!
//! Each intent validates on the device and builds a [`NewOutboxItem`] for one
//! function in the reviewed catalog. The outbox then applies the catalog's
//! idempotency contract (injecting the operation ID where the contract names an
//! argument) before anything is persisted.

use mobile_store::NewOutboxItem;
use serde_json::{Map, Value, json};

use crate::SdkError;

/// Typed offline task creation.
#[derive(Debug, Clone, uniffi::Record)]
pub struct CreateTaskIntent {
    /// Organization ID.
    pub org_id: String,
    /// Task title.
    pub title: String,
    /// Local creation timestamp.
    pub created_at: i64,
}

/// Typed offline task completion.
#[derive(Debug, Clone, uniffi::Record)]
pub struct CompleteTaskIntent {
    /// Organization ID.
    pub org_id: String,
    /// Task ID (the operation ID returned by `enqueue_create_task` works too).
    pub task_id: String,
    /// Local creation timestamp.
    pub created_at: i64,
}

/// One time entry with a stable native identity.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TimeEntry {
    /// Stable native entry ID.
    pub entry_id: String,
    /// Minutes worked.
    pub minutes: i64,
}

/// Typed offline time-log batch.
#[derive(Debug, Clone, uniffi::Record)]
pub struct LogTimeIntent {
    /// Organization ID.
    pub org_id: String,
    /// Task ID.
    pub task_id: String,
    /// Entries to apply at most once each.
    pub entries: Vec<TimeEntry>,
    /// Local creation timestamp.
    pub created_at: i64,
}

/// One device reading with a monotonic sequence.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SensorReading {
    /// Monotonic device sequence.
    pub seq: i64,
    /// Device timestamp.
    pub recorded_at: i64,
    /// Latitude, when the device reports one.
    pub latitude: Option<f64>,
    /// Longitude, when the device reports one.
    pub longitude: Option<f64>,
    /// Reading value.
    pub value: f64,
}

/// Typed offline device-reading batch.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SensorBatchIntent {
    /// Organization ID.
    pub org_id: String,
    /// Device ID.
    pub device_id: String,
    /// Ordered readings.
    pub readings: Vec<SensorReading>,
    /// Local creation timestamp.
    pub created_at: i64,
}

pub(crate) fn create_task(intent: &CreateTaskIntent) -> Result<NewOutboxItem, SdkError> {
    require_text(&intent.org_id)?;
    require_text(&intent.title)?;
    Ok(NewOutboxItem {
        operation_id: None,
        function_name: "tasks:create".to_owned(),
        entity_kind: Some("task".to_owned()),
        entity_id: None,
        org_id: Some(intent.org_id.clone()),
        payload: json!({
            "orgId": intent.org_id,
            "title": intent.title.trim(),
        }),
        created_at: intent.created_at,
    })
}

pub(crate) fn complete_task(intent: &CompleteTaskIntent) -> Result<NewOutboxItem, SdkError> {
    require_text(&intent.org_id)?;
    require_text(&intent.task_id)?;
    Ok(NewOutboxItem {
        operation_id: None,
        function_name: "tasks:complete".to_owned(),
        entity_kind: Some("task".to_owned()),
        entity_id: Some(intent.task_id.clone()),
        org_id: Some(intent.org_id.clone()),
        payload: json!({
            "orgId": intent.org_id,
            "taskId": intent.task_id,
        }),
        created_at: intent.created_at,
    })
}

pub(crate) fn log_time(intent: LogTimeIntent) -> Result<NewOutboxItem, SdkError> {
    require_text(&intent.org_id)?;
    require_text(&intent.task_id)?;
    if intent.entries.is_empty() {
        return Err(SdkError::InvalidFunctionArguments);
    }
    let mut entries = Vec::with_capacity(intent.entries.len());
    for entry in intent.entries {
        require_text(&entry.entry_id)?;
        if entry.minutes <= 0 {
            return Err(SdkError::InvalidFunctionArguments);
        }
        entries.push(json!({"entryId": entry.entry_id, "minutes": entry.minutes}));
    }
    Ok(NewOutboxItem {
        operation_id: None,
        function_name: "tasks:logTime".to_owned(),
        entity_kind: Some("task".to_owned()),
        entity_id: Some(intent.task_id.clone()),
        org_id: Some(intent.org_id.clone()),
        payload: json!({
            "orgId": intent.org_id,
            "taskId": intent.task_id,
            "entries": entries,
        }),
        created_at: intent.created_at,
    })
}

pub(crate) fn sensor_batch(intent: SensorBatchIntent) -> Result<NewOutboxItem, SdkError> {
    require_text(&intent.org_id)?;
    require_text(&intent.device_id)?;
    if intent.readings.is_empty() {
        return Err(SdkError::InvalidFunctionArguments);
    }
    let mut readings = Vec::with_capacity(intent.readings.len());
    for reading in intent.readings {
        if reading.seq < 0 || reading.recorded_at <= 0 || !reading.value.is_finite() {
            return Err(SdkError::InvalidFunctionArguments);
        }
        let mut payload = Map::from_iter([
            ("seq".to_owned(), json!(reading.seq)),
            ("recordedAt".to_owned(), json!(reading.recorded_at)),
            ("value".to_owned(), json!(reading.value)),
        ]);
        match (reading.latitude, reading.longitude) {
            (Some(latitude), Some(longitude)) => {
                validate_coordinate(latitude, longitude)?;
                payload.insert("lat".to_owned(), json!(latitude));
                payload.insert("lng".to_owned(), json!(longitude));
            }
            (None, None) => {}
            _ => return Err(SdkError::InvalidFunctionArguments),
        }
        readings.push(Value::Object(payload));
    }
    Ok(NewOutboxItem {
        operation_id: None,
        function_name: "sensors:ingestReadings".to_owned(),
        entity_kind: Some("device".to_owned()),
        entity_id: Some(intent.device_id.clone()),
        org_id: Some(intent.org_id.clone()),
        payload: json!({
            "orgId": intent.org_id,
            "deviceId": intent.device_id,
            "readings": readings,
        }),
        created_at: intent.created_at,
    })
}

fn require_text(value: &str) -> Result<(), SdkError> {
    if value.trim().is_empty() {
        Err(SdkError::InvalidFunctionArguments)
    } else {
        Ok(())
    }
}

fn validate_coordinate(latitude: f64, longitude: f64) -> Result<(), SdkError> {
    if latitude.is_finite()
        && longitude.is_finite()
        && (-90.0..=90.0).contains(&latitude)
        && (-180.0..=180.0).contains(&longitude)
    {
        Ok(())
    } else {
        Err(SdkError::InvalidFunctionArguments)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CompleteTaskIntent, LogTimeIntent, SensorBatchIntent, SensorReading, TimeEntry,
        complete_task, log_time, sensor_batch,
    };

    #[test]
    fn builds_typed_sensor_batch_contract() {
        let item = sensor_batch(SensorBatchIntent {
            org_id: "org-1".to_owned(),
            device_id: "device-1".to_owned(),
            readings: vec![SensorReading {
                seq: 1,
                recorded_at: 1_000,
                latitude: Some(45.0),
                longitude: Some(7.0),
                value: 21.5,
            }],
            created_at: 1_100,
        })
        .expect("intent should build");
        assert_eq!(item.payload["readings"][0]["recordedAt"], 1_000);
        assert_eq!(item.payload["readings"][0]["lat"], 45.0);
    }

    #[test]
    fn rejects_invalid_time_entries() {
        assert!(
            log_time(LogTimeIntent {
                org_id: "org-1".to_owned(),
                task_id: "task-1".to_owned(),
                entries: vec![TimeEntry {
                    entry_id: "entry-1".to_owned(),
                    minutes: -1,
                }],
                created_at: 1_000,
            })
            .is_err()
        );
    }

    #[test]
    fn builds_intrinsically_idempotent_completion_contract() {
        let item = complete_task(&CompleteTaskIntent {
            org_id: "org-1".to_owned(),
            task_id: "task-1".to_owned(),
            created_at: 2_000,
        })
        .expect("intent should build");

        assert_eq!(item.function_name, "tasks:complete");
        assert_eq!(item.entity_id.as_deref(), Some("task-1"));
    }
}
