use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Invalid operation identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidOperationId;

/// Stable identifier used to make backend mutation retries idempotent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperationId(String);

impl OperationId {
    /// Generates a new client-side operation identifier.
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Returns the wire representation sent to the backend.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parses and normalizes a UUID operation identifier.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidOperationId`] when the value is not a UUID.
    pub fn parse(value: &str) -> Result<Self, InvalidOperationId> {
        Uuid::parse_str(value)
            .map(|value| Self(value.to_string()))
            .map_err(|_| InvalidOperationId)
    }
}

impl Default for OperationId {
    fn default() -> Self {
        Self::new()
    }
}

/// Durable outbox processing state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboxState {
    /// Waiting for connectivity and authentication.
    Pending,
    /// Currently being sent.
    Sending,
    /// Waiting for the next retry window.
    RetryWait,
    /// Acknowledged by the backend.
    Acknowledged,
    /// Rejected by an authoritative non-retryable server decision.
    RejectedTerminal,
}

impl OutboxState {
    /// Returns the stable database representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Sending => "sending",
            Self::RetryWait => "retry_wait",
            Self::Acknowledged => "acknowledged",
            Self::RejectedTerminal => "rejected_terminal",
        }
    }

    /// Parses a stable database representation.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "sending" => Some(Self::Sending),
            "retry_wait" => Some(Self::RetryWait),
            "acknowledged" => Some(Self::Acknowledged),
            "rejected_terminal" => Some(Self::RejectedTerminal),
            _ => None,
        }
    }
}
