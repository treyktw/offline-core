use super::{BoundingBox, LngLat};

const EARTH_RADIUS_MILES: f64 = 3_958.761_3;

/// Computes great-circle distance in miles.
pub fn haversine_miles(a_lat: f64, a_lng: f64, b_lat: f64, b_lng: f64) -> f64 {
    let latitude_delta = (b_lat - a_lat).to_radians();
    let longitude_delta = (b_lng - a_lng).to_radians();
    let latitude_one = a_lat.to_radians();
    let latitude_two = b_lat.to_radians();
    let haversine = (latitude_delta / 2.0).sin().powi(2)
        + latitude_one.cos() * latitude_two.cos() * (longitude_delta / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_MILES * haversine.sqrt().min(1.0).asin()
}

/// Tests whether a point is inside a linear ring using ray casting.
pub fn point_in_ring(lng: f64, lat: f64, ring: &[LngLat]) -> bool {
    if ring.is_empty() {
        return false;
    }
    let mut inside = false;
    let mut previous = ring.len() - 1;
    for current in 0..ring.len() {
        let LngLat(current_lng, current_lat) = ring[current];
        let LngLat(previous_lng, previous_lat) = ring[previous];
        let intersects = (current_lat > lat) != (previous_lat > lat)
            && lng
                < (previous_lng - current_lng) * (lat - current_lat) / (previous_lat - current_lat)
                    + current_lng;
        if intersects {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

/// Tests whether a point is inside a polygon and outside all holes.
pub fn point_in_polygon(lng: f64, lat: f64, rings: &[Vec<LngLat>]) -> bool {
    let Some(outer) = rings.first() else {
        return false;
    };
    point_in_ring(lng, lat, outer) && !rings[1..].iter().any(|hole| point_in_ring(lng, lat, hole))
}

/// Tests whether a point lies inside a bounding box.
pub const fn bbox_contains(bbox: BoundingBox, lng: f64, lat: f64) -> bool {
    lng >= bbox.0 && lng <= bbox.2 && lat >= bbox.1 && lat <= bbox.3
}

/// Calculates a bounding box for a multi-polygon.
pub fn polygon_bbox(polygons: &[Vec<Vec<LngLat>>]) -> BoundingBox {
    let mut minimum_lng = f64::INFINITY;
    let mut minimum_lat = f64::INFINITY;
    let mut maximum_lng = f64::NEG_INFINITY;
    let mut maximum_lat = f64::NEG_INFINITY;
    for coordinate in polygons.iter().flatten().flatten() {
        minimum_lng = minimum_lng.min(coordinate.0);
        minimum_lat = minimum_lat.min(coordinate.1);
        maximum_lng = maximum_lng.max(coordinate.0);
        maximum_lat = maximum_lat.max(coordinate.1);
    }
    BoundingBox(minimum_lng, minimum_lat, maximum_lng, maximum_lat)
}
