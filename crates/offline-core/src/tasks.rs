//! Typed task snapshots decoded from materialized query results.
//!
//! The production core had one of these per driver bootstrap query; the shape
//! is the same: decode the encrypted JSON snapshot into a reviewed record type
//! and surface contract drift as an error instead of a crash in Swift/Kotlin.

use mobile_store::{MobileStore, QuerySnapshotRepository};
use serde::Deserialize;

use crate::SdkError;

/// One task as the native UI sees it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TaskItem {
    /// Server task ID.
    pub id: String,
    /// Task title.
    pub title: String,
    /// Workflow status (`open` or `done`).
    pub status: String,
    /// Total logged minutes.
    pub logged_minutes: i64,
}

/// Typed task list plus cache revision metadata.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TasksSnapshot {
    /// Tasks in server order.
    pub tasks: Vec<TaskItem>,
    /// Local materialization revision.
    pub revision: i64,
    /// Local materialization time in epoch milliseconds.
    pub cached_at: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskWire {
    #[serde(rename = "_id")]
    id: String,
    title: String,
    status: String,
    #[serde(default)]
    logged_minutes: i64,
}

pub(crate) fn load(
    store: &MobileStore,
    subscription_key: &str,
) -> Result<Option<TasksSnapshot>, SdkError> {
    let Some(snapshot) = QuerySnapshotRepository::new(store)
        .get(subscription_key)
        .map_err(|_| SdkError::Persistence)?
    else {
        return Ok(None);
    };
    let wire: Vec<TaskWire> =
        serde_json::from_value(snapshot.result).map_err(|_| SdkError::FeatureContract)?;
    Ok(Some(TasksSnapshot {
        tasks: wire
            .into_iter()
            .map(|task| TaskItem {
                id: task.id,
                title: task.title,
                status: task.status,
                logged_minutes: task.logged_minutes,
            })
            .collect(),
        revision: snapshot.revision,
        cached_at: snapshot.cached_at,
    }))
}
