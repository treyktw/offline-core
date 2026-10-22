//! Shared HOS parity fixture tests.

use std::{fs, path::PathBuf};

use mobile_domain::{
    ComputeHosInput, DutyStatus, RouteHosLegality, RouteHosProjectionInput, compute_hos,
    project_hos_along_route,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ComputeCase {
    name: String,
    input: ComputeHosInput,
    expected: ComputeExpected,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ComputeExpected {
    current_status: DutyStatus,
    driving_remaining_ms: i64,
    duty_window_remaining_ms: i64,
    break_required_in_ms: Option<i64>,
    cycle_remaining_ms: i64,
    driving_accumulated_ms: i64,
    on_duty_accumulated_ms: i64,
    violation_rules: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RouteCase {
    name: String,
    input: RouteHosProjectionInput,
    expected: RouteExpected,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RouteExpected {
    legality: RouteHosLegality,
    breaks_required: i64,
    overnight_required: bool,
    hos_aware_eta_seconds: i64,
}

#[test]
fn compute_matches_shared_v1_fixtures() {
    let cases: Vec<ComputeCase> = read_fixture("hos/compute-v1.json");
    for case in cases {
        let result = compute_hos(&case.input);
        assert_eq!(
            result.snapshot.current_status, case.expected.current_status,
            "{}",
            case.name
        );
        assert_eq!(
            result.snapshot.driving_remaining_ms, case.expected.driving_remaining_ms,
            "{}",
            case.name
        );
        assert_eq!(
            result.snapshot.duty_window_remaining_ms, case.expected.duty_window_remaining_ms,
            "{}",
            case.name
        );
        assert_eq!(
            result.snapshot.break_required_in_ms, case.expected.break_required_in_ms,
            "{}",
            case.name
        );
        assert_eq!(
            result.snapshot.cycle_remaining_ms, case.expected.cycle_remaining_ms,
            "{}",
            case.name
        );
        assert_eq!(
            result.snapshot.driving_accumulated_ms, case.expected.driving_accumulated_ms,
            "{}",
            case.name
        );
        assert_eq!(
            result.snapshot.on_duty_accumulated_ms, case.expected.on_duty_accumulated_ms,
            "{}",
            case.name
        );
        let rules: Vec<_> = result
            .violations
            .iter()
            .map(|violation| violation.rule.clone())
            .collect();
        assert_eq!(rules, case.expected.violation_rules, "{}", case.name);
    }
}

#[test]
fn route_projection_matches_shared_v1_fixtures() {
    let cases: Vec<RouteCase> = read_fixture("hos/route-v1.json");
    for case in cases {
        let result = project_hos_along_route(&case.input);
        assert_eq!(result.legality, case.expected.legality, "{}", case.name);
        assert_eq!(
            result.breaks_required, case.expected.breaks_required,
            "{}",
            case.name
        );
        assert_eq!(
            result.overnight_required, case.expected.overnight_required,
            "{}",
            case.name
        );
        assert_eq!(
            result.hos_aware_eta_seconds, case.expected.hos_aware_eta_seconds,
            "{}",
            case.name
        );
    }
}

fn read_fixture<T: for<'de> Deserialize<'de>>(path: &str) -> T {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(path);
    let contents = fs::read_to_string(path).expect("fixture should be readable");
    serde_json::from_str(&contents).expect("fixture should be valid")
}
