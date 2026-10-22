//! FMCSA property-carrying HOS projections ported from the server-side
//! TypeScript HOS engine.
//!
//! These calculations support immediate and offline mobile behavior. The
//! TypeScript backend remains authoritative after synchronization.

mod compute;
mod normalize;
mod route_projection;
mod types;

pub use compute::compute_hos;
pub use normalize::normalize_events;
pub use route_projection::project_hos_along_route;
pub use types::{
    ComputeHosInput, ComputeHosResult, CycleRule, DutyEvent, DutyStatus, EventSource, HosAlert,
    HosConfig, HosSnapshot, HosViolation, RecapDay, RouteHosInsertion, RouteHosInsertionType,
    RouteHosLegality, RouteHosProjection, RouteHosProjectionInput, ViolationSeverity,
};

/// Eleven-hour driving limit in milliseconds.
pub const DRIVING_LIMIT_MS: i64 = 11 * 60 * 60 * 1_000;
/// Fourteen-hour duty-window limit in milliseconds.
pub const DUTY_WINDOW_MS: i64 = 14 * 60 * 60 * 1_000;
/// Driving time after which a break is required.
pub const BREAK_REQUIRED_AFTER_MS: i64 = 8 * 60 * 60 * 1_000;
/// Minimum qualifying break duration.
pub const BREAK_MINIMUM_MS: i64 = 30 * 60 * 1_000;
/// Minimum off-duty reset duration.
pub const RESET_MINIMUM_MS: i64 = 10 * 60 * 60 * 1_000;
/// Sixty-hour/seven-day cycle limit.
pub const CYCLE_60_7_MS: i64 = 60 * 60 * 60 * 1_000;
/// Seventy-hour/eight-day cycle limit.
pub const CYCLE_70_8_MS: i64 = 70 * 60 * 60 * 1_000;
