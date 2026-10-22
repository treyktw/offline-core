use mobile_domain as domain;

/// Driver duty status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
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

/// HOS cycle rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CycleRule {
    /// Sixty hours in seven days.
    SixtySeven,
    /// Seventy hours in eight days.
    SeventyEight,
}

/// Duty event source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EventSource {
    /// Driver-entered event.
    Driver,
    /// Automatically proposed event.
    Auto,
    /// Administrative correction.
    AdminEdit,
}

/// Duty-status event.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DutyEvent {
    /// Stable event identifier.
    pub id: String,
    /// Driver identifier.
    pub driver_id: String,
    /// Status.
    pub status: DutyStatus,
    /// Start in epoch milliseconds.
    pub started_at: i64,
    /// Optional end in epoch milliseconds.
    pub ended_at: Option<i64>,
    /// Source.
    pub source: EventSource,
    /// Superseded event identifier.
    pub edit_of: Option<String>,
}

/// HOS calculation configuration.
#[derive(Debug, Clone, uniffi::Record)]
pub struct HosConfig {
    /// Cycle rule.
    pub cycle_rule: CycleRule,
    /// Alert lead time in seconds.
    pub reminder_lead_seconds: i64,
}

/// One UTC recap day.
#[derive(Debug, Clone, uniffi::Record)]
pub struct RecapDay {
    /// Date in `YYYY-MM-DD` form.
    pub date: String,
    /// On-duty non-driving seconds.
    pub on_duty_seconds: f64,
    /// Driving seconds.
    pub driving_seconds: f64,
    /// Off-duty seconds.
    pub off_duty_seconds: f64,
    /// Sleeper seconds.
    pub sleeper_seconds: f64,
}

/// HOS input.
#[derive(Debug, Clone, uniffi::Record)]
pub struct HosInput {
    /// Duty events.
    pub events: Vec<DutyEvent>,
    /// Configuration.
    pub config: HosConfig,
    /// Projection time in epoch milliseconds.
    pub as_of: i64,
    /// Existing recap rows.
    pub recap_days: Vec<RecapDay>,
}

/// Current projected clocks.
#[derive(Debug, Clone, uniffi::Record)]
pub struct HosSnapshot {
    /// Contract version.
    pub hos_version: String,
    /// Current status.
    pub current_status: DutyStatus,
    /// Current status start.
    pub current_status_since: i64,
    /// Remaining driving milliseconds.
    pub driving_remaining_ms: i64,
    /// Remaining duty-window milliseconds.
    pub duty_window_remaining_ms: i64,
    /// Time until break requirement.
    pub break_required_in_ms: Option<i64>,
    /// Remaining cycle milliseconds.
    pub cycle_remaining_ms: i64,
    /// Shift start.
    pub shift_started_at: Option<i64>,
    /// Accumulated on-duty milliseconds.
    pub on_duty_accumulated_ms: i64,
    /// Accumulated driving milliseconds.
    pub driving_accumulated_ms: i64,
    /// Last reset completion.
    pub last_reset_at: Option<i64>,
}

/// Violation severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ViolationSeverity {
    /// Warning.
    Warning,
    /// Active violation.
    Violation,
}

/// Active violation.
#[derive(Debug, Clone, uniffi::Record)]
pub struct HosViolation {
    /// Rule identifier.
    pub rule: String,
    /// Severity.
    pub severity: ViolationSeverity,
    /// Start in epoch milliseconds.
    pub started_at: i64,
    /// Human-readable explanation.
    pub message: String,
}

/// Approaching-limit alert.
#[derive(Debug, Clone, uniffi::Record)]
pub struct HosAlert {
    /// Alert type.
    pub alert_type: String,
    /// Rule identifier.
    pub rule: String,
    /// Remaining milliseconds.
    pub remaining_ms: i64,
    /// Human-readable explanation.
    pub message: String,
}

/// Local HOS calculation result.
#[derive(Debug, Clone, uniffi::Record)]
pub struct HosResult {
    /// Current snapshot.
    pub snapshot: HosSnapshot,
    /// Active violations.
    pub violations: Vec<HosViolation>,
    /// Approaching-limit alerts.
    pub alerts: Vec<HosAlert>,
    /// UTC recap.
    pub recap: Vec<RecapDay>,
}

/// Route insertion type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RouteHosInsertionType {
    /// Thirty-minute break.
    Break,
    /// Planned fuel stop.
    Fuel,
    /// Ten-hour reset.
    Overnight,
}

/// Route legality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RouteHosLegality {
    /// Fits current clocks.
    Legal,
    /// Requires reset or exceeds cycle.
    Warning,
    /// Cannot start legally.
    Illegal,
}

/// Route-HOS input.
#[derive(Debug, Clone, uniffi::Record)]
pub struct RouteHosProjectionInput {
    /// Remaining driving seconds.
    pub driving_remaining_seconds: i64,
    /// Remaining duty-window seconds.
    pub duty_window_remaining_seconds: i64,
    /// Time until break.
    pub break_required_in_seconds: Option<i64>,
    /// Remaining cycle seconds.
    pub cycle_remaining_seconds: i64,
    /// Provider section durations.
    pub section_durations_seconds: Vec<i64>,
    /// Optional fuel interval.
    pub fuel_interval_seconds: Option<i64>,
}

/// Inserted route stop.
#[derive(Debug, Clone, uniffi::Record)]
pub struct RouteHosInsertion {
    /// Insertion type.
    pub insertion_type: RouteHosInsertionType,
    /// Section completed before insertion.
    pub after_section_index: i64,
    /// Cumulative driving seconds.
    pub at_driving_second: i64,
    /// Stop duration.
    pub duration_seconds: i64,
    /// Human-readable reason.
    pub reason: String,
}

/// Route-HOS result.
#[derive(Debug, Clone, uniffi::Record)]
pub struct RouteHosProjection {
    /// Contract version.
    pub hos_version: String,
    /// Route legality.
    pub legality: RouteHosLegality,
    /// Required break count.
    pub breaks_required: i64,
    /// Whether an overnight is required.
    pub overnight_required: bool,
    /// Inserted stops.
    pub insertions: Vec<RouteHosInsertion>,
    /// HOS-aware ETA.
    pub hos_aware_eta_seconds: i64,
    /// Provider driving seconds.
    pub total_driving_seconds: i64,
    /// Warnings.
    pub warnings: Vec<String>,
}

/// Computes provisional local HOS clocks.
#[uniffi::export]
pub fn compute_hos_projection(input: HosInput) -> HosResult {
    domain::compute_hos(&input.into()).into()
}

/// Projects HOS constraints over route sections.
#[uniffi::export]
pub fn project_hos_route(input: RouteHosProjectionInput) -> RouteHosProjection {
    domain::project_hos_along_route(&input.into()).into()
}

impl From<HosInput> for domain::ComputeHosInput {
    fn from(value: HosInput) -> Self {
        Self {
            events: value.events.into_iter().map(Into::into).collect(),
            config: value.config.into(),
            as_of: value.as_of,
            recap_days: value.recap_days.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<DutyEvent> for domain::DutyEvent {
    fn from(value: DutyEvent) -> Self {
        Self {
            id: value.id,
            driver_id: value.driver_id,
            status: value.status.into(),
            started_at: value.started_at,
            ended_at: value.ended_at,
            location: None,
            source: value.source.into(),
            edit_of: value.edit_of,
        }
    }
}

impl From<HosConfig> for domain::HosConfig {
    fn from(value: HosConfig) -> Self {
        Self {
            cycle_rule: value.cycle_rule.into(),
            reminder_lead_seconds: value.reminder_lead_seconds,
        }
    }
}

impl From<RecapDay> for domain::RecapDay {
    fn from(value: RecapDay) -> Self {
        Self {
            date: value.date,
            on_duty_seconds: value.on_duty_seconds,
            driving_seconds: value.driving_seconds,
            off_duty_seconds: value.off_duty_seconds,
            sleeper_seconds: value.sleeper_seconds,
        }
    }
}

impl From<domain::ComputeHosResult> for HosResult {
    fn from(value: domain::ComputeHosResult) -> Self {
        Self {
            snapshot: value.snapshot.into(),
            violations: value.violations.into_iter().map(Into::into).collect(),
            alerts: value.alerts.into_iter().map(Into::into).collect(),
            recap: value.recap.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<domain::HosSnapshot> for HosSnapshot {
    fn from(value: domain::HosSnapshot) -> Self {
        Self {
            hos_version: value.hos_version,
            current_status: value.current_status.into(),
            current_status_since: value.current_status_since,
            driving_remaining_ms: value.driving_remaining_ms,
            duty_window_remaining_ms: value.duty_window_remaining_ms,
            break_required_in_ms: value.break_required_in_ms,
            cycle_remaining_ms: value.cycle_remaining_ms,
            shift_started_at: value.shift_started_at,
            on_duty_accumulated_ms: value.on_duty_accumulated_ms,
            driving_accumulated_ms: value.driving_accumulated_ms,
            last_reset_at: value.last_reset_at,
        }
    }
}

impl From<domain::HosViolation> for HosViolation {
    fn from(value: domain::HosViolation) -> Self {
        Self {
            rule: value.rule,
            severity: value.severity.into(),
            started_at: value.started_at,
            message: value.message,
        }
    }
}

impl From<domain::HosAlert> for HosAlert {
    fn from(value: domain::HosAlert) -> Self {
        Self {
            alert_type: value.alert_type,
            rule: value.rule,
            remaining_ms: value.remaining_ms,
            message: value.message,
        }
    }
}

impl From<domain::RecapDay> for RecapDay {
    fn from(value: domain::RecapDay) -> Self {
        Self {
            date: value.date,
            on_duty_seconds: value.on_duty_seconds,
            driving_seconds: value.driving_seconds,
            off_duty_seconds: value.off_duty_seconds,
            sleeper_seconds: value.sleeper_seconds,
        }
    }
}

impl From<RouteHosProjectionInput> for domain::RouteHosProjectionInput {
    fn from(value: RouteHosProjectionInput) -> Self {
        Self {
            driving_remaining_seconds: value.driving_remaining_seconds,
            duty_window_remaining_seconds: value.duty_window_remaining_seconds,
            break_required_in_seconds: value.break_required_in_seconds,
            cycle_remaining_seconds: value.cycle_remaining_seconds,
            section_durations_seconds: value.section_durations_seconds,
            fuel_interval_seconds: value.fuel_interval_seconds,
        }
    }
}

impl From<domain::RouteHosProjection> for RouteHosProjection {
    fn from(value: domain::RouteHosProjection) -> Self {
        Self {
            hos_version: value.hos_version,
            legality: value.legality.into(),
            breaks_required: value.breaks_required,
            overnight_required: value.overnight_required,
            insertions: value.insertions.into_iter().map(Into::into).collect(),
            hos_aware_eta_seconds: value.hos_aware_eta_seconds,
            total_driving_seconds: value.total_driving_seconds,
            warnings: value.warnings,
        }
    }
}

impl From<domain::RouteHosInsertion> for RouteHosInsertion {
    fn from(value: domain::RouteHosInsertion) -> Self {
        Self {
            insertion_type: value.insertion_type.into(),
            after_section_index: value.after_section_index,
            at_driving_second: value.at_driving_second,
            duration_seconds: value.duration_seconds,
            reason: value.reason,
        }
    }
}

macro_rules! bidirectional_enum {
    ($ffi:ty, $domain:ty, {$($variant:ident),+ $(,)?}) => {
        impl From<$ffi> for $domain {
            fn from(value: $ffi) -> Self {
                match value {
                    $(<$ffi>::$variant => <$domain>::$variant,)+
                }
            }
        }
        impl From<$domain> for $ffi {
            fn from(value: $domain) -> Self {
                match value {
                    $(<$domain>::$variant => <$ffi>::$variant,)+
                }
            }
        }
    };
}

bidirectional_enum!(DutyStatus, domain::DutyStatus, {
    OffDuty,
    SleeperBerth,
    Driving,
    OnDutyNotDriving
});
bidirectional_enum!(CycleRule, domain::CycleRule, { SixtySeven, SeventyEight });
bidirectional_enum!(EventSource, domain::EventSource, {
    Driver,
    Auto,
    AdminEdit
});
bidirectional_enum!(
    ViolationSeverity,
    domain::ViolationSeverity,
    { Warning, Violation }
);
bidirectional_enum!(
    RouteHosInsertionType,
    domain::RouteHosInsertionType,
    { Break, Fuel, Overnight }
);
bidirectional_enum!(
    RouteHosLegality,
    domain::RouteHosLegality,
    { Legal, Warning, Illegal }
);
