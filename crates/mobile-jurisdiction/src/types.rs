use serde::{Deserialize, Serialize};

/// Longitude/latitude coordinate in `GeoJSON` order.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LngLat(pub f64, pub f64);

/// Minimum longitude, minimum latitude, maximum longitude, maximum latitude.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox(pub f64, pub f64, pub f64, pub f64);

/// One jurisdiction with one or more polygons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Jurisdiction {
    /// Two-character IFTA code.
    pub code: String,
    /// Multi-polygon with outer rings and optional holes.
    pub polygons: Vec<Vec<Vec<LngLat>>>,
    /// Precomputed bounds.
    pub bbox: BoundingBox,
}

/// Collection of jurisdiction boundaries.
pub type JurisdictionDataset = Vec<Jurisdiction>;

/// GPS point used for mileage attribution.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackPoint {
    /// Latitude.
    pub lat: f64,
    /// Longitude.
    pub lng: f64,
    /// Recording time in epoch milliseconds.
    pub recorded_at: i64,
}

/// Mileage attributed to one jurisdiction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JurisdictionMiles {
    /// Jurisdiction code.
    pub jurisdiction: String,
    /// Great-circle mileage.
    pub miles: f64,
}

/// Result of attributing a GPS track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributionResult {
    /// Mileage grouped by jurisdiction, descending by miles.
    pub by_jurisdiction: Vec<JurisdictionMiles>,
    /// Mileage outside all known polygons.
    pub unattributed_miles: f64,
    /// Total track mileage.
    pub total_miles: f64,
}
