//! Offline jurisdiction mileage attribution exposed to Swift and Kotlin.

use mobile_jurisdiction as engine;

use crate::SdkError;

/// GPS point used for mileage attribution.
#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct TrackPoint {
    /// Latitude.
    pub lat: f64,
    /// Longitude.
    pub lng: f64,
    /// Recording time in epoch milliseconds.
    pub recorded_at: i64,
}

/// Mileage attributed to one jurisdiction.
#[derive(Debug, Clone, uniffi::Record)]
pub struct JurisdictionMiles {
    /// Jurisdiction code.
    pub jurisdiction: String,
    /// Great-circle mileage.
    pub miles: f64,
}

/// Result of attributing a GPS track.
#[derive(Debug, Clone, uniffi::Record)]
pub struct MileageAttribution {
    /// Mileage grouped by jurisdiction, descending by miles.
    pub by_jurisdiction: Vec<JurisdictionMiles>,
    /// Mileage outside all known polygons.
    pub unattributed_miles: f64,
    /// Total track mileage.
    pub total_miles: f64,
}

/// Attributes a GPS track to the polygons in a `GeoJSON` feature collection.
///
/// # Errors
///
/// Returns [`SdkError::InvalidJurisdictionDataset`] when the dataset is not a
/// valid `GeoJSON` feature collection.
#[uniffi::export]
pub fn attribute_track_miles(
    geojson: String,
    points: Vec<TrackPoint>,
) -> Result<MileageAttribution, SdkError> {
    let value: serde_json::Value =
        serde_json::from_str(&geojson).map_err(|_| SdkError::InvalidJurisdictionDataset)?;
    drop(geojson);
    let dataset =
        engine::parse_geojson_dataset(&value).map_err(|_| SdkError::InvalidJurisdictionDataset)?;
    let points: Vec<engine::TrackPoint> = points
        .into_iter()
        .map(|point| engine::TrackPoint {
            lat: point.lat,
            lng: point.lng,
            recorded_at: point.recorded_at,
        })
        .collect();
    let result = engine::attribute_track_to_jurisdictions(&points, &dataset);
    Ok(MileageAttribution {
        by_jurisdiction: result
            .by_jurisdiction
            .into_iter()
            .map(|row| JurisdictionMiles {
                jurisdiction: row.jurisdiction,
                miles: row.miles,
            })
            .collect(),
        unattributed_miles: result.unattributed_miles,
        total_miles: result.total_miles,
    })
}
