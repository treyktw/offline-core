use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use mobile_contracts::{FunctionKind, find_function};
use mobile_store::{MobileStore, QuerySnapshotRepository, StoreError};
use tokio::sync::watch;

use crate::DeliveryError;

/// Stable description of one realtime backend query.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterializationSpec {
    /// Native-selected identity for the materialized result.
    pub subscription_key: String,
    /// Public backend query path.
    pub function_name: String,
    /// Query arguments.
    pub args: serde_json::Value,
}

impl MaterializationSpec {
    /// Validates a realtime query specification against the reviewed catalog.
    ///
    /// # Errors
    ///
    /// Returns [`DeliveryError`] when the key is blank, arguments are not an
    /// object, or the function is absent or not a query.
    pub fn new(
        subscription_key: impl Into<String>,
        function_name: impl Into<String>,
        args: serde_json::Value,
    ) -> Result<Self, DeliveryError> {
        let subscription_key = subscription_key.into();
        let function_name = function_name.into();
        if subscription_key.trim().is_empty() {
            return Err(contract_error(
                "invalid_subscription_key",
                "subscription key must not be blank",
            ));
        }
        if !args.is_object() {
            return Err(contract_error(
                "invalid_query_arguments",
                "query arguments must be a JSON object",
            ));
        }
        let contract = find_function(&function_name).ok_or_else(|| {
            contract_error(
                "unknown_query",
                "query is absent from the reviewed mobile contract catalog",
            )
        })?;
        if contract.kind != FunctionKind::Query {
            return Err(contract_error(
                "function_not_query",
                "only queries can be materialized",
            ));
        }
        Ok(Self {
            subscription_key,
            function_name,
            args,
        })
    }
}

/// Stream of realtime query results.
#[async_trait]
pub trait QueryEventStream: Send {
    /// Waits for the next complete query result.
    async fn next(&mut self) -> Option<Result<serde_json::Value, DeliveryError>>;
}

/// Transport capable of creating realtime query streams.
#[async_trait]
pub trait RealtimeTransport: Send + Sync {
    /// Subscribes to one public backend query.
    async fn subscribe_query(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<Box<dyn QueryEventStream>, DeliveryError>;
}

/// Realtime materializer reconnect behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RealtimeMaterializerConfig {
    /// Delay before recreating a failed or ended subscription.
    pub reconnect_delay_ms: u64,
}

impl Default for RealtimeMaterializerConfig {
    fn default() -> Self {
        Self {
            reconnect_delay_ms: 2_000,
        }
    }
}

/// Lifetime counters for one realtime materializer run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RealtimeRunReport {
    /// Number of subscription connections established.
    pub connections: u64,
    /// Complete query results persisted.
    pub materialized: u64,
    /// Subscription or query-result failures.
    pub delivery_errors: u64,
}

/// Reconnecting realtime query materializer.
pub struct RealtimeMaterializer<T: ?Sized> {
    store: Arc<MobileStore>,
    transport: Arc<T>,
    spec: MaterializationSpec,
    config: RealtimeMaterializerConfig,
}

impl<T> RealtimeMaterializer<T>
where
    T: RealtimeTransport + ?Sized + 'static,
{
    /// Creates a query materializer.
    pub const fn new(
        store: Arc<MobileStore>,
        transport: Arc<T>,
        spec: MaterializationSpec,
        config: RealtimeMaterializerConfig,
    ) -> Self {
        Self {
            store,
            transport,
            spec,
            config,
        }
    }

    /// Runs until cancellation, reconnecting when a subscription ends.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when a query result cannot be persisted.
    pub async fn run(
        &self,
        mut cancelled: watch::Receiver<bool>,
    ) -> Result<RealtimeRunReport, StoreError> {
        let mut report = RealtimeRunReport::default();
        while !*cancelled.borrow() {
            let subscription = tokio::select! {
                _ = cancelled.changed() => continue,
                result = self.transport.subscribe_query(
                    &self.spec.function_name,
                    self.spec.args.clone(),
                ) => result,
            };
            let mut stream = if let Ok(stream) = subscription {
                report.connections += 1;
                stream
            } else {
                report.delivery_errors += 1;
                if wait_for_reconnect(self.config.reconnect_delay_ms, &mut cancelled).await {
                    break;
                }
                continue;
            };

            loop {
                let next = tokio::select! {
                    _ = cancelled.changed() => None,
                    value = stream.next() => value,
                };
                match next {
                    Some(Ok(value)) => {
                        self.materialize_value(&value, now_epoch_ms())?;
                        report.materialized += 1;
                    }
                    Some(Err(_)) => {
                        report.delivery_errors += 1;
                        break;
                    }
                    None => break,
                }
            }
            if *cancelled.borrow() {
                break;
            }
            if wait_for_reconnect(self.config.reconnect_delay_ms, &mut cancelled).await {
                break;
            }
        }
        Ok(report)
    }

    /// Persists one complete query result.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when encryption or storage fails.
    pub fn materialize_value(
        &self,
        value: &serde_json::Value,
        cached_at: i64,
    ) -> Result<(), StoreError> {
        QuerySnapshotRepository::new(&self.store).materialize(
            &self.spec.subscription_key,
            &self.spec.function_name,
            &self.spec.args,
            value,
            None,
            cached_at,
        )?;
        Ok(())
    }
}

async fn wait_for_reconnect(delay_ms: u64, cancelled: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        () = tokio::time::sleep(std::time::Duration::from_millis(delay_ms)) => false,
        _ = cancelled.changed() => *cancelled.borrow(),
    }
}

fn contract_error(code: &str, message: &str) -> DeliveryError {
    DeliveryError {
        code: code.to_owned(),
        message: message.to_owned(),
        retryable: false,
    }
}

fn now_epoch_ms() -> i64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use mobile_store::{DatabaseKey, MobileStore, QuerySnapshotRepository};
    use serde_json::json;

    use super::{MaterializationSpec, RealtimeMaterializer, RealtimeMaterializerConfig};

    #[test]
    fn rejects_mutations_as_realtime_queries() {
        assert!(
            MaterializationSpec::new("task-create", "tasks:create", json!({"orgId": "org-1"}),)
                .is_err()
        );
    }

    #[test]
    fn persists_complete_query_values() {
        struct UnusedTransport;
        #[async_trait::async_trait]
        impl super::RealtimeTransport for UnusedTransport {
            async fn subscribe_query(
                &self,
                _name: &str,
                _args: serde_json::Value,
            ) -> Result<Box<dyn super::QueryEventStream>, crate::DeliveryError> {
                unreachable!("transport is not used by this test")
            }
        }

        let store = Arc::new(
            MobileStore::open(
                ":memory:",
                DatabaseKey::new(&[0x52; 32]).expect("key should be valid"),
            )
            .expect("store should open"),
        );
        let materializer = RealtimeMaterializer::new(
            Arc::clone(&store),
            Arc::new(UnusedTransport),
            MaterializationSpec::new("open-tasks", "tasks:list", json!({"orgId": "org-1"}))
                .expect("spec should be valid"),
            RealtimeMaterializerConfig::default(),
        );
        materializer
            .materialize_value(&json!([{"_id": "task-1"}]), 1_000)
            .expect("result should materialize");
        let snapshot = QuerySnapshotRepository::new(&store)
            .get("open-tasks")
            .expect("snapshot should load")
            .expect("snapshot should exist");
        assert_eq!(snapshot.revision, 1);
        assert_eq!(snapshot.result[0]["_id"], "task-1");
    }
}
