#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, NaiveDate, Utc};

use super::{
    BREAK_MINIMUM_MS, BREAK_REQUIRED_AFTER_MS, CYCLE_60_7_MS, CYCLE_70_8_MS, ComputeHosInput,
    ComputeHosResult, CycleRule, DRIVING_LIMIT_MS, DUTY_WINDOW_MS, DutyStatus, HosAlert,
    HosSnapshot, HosViolation, RESET_MINIMUM_MS, RecapDay, ViolationSeverity, normalize_events,
};

#[derive(Debug, Clone, Copy)]
struct Segment {
    status: DutyStatus,
    start: i64,
    end: i64,
}

/// Computes a local HOS projection matching the TypeScript HOS engine.
///
/// # Panics
///
/// Panics when `as_of` or an event timestamp is outside the range supported by
/// both JavaScript `Date` and `chrono`.
pub fn compute_hos(input: &ComputeHosInput) -> ComputeHosResult {
    let events = normalize_events(&input.events);
    let current_date = utc_date(input.as_of);
    let mut segments: Vec<Segment> = Vec::new();

    for event in events {
        if let Some(previous) = segments.last()
            && event.started_at > previous.end
        {
            segments.push(Segment {
                status: DutyStatus::OffDuty,
                start: previous.end,
                end: event.started_at,
            });
        }
        let end = event.ended_at.unwrap_or(input.as_of).min(input.as_of);
        if end > event.started_at {
            segments.push(Segment {
                status: event.status,
                start: event.started_at,
                end,
            });
        }
    }

    if segments.is_empty() {
        return ComputeHosResult {
            snapshot: default_snapshot(input.as_of),
            violations: Vec::new(),
            alerts: Vec::new(),
            recap: derive_recap(&[], &input.recap_days),
        };
    }

    let mut shift_started_at = None;
    let mut last_reset_at = None;
    let mut driving_accumulated = 0;
    let mut on_duty_accumulated = 0;
    let mut driving_since_break = 0;

    for (index, segment) in segments.iter().enumerate() {
        let duration = segment.end - segment.start;
        if segment.status.is_off_duty_or_sleeper() {
            let consecutive = consecutive_off_duty(&segments, index);
            if consecutive >= RESET_MINIMUM_MS {
                shift_started_at = None;
                driving_accumulated = 0;
                on_duty_accumulated = 0;
                driving_since_break = 0;
                last_reset_at = Some(segment.end);
            } else if consecutive >= BREAK_MINIMUM_MS {
                driving_since_break = 0;
            }
            continue;
        }
        if shift_started_at.is_none() {
            shift_started_at = Some(segment.start);
        }
        if segment.status == DutyStatus::Driving {
            driving_accumulated += duration;
            driving_since_break += duration;
        }
        if segment.status.is_on_duty() {
            on_duty_accumulated += duration;
        }
    }

    let last = segments.last().expect("segments are non-empty");
    let driving_remaining = (DRIVING_LIMIT_MS - driving_accumulated).max(0);
    let duty_elapsed = shift_started_at.map_or(0, |start| input.as_of - start);
    let duty_remaining = (DUTY_WINDOW_MS - duty_elapsed).max(0);
    let break_required =
        (driving_since_break > 0).then(|| (BREAK_REQUIRED_AFTER_MS - driving_since_break).max(0));
    let (cycle_limit, cycle_days) = cycle_limits(input.config.cycle_rule);
    let cycle_on_duty = recap_cycle_ms(&input.recap_days, cycle_days, input.as_of, &current_date)
        + current_day_on_duty(&segments, input.as_of, &current_date);

    let snapshot = HosSnapshot {
        hos_version: "v1".to_owned(),
        current_status: last.status,
        current_status_since: last.start,
        driving_remaining_ms: driving_remaining,
        duty_window_remaining_ms: duty_remaining,
        break_required_in_ms: break_required,
        cycle_remaining_ms: (cycle_limit - cycle_on_duty).max(0),
        shift_started_at,
        on_duty_accumulated_ms: on_duty_accumulated,
        driving_accumulated_ms: driving_accumulated,
        last_reset_at,
    };

    ComputeHosResult {
        violations: derive_violations(&snapshot, &segments, input.as_of),
        alerts: derive_alerts(&snapshot, input.config.reminder_lead_seconds),
        recap: derive_recap(&segments, &input.recap_days),
        snapshot,
    }
}

fn consecutive_off_duty(segments: &[Segment], up_to: usize) -> i64 {
    segments[..=up_to]
        .iter()
        .rev()
        .take_while(|segment| segment.status.is_off_duty_or_sleeper())
        .map(|segment| segment.end - segment.start)
        .sum()
}

fn default_snapshot(as_of: i64) -> HosSnapshot {
    HosSnapshot {
        hos_version: "v1".to_owned(),
        current_status: DutyStatus::OffDuty,
        current_status_since: as_of,
        driving_remaining_ms: DRIVING_LIMIT_MS,
        duty_window_remaining_ms: DUTY_WINDOW_MS,
        break_required_in_ms: None,
        cycle_remaining_ms: CYCLE_60_7_MS,
        shift_started_at: None,
        on_duty_accumulated_ms: 0,
        driving_accumulated_ms: 0,
        last_reset_at: None,
    }
}

const fn cycle_limits(rule: CycleRule) -> (i64, i64) {
    match rule {
        CycleRule::SixtySeven => (CYCLE_60_7_MS, 7),
        CycleRule::SeventyEight => (CYCLE_70_8_MS, 8),
    }
}

fn recap_cycle_ms(recap: &[RecapDay], cycle_days: i64, as_of: i64, current: &str) -> i64 {
    let cutoff = epoch_date(as_of) - Duration::days(cycle_days);
    let cutoff = cutoff.format("%Y-%m-%d").to_string();
    recap
        .iter()
        .filter(|day| day.date > cutoff && day.date.as_str() < current)
        .map(|day| ((day.on_duty_seconds + day.driving_seconds) * 1_000.0) as i64)
        .sum()
}

fn current_day_on_duty(segments: &[Segment], as_of: i64, current: &str) -> i64 {
    let day_start = NaiveDate::parse_from_str(current, "%Y-%m-%d")
        .expect("generated UTC date must parse")
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid")
        .and_utc()
        .timestamp_millis();
    segments
        .iter()
        .filter(|segment| segment.status.is_on_duty())
        .map(|segment| segment.end.min(as_of) - segment.start.max(day_start))
        .filter(|duration| *duration > 0)
        .sum()
}

fn derive_recap(segments: &[Segment], existing: &[RecapDay]) -> Vec<RecapDay> {
    let mut days: BTreeMap<String, RecapDay> = existing
        .iter()
        .map(|day| (day.date.clone(), day.clone()))
        .collect();
    for segment in segments {
        let mut cursor = segment.start;
        while cursor < segment.end {
            let date = utc_date(cursor);
            let parsed = NaiveDate::parse_from_str(&date, "%Y-%m-%d")
                .expect("generated UTC date must parse");
            let end_of_day = (parsed + Duration::days(1))
                .and_hms_opt(0, 0, 0)
                .expect("midnight is valid")
                .and_utc()
                .timestamp_millis();
            let segment_end = segment.end.min(end_of_day);
            let seconds = (segment_end - cursor) as f64 / 1_000.0;
            let day = days.entry(date.clone()).or_insert(RecapDay {
                date,
                on_duty_seconds: 0.0,
                driving_seconds: 0.0,
                off_duty_seconds: 0.0,
                sleeper_seconds: 0.0,
            });
            match segment.status {
                DutyStatus::Driving => day.driving_seconds += seconds,
                DutyStatus::OnDutyNotDriving => day.on_duty_seconds += seconds,
                DutyStatus::OffDuty => day.off_duty_seconds += seconds,
                DutyStatus::SleeperBerth => day.sleeper_seconds += seconds,
            }
            cursor = segment_end;
        }
    }
    days.into_values().collect()
}

fn derive_violations(
    snapshot: &HosSnapshot,
    segments: &[Segment],
    as_of: i64,
) -> Vec<HosViolation> {
    let mut violations = Vec::new();
    if snapshot.driving_accumulated_ms > DRIVING_LIMIT_MS {
        violations.push(HosViolation {
            rule: "driving_11h".to_owned(),
            severity: ViolationSeverity::Violation,
            started_at: driving_overage_start(segments, DRIVING_LIMIT_MS).unwrap_or(as_of),
            message: format!(
                "Exceeded 11-hour driving limit by {} minutes",
                ((snapshot.driving_accumulated_ms - DRIVING_LIMIT_MS) as f64 / 60_000.0).round()
            ),
        });
    }
    if let Some(start) = snapshot.shift_started_at {
        let elapsed = as_of - start;
        if elapsed > DUTY_WINDOW_MS {
            violations.push(HosViolation {
                rule: "duty_window_14h".to_owned(),
                severity: ViolationSeverity::Violation,
                started_at: start + DUTY_WINDOW_MS,
                message: format!(
                    "Exceeded 14-hour duty window by {} minutes",
                    ((elapsed - DUTY_WINDOW_MS) as f64 / 60_000.0).round()
                ),
            });
        }
    }
    if snapshot
        .break_required_in_ms
        .is_some_and(|remaining| remaining <= 0)
    {
        violations.push(HosViolation {
            rule: "break_30min".to_owned(),
            severity: ViolationSeverity::Violation,
            started_at: driving_overage_start(segments, BREAK_REQUIRED_AFTER_MS).unwrap_or(as_of),
            message: "Required 30-minute break not taken after 8 hours of cumulative driving"
                .to_owned(),
        });
    }
    if snapshot.cycle_remaining_ms <= 0 {
        violations.push(HosViolation {
            rule: "cycle_limit".to_owned(),
            severity: ViolationSeverity::Violation,
            started_at: as_of,
            message: "Exceeded cycle hours limit".to_owned(),
        });
    }
    violations
}

fn driving_overage_start(segments: &[Segment], limit: i64) -> Option<i64> {
    let mut accumulated = 0;
    for segment in segments {
        if segment.status == DutyStatus::Driving {
            let duration = segment.end - segment.start;
            if accumulated + duration > limit {
                return Some(segment.start + (limit - accumulated));
            }
            accumulated += duration;
        }
    }
    None
}

fn derive_alerts(snapshot: &HosSnapshot, lead_seconds: i64) -> Vec<HosAlert> {
    let lead = lead_seconds * 1_000;
    let mut alerts = Vec::new();
    push_alert(
        &mut alerts,
        "driving_11h",
        snapshot.driving_remaining_ms,
        lead,
        "Driving limit approaching",
    );
    push_alert(
        &mut alerts,
        "duty_window_14h",
        snapshot.duty_window_remaining_ms,
        lead,
        "Duty window approaching",
    );
    if let Some(remaining) = snapshot.break_required_in_ms
        && remaining > 0
        && remaining <= lead
    {
        alerts.push(HosAlert {
            alert_type: "approaching_limit".to_owned(),
            rule: "break_30min".to_owned(),
            remaining_ms: remaining,
            message: format!(
                "Break required in {} minutes",
                (remaining as f64 / 60_000.0).round()
            ),
        });
    }
    push_alert(
        &mut alerts,
        "cycle_limit",
        snapshot.cycle_remaining_ms,
        lead,
        "Cycle limit approaching",
    );
    alerts
}

fn push_alert(alerts: &mut Vec<HosAlert>, rule: &str, remaining: i64, lead: i64, label: &str) {
    if remaining > 0 && remaining <= lead {
        alerts.push(HosAlert {
            alert_type: "approaching_limit".to_owned(),
            rule: rule.to_owned(),
            remaining_ms: remaining,
            message: format!(
                "{label}: {} minutes remaining",
                (remaining as f64 / 60_000.0).round()
            ),
        });
    }
}

fn utc_date(timestamp_ms: i64) -> String {
    epoch_date(timestamp_ms).format("%Y-%m-%d").to_string()
}

fn epoch_date(timestamp_ms: i64) -> NaiveDate {
    DateTime::<Utc>::from_timestamp_millis(timestamp_ms)
        .expect("valid epoch millisecond timestamp")
        .date_naive()
}
