//! Deterministic local projections used by the native applications.
//!
//! The TypeScript backend remains authoritative for synchronized business truth.

use mobile_models::DataAuthority;
use serde::{Deserialize, Serialize};

pub mod hos;

pub use hos::{
    ComputeHosInput, ComputeHosResult, CycleRule, DutyEvent, DutyStatus, EventSource, HosAlert,
    HosConfig, HosSnapshot, HosViolation, RecapDay, RouteHosInsertion, RouteHosInsertionType,
    RouteHosLegality, RouteHosProjection, RouteHosProjectionInput, ViolationSeverity, compute_hos,
    normalize_events, project_hos_along_route,
};

/// Wraps a locally computed value with its current authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Projection<T> {
    /// Projected value.
    pub value: T,
    /// Whether the server has confirmed the value.
    pub authority: DataAuthority,
}

impl<T> Projection<T> {
    /// Creates a provisional result that must later be reconciled with the server.
    pub const fn provisional(value: T) -> Self {
        Self {
            value,
            authority: DataAuthority::LocalProvisional,
        }
    }
}
