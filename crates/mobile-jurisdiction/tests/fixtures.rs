//! Shared jurisdiction parity fixture tests.

use std::{fs, path::PathBuf};

use mobile_jurisdiction::{JurisdictionDataset, TrackPoint, attribute_track_to_jurisdictions};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Fixture {
    dataset: JurisdictionDataset,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    points: Vec<TrackPoint>,
    expected_jurisdictions: Vec<String>,
    unattributed: bool,
}

#[test]
fn attribution_matches_shared_v1_fixtures() {
    let fixture = read_fixture();
    for case in fixture.cases {
        let result = attribute_track_to_jurisdictions(&case.points, &fixture.dataset);
        let mut codes: Vec<_> = result
            .by_jurisdiction
            .iter()
            .map(|row| row.jurisdiction.clone())
            .collect();
        codes.sort();
        assert_eq!(codes, case.expected_jurisdictions, "{}", case.name);
        assert_eq!(
            result.unattributed_miles > 0.0,
            case.unattributed,
            "{}",
            case.name
        );
        assert!(result.total_miles >= result.unattributed_miles);
    }
}

fn read_fixture() -> Fixture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/jurisdiction/attribution-v1.json");
    let contents = fs::read_to_string(path).expect("fixture should be readable");
    serde_json::from_str(&contents).expect("fixture should be valid")
}
