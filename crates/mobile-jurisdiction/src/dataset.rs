use std::collections::BTreeMap;

use serde_json::Value;
use thiserror::Error;

use super::{Jurisdiction, JurisdictionDataset, LngLat, polygon_bbox};

const CODE_KEYS: &[&str] = &[
    "STUSPS",
    "postal",
    "iso_3166_2",
    "abbr",
    "state_abbr",
    "STATE_ABBR",
    "code",
];
const NAME_KEYS: &[&str] = &["name", "NAME", "NAME_1", "state_name", "STATE_NAME"];

/// `GeoJSON` parsing failures.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DatasetError {
    /// Root was not a `GeoJSON` feature collection.
    #[error("jurisdiction dataset must be a GeoJSON FeatureCollection")]
    InvalidFeatureCollection,
    /// Coordinate arrays contained a non-numeric or missing coordinate.
    #[error("jurisdiction dataset contains invalid coordinates")]
    InvalidCoordinates,
}

/// Parses a `GeoJSON` feature collection into jurisdiction polygons.
///
/// # Errors
///
/// Returns [`DatasetError`] when the root or coordinates are malformed.
pub fn parse_geojson_dataset(value: &Value) -> Result<JurisdictionDataset, DatasetError> {
    if value.get("type").and_then(Value::as_str) != Some("FeatureCollection") {
        return Err(DatasetError::InvalidFeatureCollection);
    }
    let features = value
        .get("features")
        .and_then(Value::as_array)
        .ok_or(DatasetError::InvalidFeatureCollection)?;
    let mut grouped: BTreeMap<String, Vec<Vec<Vec<LngLat>>>> = BTreeMap::new();

    for feature in features {
        let Some(code) = extract_code(feature.get("properties")) else {
            continue;
        };
        let Some(geometry) = feature.get("geometry").filter(|value| !value.is_null()) else {
            continue;
        };
        let Some(kind) = geometry.get("type").and_then(Value::as_str) else {
            continue;
        };
        let Some(coordinates) = geometry.get("coordinates") else {
            continue;
        };
        let polygons = match kind {
            "Polygon" => vec![parse_polygon(coordinates)?],
            "MultiPolygon" => coordinates
                .as_array()
                .ok_or(DatasetError::InvalidCoordinates)?
                .iter()
                .map(parse_polygon)
                .collect::<Result<Vec<_>, _>>()?,
            _ => continue,
        };
        grouped.entry(code).or_default().extend(polygons);
    }

    Ok(grouped
        .into_iter()
        .map(|(code, polygons)| Jurisdiction {
            bbox: polygon_bbox(&polygons),
            code,
            polygons,
        })
        .collect())
}

fn parse_polygon(value: &Value) -> Result<Vec<Vec<LngLat>>, DatasetError> {
    value
        .as_array()
        .ok_or(DatasetError::InvalidCoordinates)?
        .iter()
        .map(|ring| {
            ring.as_array()
                .ok_or(DatasetError::InvalidCoordinates)?
                .iter()
                .map(|coordinate| {
                    let coordinate = coordinate
                        .as_array()
                        .ok_or(DatasetError::InvalidCoordinates)?;
                    let lng = coordinate
                        .first()
                        .and_then(Value::as_f64)
                        .ok_or(DatasetError::InvalidCoordinates)?;
                    let lat = coordinate
                        .get(1)
                        .and_then(Value::as_f64)
                        .ok_or(DatasetError::InvalidCoordinates)?;
                    Ok(LngLat(lng, lat))
                })
                .collect()
        })
        .collect()
}

fn extract_code(properties: Option<&Value>) -> Option<String> {
    let properties = properties?.as_object()?;
    for key in CODE_KEYS {
        let Some(raw) = properties.get(*key).and_then(Value::as_str) else {
            continue;
        };
        let value = raw.trim().to_uppercase();
        if value.len() == 2
            && value
                .chars()
                .all(|character| character.is_ascii_uppercase())
        {
            return Some(value);
        }
        if let Some(code) = value
            .strip_prefix("US-")
            .or_else(|| value.strip_prefix("CA-"))
            .filter(|code| code.len() == 2)
        {
            return Some(code.to_owned());
        }
    }
    for key in NAME_KEYS {
        let Some(raw) = properties.get(*key).and_then(Value::as_str) else {
            continue;
        };
        if let Some(code) = name_to_code(&raw.trim().to_lowercase()) {
            return Some(code.to_owned());
        }
    }
    None
}

fn name_to_code(name: &str) -> Option<&'static str> {
    Some(match name {
        "alabama" => "AL",
        "alaska" => "AK",
        "arizona" => "AZ",
        "arkansas" => "AR",
        "california" => "CA",
        "colorado" => "CO",
        "connecticut" => "CT",
        "delaware" => "DE",
        "district of columbia" => "DC",
        "florida" => "FL",
        "georgia" => "GA",
        "hawaii" => "HI",
        "idaho" => "ID",
        "illinois" => "IL",
        "indiana" => "IN",
        "iowa" => "IA",
        "kansas" => "KS",
        "kentucky" => "KY",
        "louisiana" => "LA",
        "maine" => "ME",
        "maryland" => "MD",
        "massachusetts" => "MA",
        "michigan" => "MI",
        "minnesota" => "MN",
        "mississippi" => "MS",
        "missouri" => "MO",
        "montana" => "MT",
        "nebraska" => "NE",
        "nevada" => "NV",
        "new hampshire" => "NH",
        "new jersey" => "NJ",
        "new mexico" => "NM",
        "new york" => "NY",
        "north carolina" => "NC",
        "north dakota" => "ND",
        "ohio" => "OH",
        "oklahoma" => "OK",
        "oregon" => "OR",
        "pennsylvania" => "PA",
        "rhode island" => "RI",
        "south carolina" => "SC",
        "south dakota" => "SD",
        "tennessee" => "TN",
        "texas" => "TX",
        "utah" => "UT",
        "vermont" => "VT",
        "virginia" => "VA",
        "washington" => "WA",
        "west virginia" => "WV",
        "wisconsin" => "WI",
        "wyoming" => "WY",
        "alberta" => "AB",
        "british columbia" => "BC",
        "manitoba" => "MB",
        "new brunswick" => "NB",
        "newfoundland and labrador" => "NL",
        "nova scotia" => "NS",
        "northwest territories" => "NT",
        "nunavut" => "NU",
        "ontario" => "ON",
        "prince edward island" => "PE",
        "quebec" | "québec" => "QC",
        "saskatchewan" => "SK",
        "yukon" => "YT",
        _ => return None,
    })
}
