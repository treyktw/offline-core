//! Platform-neutral internal models for the offline mobile core.

use serde::{Deserialize, Serialize};

mod sync;

pub use sync::{InvalidOperationId, OperationId, OutboxState};

/// Backend environment selected by the native application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Environment {
    /// Local or shared development services.
    Development,
    /// Production-like validation services.
    Staging,
    /// Live customer services.
    Production,
}

/// Identifies whether locally visible state has been confirmed by the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataAuthority {
    /// Derived or written locally while awaiting server confirmation.
    LocalProvisional,
    /// Confirmed by the canonical TypeScript backend.
    ServerConfirmed,
    /// Replaced locally after the server returned a different authoritative value.
    ServerCorrected,
}

/// Current operating-system lifecycle state supplied by the native shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationState {
    /// The application is visible and interactive.
    Active,
    /// The application is temporarily inactive.
    Inactive,
    /// The application is executing in the background.
    Background,
}

/// Current network reachability supplied by the native shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkState {
    /// No usable network is available.
    Offline,
    /// A network is available but may be constrained.
    Constrained,
    /// A normal network connection is available.
    Online,
}

/// Internal configuration used to initialize backend adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreConfig {
    /// Selected deployment environment.
    pub environment: Environment,
    /// URL of the shared backend deployment for that environment.
    pub backend_url: String,
    /// Stable application installation identifier.
    pub installation_id: String,
}
