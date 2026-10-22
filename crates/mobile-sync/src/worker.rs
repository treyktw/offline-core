use std::sync::Arc;

use async_trait::async_trait;
use mobile_contracts::FunctionKind;
use mobile_store::{MobileStore, OutboxRepository, StoreError};
use thiserror::Error;

use crate::RetryPolicy;

/// Mutation delivery failure classified for retry policy.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{code}: {message}")]
pub struct DeliveryError {
    /// Stable diagnostic code.
    pub code: String,
    /// Safe diagnostic message.
    pub message: String,
    /// Whether retrying may succeed without changing the payload.
    pub retryable: bool,
}

/// Transport used by the function outbox worker.
#[async_trait]
pub trait FunctionTransport: Send + Sync {
    /// Executes one public backend write function.
    async fn execute(
        &self,
        kind: FunctionKind,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, DeliveryError>;
}

/// Outbox worker configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxWorkerConfig {
    /// Maximum operations processed per run.
    pub batch_size: i64,
    /// Lease duration protecting work from concurrent workers.
    pub lease_duration_ms: i64,
    /// Stable worker identity.
    pub worker_id: String,
    /// Retry timing policy.
    pub retry_policy: RetryPolicy,
}

impl Default for OutboxWorkerConfig {
    fn default() -> Self {
        Self {
            batch_size: 25,
            lease_duration_ms: 60_000,
            worker_id: "mobile-sdk".to_owned(),
            retry_policy: RetryPolicy::default(),
        }
    }
}

/// Result of one bounded worker run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OutboxRunReport {
    /// Claimed operations.
    pub claimed: u64,
    /// Successfully acknowledged operations.
    pub acknowledged: u64,
    /// Operations scheduled for retry.
    pub retry_scheduled: u64,
    /// Authoritatively rejected operations.
    pub rejected: u64,
}

/// Runs durable function intents against the canonical backend.
pub struct OutboxWorker<T: ?Sized> {
    store: Arc<MobileStore>,
    transport: Arc<T>,
    config: OutboxWorkerConfig,
}

impl<T> OutboxWorker<T>
where
    T: FunctionTransport + ?Sized + 'static,
{
    /// Creates an outbox worker.
    pub const fn new(
        store: Arc<MobileStore>,
        transport: Arc<T>,
        config: OutboxWorkerConfig,
    ) -> Self {
        Self {
            store,
            transport,
            config,
        }
    }

    /// Processes one bounded batch.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when local durable state cannot be read or
    /// updated. Individual delivery failures are persisted and reflected in the
    /// report rather than returned.
    pub async fn drain_once(&self, now: i64) -> Result<OutboxRunReport, StoreError> {
        let repository = OutboxRepository::new(&self.store);
        let items = repository.claim_ready(
            now,
            self.config.batch_size,
            &self.config.worker_id,
            self.config.lease_duration_ms,
        )?;
        let mut report = OutboxRunReport {
            claimed: u64::try_from(items.len()).unwrap_or(u64::MAX),
            ..OutboxRunReport::default()
        };

        for item in items {
            match self
                .transport
                .execute(
                    item.function_kind,
                    &item.function_name,
                    item.payload.clone(),
                )
                .await
            {
                Ok(_) => {
                    repository.acknowledge(&item.operation_id, now)?;
                    report.acknowledged += 1;
                }
                Err(error) if error.retryable => {
                    let failed_attempts =
                        u32::try_from(item.attempt_count.saturating_add(1)).unwrap_or(u32::MAX);
                    let next_attempt = self.config.retry_policy.next_attempt_at(
                        &item.operation_id,
                        failed_attempts,
                        now,
                    );
                    repository.retry(
                        &item.operation_id,
                        now,
                        next_attempt,
                        &error.code,
                        &error.message,
                    )?;
                    report.retry_scheduled += 1;
                }
                Err(error) => {
                    repository.reject(&item.operation_id, now, &error.code, &error.message)?;
                    report.rejected += 1;
                }
            }
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use mobile_contracts::FunctionKind;
    use mobile_store::{DatabaseKey, MobileStore, NewOutboxItem, OutboxRepository};
    use serde_json::json;

    use super::{DeliveryError, FunctionTransport, OutboxWorker, OutboxWorkerConfig, RetryPolicy};

    struct FakeTransport {
        calls: AtomicUsize,
        retryable: bool,
    }

    #[async_trait::async_trait]
    impl FunctionTransport for FakeTransport {
        async fn execute(
            &self,
            _kind: FunctionKind,
            _name: &str,
            _args: serde_json::Value,
        ) -> Result<serde_json::Value, DeliveryError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.retryable {
                Err(DeliveryError {
                    code: "offline".to_owned(),
                    message: "network unavailable".to_owned(),
                    retryable: true,
                })
            } else {
                Ok(json!({"ok": true}))
            }
        }
    }

    #[tokio::test]
    async fn acknowledges_successful_mutations() {
        let store = test_store();
        enqueue(&store);
        let transport = Arc::new(FakeTransport {
            calls: AtomicUsize::new(0),
            retryable: false,
        });
        let worker = OutboxWorker::new(
            Arc::clone(&store),
            Arc::clone(&transport),
            OutboxWorkerConfig::default(),
        );
        let report = worker.drain_once(1_000).await.expect("worker should run");
        assert_eq!(report.acknowledged, 1);
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn persists_retryable_failures() {
        let store = test_store();
        let item = enqueue(&store);
        let worker = OutboxWorker::new(
            Arc::clone(&store),
            Arc::new(FakeTransport {
                calls: AtomicUsize::new(0),
                retryable: true,
            }),
            OutboxWorkerConfig {
                retry_policy: RetryPolicy {
                    base_delay_ms: 1_000,
                    maximum_delay_ms: 1_000,
                    jitter_ms: 0,
                },
                ..OutboxWorkerConfig::default()
            },
        );
        let report = worker.drain_once(1_000).await.expect("worker should run");
        assert_eq!(report.retry_scheduled, 1);
        let saved = OutboxRepository::new(&store)
            .get(&item.operation_id)
            .expect("item should load")
            .expect("item should exist");
        assert_eq!(saved.next_attempt_at, 2_000);
    }

    fn test_store() -> Arc<MobileStore> {
        Arc::new(
            MobileStore::open(
                ":memory:",
                DatabaseKey::new(&[0x33; 32]).expect("key should be valid"),
            )
            .expect("store should open"),
        )
    }

    fn enqueue(store: &MobileStore) -> mobile_store::OutboxItem {
        OutboxRepository::new(store)
            .enqueue(NewOutboxItem {
                operation_id: None,
                function_name: "tasks:create".to_owned(),
                entity_kind: Some("task".to_owned()),
                entity_id: None,
                org_id: Some("org-1".to_owned()),
                payload: json!({"orgId": "org-1", "title": "Check tire pressure"}),
                created_at: 1_000,
            })
            .expect("item should enqueue")
    }
}
