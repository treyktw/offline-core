//! Offline outbox orchestration and reconciliation policy.

pub mod mock;
mod realtime;
mod worker;

use std::hash::{Hash, Hasher};

pub use mobile_models::{InvalidOperationId, OperationId, OutboxState};
pub use realtime::{
    MaterializationSpec, QueryEventStream, RealtimeMaterializer, RealtimeMaterializerConfig,
    RealtimeRunReport, RealtimeTransport,
};
pub use worker::{
    DeliveryError, FunctionTransport, OutboxRunReport, OutboxWorker, OutboxWorkerConfig,
};

/// A complete backend connection: durable write delivery plus realtime reads.
///
/// The production build implemented this over a hosted realtime backend's
/// client. Anything that can execute a write and stream query results can be
/// plugged in here; [`mock::InMemoryServer`] is the in-process reference.
pub trait Transport: FunctionTransport + RealtimeTransport {}

impl<T> Transport for T where T: FunctionTransport + RealtimeTransport + ?Sized {}

/// Deterministic exponential retry policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Initial retry delay.
    pub base_delay_ms: i64,
    /// Maximum retry delay.
    pub maximum_delay_ms: i64,
    /// Maximum deterministic jitter.
    pub jitter_ms: i64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            base_delay_ms: 1_000,
            maximum_delay_ms: 5 * 60 * 1_000,
            jitter_ms: 500,
        }
    }
}

impl RetryPolicy {
    /// Computes the next attempt timestamp.
    pub fn next_attempt_at(
        self,
        operation_id: &OperationId,
        failed_attempt_count: u32,
        now: i64,
    ) -> i64 {
        let exponent = failed_attempt_count.min(20);
        let multiplier = 1_i64.checked_shl(exponent).unwrap_or(i64::MAX);
        let delay = self
            .base_delay_ms
            .saturating_mul(multiplier)
            .min(self.maximum_delay_ms);
        let jitter = if self.jitter_ms <= 0 {
            0
        } else {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            operation_id.hash(&mut hasher);
            i64::try_from(hasher.finish() % u64::try_from(self.jitter_ms).unwrap_or(1)).unwrap_or(0)
        };
        now.saturating_add(delay).saturating_add(jitter)
    }
}

#[cfg(test)]
mod tests {
    use super::{OperationId, RetryPolicy};

    #[test]
    fn retry_delay_grows_and_caps() {
        let id = OperationId::parse("f6ff5a19-0f3f-4e31-9ac2-ea171538bf38")
            .expect("operation ID should parse");
        let policy = RetryPolicy {
            base_delay_ms: 1_000,
            maximum_delay_ms: 8_000,
            jitter_ms: 0,
        };
        assert_eq!(policy.next_attempt_at(&id, 0, 100), 1_100);
        assert_eq!(policy.next_attempt_at(&id, 1, 100), 2_100);
        assert_eq!(policy.next_attempt_at(&id, 10, 100), 8_100);
    }
}
