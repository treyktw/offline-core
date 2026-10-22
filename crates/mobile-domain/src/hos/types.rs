use serde::{Deserialize, Serialize};

/// Driver duty status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DutyStatus {
    /// Off-duty time.
    OffDuty,
    /// Sleeper-berth time.
    SleeperBerth,
    /// Driving time.
    Driving,
    /// On duty without driving.
    OnDutyNotDriving,
}

impl DutyStatus {
    pub(crate) const fn is_off_duty_or_sleeper(self) -> bool {
        matches!(self, Self::OffDuty | Self::SleeperBerth)
    }

    pub(crate) const fn is_on_duty(self) -> bool {
        matches!(self, Self::Driving | Self::OnDutyNotDriving)
    }
}

/// FMCSA cycle configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CycleRule {
    /// Sixty hours in seven days.
    #[serde(rename = "60_7")]
    SixtySeven,
    /// Seventy hours in eight days.
    #[serde(rename = "70_8")]
    SeventyEight,
}

/// Source of a duty event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    /// Driver-entered event.
    Driver,
    /// Automatically proposed event.
    Auto,
    /// Administrative correction.
    AdminEdit,
}

impl EventSource {
    pub(crate) const fn priority(self) -> u8 {
        match self {
            Self::AdminEdit => 3,
            Self::Driver => 2,
            Self::Auto => 1,
        }
    }
}

/// Optional event location.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DutyLocation {
    /// Latitude.
    pub lat: f64,
    /// Longitude.
    pub lng: f64,
    /// Human-readable location.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Duty-status event consumed by the local HOS projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DutyEvent {
    /// Stable event identifier.
    pub id: String,
    /// Driver identifier.
    pub driver_id: String,
    /// Duty status.
    pub status: DutyStatus,
    /// Event start in epoch milliseconds.
    pub started_at: i64,
    /// Optional event end in epoch milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    /// Optional event location.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<DutyLocation>,
    /// Event source.
    pub source: EventSource,
    /// Identifier of a superseded event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit_of: Option<String>,
}

/// HOS engine configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HosConfig {
    /// Cycle rule.
    pub cycle_rule: CycleRule,
    /// Alert lead time in seconds.
    pub reminder_lead_seconds: i64,
}

/// One UTC recap day.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecapDay {
    /// UTC date in `YYYY-MM-DD` form.
    pub date: String,
    /// On-duty non-driving seconds.
    pub on_duty_seconds: f64,
    /// Driving seconds.
    pub driving_seconds: f64,
    /// Off-duty seconds.
    pub off_duty_seconds: f64,
    /// Sleeper-berth seconds.
    pub sleeper_seconds: f64,
}

/// Current HOS clock projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HosSnapshot {
    /// Engine contract version.
    pub hos_version: String,
    /// Current duty status.
    pub current_status: DutyStatus,
    /// Start of current status in epoch milliseconds.
    pub current_status_since: i64,
    /// Remaining driving milliseconds.
    pub driving_remaining_ms: i64,
    /// Remaining duty-window milliseconds.
    pub duty_window_remaining_ms: i64,
    /// Milliseconds until a break is required.
    pub break_required_in_ms: Option<i64>,
    /// Remaining cycle milliseconds.
    pub cycle_remaining_ms: i64,
    /// Shift start in epoch milliseconds.
    pub shift_started_at: Option<i64>,
    /// Accumulated on-duty milliseconds.
    pub on_duty_accumulated_ms: i64,
    /// Accumulated driving milliseconds.
    pub driving_accumulated_ms: i64,
    /// End of the last qualifying reset.
    pub last_reset_at: Option<i64>,
}

/// Violation severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationSeverity {
    /// Warning severity.
    Warning,
    /// Active violation.
    Violation,
}

/// Active HOS violation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HosViolation {
    /// Stable rule identifier.
    pub rule: String,
    /// Severity.
    pub severity: ViolationSeverity,
    /// Violation start in epoch milliseconds.
    pub started_at: i64,
    /// Human-readable explanation.
    pub message: String,
}

/// Approaching-limit alert.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HosAlert {
    /// Stable alert type.
    #[serde(rename = "type")]
    pub alert_type: String,
    /// Rule being approached.
    pub rule: String,
    /// Remaining milliseconds.
    pub remaining_ms: i64,
    /// Human-readable explanation.
    pub message: String,
}

/// Input to the local HOS projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputeHosInput {
    /// Duty events.
    pub events: Vec<DutyEvent>,
    /// Engine configuration.
    pub config: HosConfig,
    /// Projection timestamp in epoch milliseconds.
    pub as_of: i64,
    /// Existing recap rows.
    #[serde(default)]
    pub recap_days: Vec<RecapDay>,
}

/// HOS projection result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputeHosResult {
    /// Current snapshot.
    pub snapshot: HosSnapshot,
    /// Active violations.
    pub violations: Vec<HosViolation>,
    /// Approaching-limit alerts.
    pub alerts: Vec<HosAlert>,
    /// UTC daily recap.
    pub recap: Vec<RecapDay>,
}

/// Route insertion type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteHosInsertionType {
    /// Thirty-minute break.
    Break,
    /// Planned fuel stop.
    Fuel,
    /// Ten-hour reset.
    Overnight,
}

/// Stop inserted into an HOS-aware route projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteHosInsertion {
    /// Insertion type.
    #[serde(rename = "type")]
    pub insertion_type: RouteHosInsertionType,
    /// Section completed before the stop.
    pub after_section_index: i64,
    /// Cumulative driving seconds.
    pub at_driving_second: i64,
    /// Stop duration.
    pub duration_seconds: i64,
    /// Human-readable reason.
    pub reason: String,
}

/// HOS-aware route legality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteHosLegality {
    /// Route fits current HOS clocks.
    Legal,
    /// Route requires a reset or exceeds remaining cycle hours.
    Warning,
    /// Driver cannot start legally.
    Illegal,
}

/// Input to route-HOS projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteHosProjectionInput {
    /// Remaining driving seconds.
    pub driving_remaining_seconds: i64,
    /// Remaining duty-window seconds.
    pub duty_window_remaining_seconds: i64,
    /// Seconds until a break is required.
    pub break_required_in_seconds: Option<i64>,
    /// Remaining cycle seconds.
    pub cycle_remaining_seconds: i64,
    /// Provider route section durations.
    pub section_durations_seconds: Vec<i64>,
    /// Optional planned fuel interval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fuel_interval_seconds: Option<i64>,
}

/// HOS-aware route projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteHosProjection {
    /// Engine contract version.
    pub hos_version: String,
    /// Route legality.
    pub legality: RouteHosLegality,
    /// Required break count.
    pub breaks_required: i64,
    /// Whether an overnight reset is required.
    pub overnight_required: bool,
    /// Inserted stops.
    pub insertions: Vec<RouteHosInsertion>,
    /// Driving and inserted dwell seconds.
    pub hos_aware_eta_seconds: i64,
    /// Total provider driving seconds.
    pub total_driving_seconds: i64,
    /// Projection warnings.
    pub warnings: Vec<String>,
}
