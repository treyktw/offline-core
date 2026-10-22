use std::collections::HashMap;

use super::{
    AttributionResult, JurisdictionDataset, JurisdictionMiles, TrackPoint, bbox_contains,
    haversine_miles, point_in_polygon,
};

/// Returns the first jurisdiction containing a latitude/longitude point.
pub fn point_in_jurisdiction(lat: f64, lng: f64, dataset: &JurisdictionDataset) -> Option<&str> {
    dataset.iter().find_map(|jurisdiction| {
        if !bbox_contains(jurisdiction.bbox, lng, lat) {
            return None;
        }
        jurisdiction
            .polygons
            .iter()
            .any(|polygon| point_in_polygon(lng, lat, polygon))
            .then_some(jurisdiction.code.as_str())
    })
}

/// Attributes consecutive GPS segment mileage to each segment midpoint.
pub fn attribute_track_to_jurisdictions(
    points: &[TrackPoint],
    dataset: &JurisdictionDataset,
) -> AttributionResult {
    let mut sorted = points.to_vec();
    sorted.sort_by_key(|point| point.recorded_at);
    let mut by_jurisdiction: HashMap<String, f64> = HashMap::new();
    let mut unattributed_miles = 0.0;
    let mut total_miles = 0.0;

    for pair in sorted.windows(2) {
        let previous = pair[0];
        let current = pair[1];
        let miles = haversine_miles(previous.lat, previous.lng, current.lat, current.lng);
        if miles <= 0.0 {
            continue;
        }
        total_miles += miles;
        let midpoint_lat = f64::midpoint(previous.lat, current.lat);
        let midpoint_lng = f64::midpoint(previous.lng, current.lng);
        if let Some(code) = point_in_jurisdiction(midpoint_lat, midpoint_lng, dataset) {
            *by_jurisdiction.entry(code.to_owned()).or_default() += miles;
        } else {
            unattributed_miles += miles;
        }
    }

    let mut grouped: Vec<_> = by_jurisdiction
        .into_iter()
        .map(|(jurisdiction, miles)| JurisdictionMiles {
            jurisdiction,
            miles,
        })
        .collect();
    grouped.sort_by(|left, right| {
        right
            .miles
            .total_cmp(&left.miles)
            .then_with(|| left.jurisdiction.cmp(&right.jurisdiction))
    });

    AttributionResult {
        by_jurisdiction: grouped,
        unattributed_miles,
        total_miles,
    }
}
