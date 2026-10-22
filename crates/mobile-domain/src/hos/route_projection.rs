use super::{
    BREAK_MINIMUM_MS, BREAK_REQUIRED_AFTER_MS, DRIVING_LIMIT_MS, DUTY_WINDOW_MS, RESET_MINIMUM_MS,
    RouteHosInsertion, RouteHosInsertionType, RouteHosLegality, RouteHosProjection,
    RouteHosProjectionInput,
};

const FUEL_STOP_SECONDS: i64 = 30 * 60;

/// Projects current HOS clocks over provider route sections.
#[allow(clippy::too_many_lines)]
pub fn project_hos_along_route(input: &RouteHosProjectionInput) -> RouteHosProjection {
    let sections: Vec<i64> = input
        .section_durations_seconds
        .iter()
        .copied()
        .filter(|duration| *duration > 0)
        .collect();
    let total: i64 = sections.iter().sum();
    if input.driving_remaining_seconds <= 0 || input.duty_window_remaining_seconds <= 0 {
        return illegal(
            total,
            "Driver has no remaining driving or on-duty time and cannot start this route.",
        );
    }
    if input.cycle_remaining_seconds <= 0 {
        return illegal(
            total,
            "Driver has no remaining cycle hours; a 34-hour restart is required.",
        );
    }

    let mut insertions = Vec::new();
    let mut warnings = Vec::new();
    let mut breaks_required = 0;
    let mut overnight_required = false;
    let mut driving_left = input.driving_remaining_seconds;
    let mut duty_left = input.duty_window_remaining_seconds;
    let mut break_left = input
        .break_required_in_seconds
        .unwrap_or(BREAK_REQUIRED_AFTER_MS / 1_000);
    let mut fuel_left = input.fuel_interval_seconds.filter(|value| *value > 0);
    let mut cumulative = 0;
    let maximum_insertions = sections.len() * 4 + 12;

    for (index, section) in sections.iter().enumerate() {
        let after_section_index = i64::try_from(index).unwrap_or(i64::MAX) - 1;
        let mut remaining = *section;
        while remaining > 0 {
            if insertions.len() > maximum_insertions {
                warnings.push(
                    "Route is too long to project Hours-of-Service reliably; treat the estimate with caution."
                        .to_owned(),
                );
                break;
            }
            if driving_left <= 0 || duty_left <= 0 {
                insertions.push(RouteHosInsertion {
                    insertion_type: RouteHosInsertionType::Overnight,
                    after_section_index,
                    at_driving_second: cumulative,
                    duration_seconds: RESET_MINIMUM_MS / 1_000,
                    reason: "10-hour reset required before the driver can continue legally."
                        .to_owned(),
                });
                overnight_required = true;
                driving_left = DRIVING_LIMIT_MS / 1_000;
                duty_left = DUTY_WINDOW_MS / 1_000;
                break_left = BREAK_REQUIRED_AFTER_MS / 1_000;
                continue;
            }
            if break_left <= 0 {
                insertions.push(RouteHosInsertion {
                    insertion_type: RouteHosInsertionType::Break,
                    after_section_index,
                    at_driving_second: cumulative,
                    duration_seconds: BREAK_MINIMUM_MS / 1_000,
                    reason: "30-minute break required after 8 hours of driving.".to_owned(),
                });
                breaks_required += 1;
                break_left = BREAK_REQUIRED_AFTER_MS / 1_000;
                duty_left -= BREAK_MINIMUM_MS / 1_000;
                continue;
            }
            if fuel_left.is_some_and(|value| value <= 0) {
                insertions.push(RouteHosInsertion {
                    insertion_type: RouteHosInsertionType::Fuel,
                    after_section_index,
                    at_driving_second: cumulative,
                    duration_seconds: FUEL_STOP_SECONDS,
                    reason: "Planned fuel stop.".to_owned(),
                });
                fuel_left = input.fuel_interval_seconds;
                duty_left -= FUEL_STOP_SECONDS;
                continue;
            }

            let mut step = remaining.min(driving_left).min(duty_left).min(break_left);
            if let Some(fuel) = fuel_left {
                step = step.min(fuel);
            }
            remaining -= step;
            driving_left -= step;
            duty_left -= step;
            break_left -= step;
            cumulative += step;
            if let Some(fuel) = &mut fuel_left {
                *fuel -= step;
            }
        }
    }

    let inserted: i64 = insertions.iter().map(|item| item.duration_seconds).sum();
    let mut legality = if overnight_required {
        RouteHosLegality::Warning
    } else {
        RouteHosLegality::Legal
    };
    if input.cycle_remaining_seconds < total {
        legality = RouteHosLegality::Warning;
        warnings.push(
            "Route exceeds the driver's remaining cycle hours; a 34-hour restart will be required to finish."
                .to_owned(),
        );
    }
    RouteHosProjection {
        hos_version: "v1".to_owned(),
        legality,
        breaks_required,
        overnight_required,
        insertions,
        hos_aware_eta_seconds: total + inserted,
        total_driving_seconds: total,
        warnings,
    }
}

fn illegal(total: i64, reason: &str) -> RouteHosProjection {
    RouteHosProjection {
        hos_version: "v1".to_owned(),
        legality: RouteHosLegality::Illegal,
        breaks_required: 0,
        overnight_required: false,
        insertions: Vec::new(),
        hos_aware_eta_seconds: total,
        total_driving_seconds: total,
        warnings: vec![reason.to_owned()],
    }
}
