//! Phase 1 raw structures and GeoJSON parser for GTFS-Flex tables:
//! `booking_rules.txt`, `location_groups.txt`, `location_group_stops.txt`, and `locations.geojson`.

use crate::enums::BookingType;
use crate::ids::{BookingRuleId, LocationGroupId, LocationId, ServiceId, ZoneId};
use crate::primitives::{deserialize_optional_time, serialize_optional_time, Coord, GtfsTime};
use crate::raw::RawReadError;

/// Raw row from `booking_rules.txt` (GTFS-Flex).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawBookingRule {
    /// Unique identifier for the booking rule.
    #[serde(rename = "booking_rule_id")]
    pub id: BookingRuleId,
    /// Type of booking (`0` = real-time, `1` = same-day advance notice, `2` = prior days).
    pub booking_type: BookingType,
    /// Minimum minutes before travel to make the request (required when `booking_type = 1`).
    pub prior_notice_duration_min: Option<i32>,
    /// Maximum minutes before travel to make the request (optional when `booking_type = 1`).
    pub prior_notice_duration_max: Option<i32>,
    /// Last day before travel to make the booking request (required when `booking_type = 2`).
    pub prior_notice_last_day: Option<i32>,
    /// Last time on the last day before travel to make the booking request.
    #[serde(
        deserialize_with = "deserialize_optional_time",
        serialize_with = "serialize_optional_time",
        default
    )]
    pub prior_notice_last_time: Option<GtfsTime>,
    /// Earliest day before travel to make the booking request.
    pub prior_notice_start_day: Option<i32>,
    /// Earliest time on the earliest day before travel to make the booking request.
    #[serde(
        deserialize_with = "deserialize_optional_time",
        serialize_with = "serialize_optional_time",
        default
    )]
    pub prior_notice_start_time: Option<GtfsTime>,
    /// Service ID from `calendar.txt` used to count prior notice days.
    pub prior_notice_service_id: Option<ServiceId>,
    /// General rider message for booking.
    pub message: Option<String>,
    /// Message specific to pickups.
    pub pickup_message: Option<String>,
    /// Message specific to drop-offs.
    pub drop_off_message: Option<String>,
    /// Phone number to call to make a booking request.
    pub phone_number: Option<String>,
    /// URL providing information about the booking rule.
    pub info_url: Option<String>,
    /// URL for online booking.
    pub booking_url: Option<String>,
}

/// Raw row from `location_groups.txt` (GTFS-Flex).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawLocationGroup {
    /// Unique identifier for the location group.
    pub location_group_id: LocationGroupId,
    /// Human-readable name of the location group.
    pub location_group_name: Option<String>,
}

/// Raw row from `location_group_stops.txt` (GTFS-Flex).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawLocationGroupStop {
    /// Identifier of the location group (`location_groups.txt`).
    pub location_group_id: LocationGroupId,
    /// Identifier of a member stop (`stops.txt`) or GeoJSON location (`locations.geojson`).
    pub stop_id: String,
}

/// Validated polygon or multipolygon rings parsed from `locations.geojson` (GTFS-Flex).
///
/// Unlike an untagged enum wrapping `geojson::Geometry` in both variants, this enum
/// holds the strongly-typed coordinate rings directly and serializes/deserializes cleanly.
#[derive(
    Clone, Debug, PartialEq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub enum RawFlexGeometry {
    /// A single polygon (exterior ring followed by zero or more interior hole rings).
    Polygon(Vec<Vec<Coord>>),
    /// A collection of polygons.
    MultiPolygon(Vec<Vec<Vec<Coord>>>),
}

impl RawFlexGeometry {
    /// Returns `true` if `coord` lies inside or on the boundary of this Polygon or MultiPolygon
    /// (accounting for interior hole rings via odd-even ray casting).
    pub fn contains_coord(&self, coord: Coord) -> bool {
        match self {
            RawFlexGeometry::Polygon(rings) => polygon_rings_contain_coord(rings, coord),
            RawFlexGeometry::MultiPolygon(polys) => polys
                .iter()
                .any(|rings| polygon_rings_contain_coord(rings, coord)),
        }
    }
}

fn polygon_rings_contain_coord(rings: &[Vec<Coord>], coord: Coord) -> bool {
    let Some((exterior, holes)) = rings.split_first() else {
        return false;
    };
    if !ring_contains_coord(exterior, coord) {
        return false;
    }
    for hole in holes {
        if ring_contains_coord_strict_interior(hole, coord) {
            return false;
        }
    }
    true
}

fn point_on_segment(p: Coord, a: Coord, b: Coord) -> bool {
    const EPS: f64 = 1e-11;
    let min_x = a.lon.min(b.lon) - EPS;
    let max_x = a.lon.max(b.lon) + EPS;
    let min_y = a.lat.min(b.lat) - EPS;
    let max_y = a.lat.max(b.lat) + EPS;
    if p.lon < min_x || p.lon > max_x || p.lat < min_y || p.lat > max_y {
        return false;
    }
    let cross = (b.lon - a.lon) * (p.lat - a.lat) - (b.lat - a.lat) * (p.lon - a.lon);
    let seg_len = ((b.lon - a.lon).hypot(b.lat - a.lat)).max(1.0);
    (cross.abs() / seg_len) <= EPS
}

fn ring_ray_cast(ring: &[Coord], coord: Coord) -> (bool, bool) {
    let n = ring.len();
    if n < 3 {
        return (false, false);
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let pi = ring[i];
        let pj = ring[j];
        if point_on_segment(coord, pj, pi) {
            return (true, true);
        }
        if (pi.lat > coord.lat) != (pj.lat > coord.lat) {
            let x_intersect =
                (pj.lon - pi.lon) * (coord.lat - pi.lat) / (pj.lat - pi.lat) + pi.lon;
            if coord.lon < x_intersect {
                inside = !inside;
            }
        }
        j = i;
    }
    (inside, false)
}

/// Returns `true` if `coord` is inside or on the boundary of `ring`.
fn ring_contains_coord(ring: &[Coord], coord: Coord) -> bool {
    let (inside, on_boundary) = ring_ray_cast(ring, coord);
    inside || on_boundary
}

/// Returns `true` if `coord` is strictly inside `ring` (used for interior holes so points on a hole boundary remain in the polygon).
fn ring_contains_coord_strict_interior(ring: &[Coord], coord: Coord) -> bool {
    let (inside, on_boundary) = ring_ray_cast(ring, coord);
    inside && !on_boundary
}

/// Raw feature from `locations.geojson` (GTFS-Flex).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RawFlexLocation {
    /// Unique identifier of the flexible zone (shares namespace with `stops.txt: stop_id`).
    pub id: LocationId,
    /// Optional human-readable name of the zone.
    pub stop_name: Option<String>,
    /// Optional description of the zone.
    pub stop_desc: Option<String>,
    /// Optional fare zone identifier.
    pub zone_id: Option<ZoneId>,
    /// Optional URL about the zone.
    pub stop_url: Option<String>,
    /// Polygon or MultiPolygon geometry of the zone.
    pub geometry: RawFlexLocationGeometry,
}

/// Type alias for `RawFlexGeometry`.
pub type RawFlexLocationGeometry = RawFlexGeometry;

#[derive(Deserialize)]
struct LocationFeatureProperties {
    #[serde(default)]
    stop_name: Option<String>,
    #[serde(default)]
    stop_desc: Option<String>,
    #[serde(default)]
    zone_id: Option<ZoneId>,
    #[serde(default)]
    stop_url: Option<String>,
}

fn convert_ring(ring: &[geojson::Position]) -> Result<Vec<Coord>, RawReadError> {
    let mut coords = Vec::with_capacity(ring.len());
    for pos in ring {
        if pos.len() < 2 {
            return Err(RawReadError::GeoJson {
                file_name: "locations.geojson".to_owned(),
                message: "coordinate position must have at least [lon, lat]".to_owned(),
            });
        }
        coords.push(Coord::new(pos[0], pos[1]));
    }
    Ok(coords)
}

/// Parses `locations.geojson` content into a vector of [`RawFlexLocation`].
pub fn parse_locations_geojson(contents: &str) -> Result<Vec<RawFlexLocation>, RawReadError> {
    let fc: geojson::FeatureCollection =
        serde_json::from_str(contents).map_err(|e| RawReadError::GeoJson {
            file_name: "locations.geojson".to_owned(),
            message: format!("GeoJSON parse error: {e}"),
        })?;

    let mut locations = Vec::with_capacity(fc.features.len());
    for (idx, feature) in fc.features.into_iter().enumerate() {
        let id_str = feature
            .id
            .as_ref()
            .map(|id| match id {
                geojson::feature::Id::String(s) => s.clone(),
                geojson::feature::Id::Number(n) => n.to_string(),
            })
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| RawReadError::GeoJson {
                file_name: "locations.geojson".to_owned(),
                message: format!("feature at index {idx} is missing a non-empty 'id'"),
            })?;

        let props: LocationFeatureProperties = match feature.properties {
            Some(map) => serde_json::from_value(serde_json::Value::Object(map)).map_err(|e| {
                RawReadError::GeoJson {
                    file_name: "locations.geojson".to_owned(),
                    message: format!("failed to parse properties for location '{id_str}': {e}"),
                }
            })?,
            None => LocationFeatureProperties {
                stop_name: None,
                stop_desc: None,
                zone_id: None,
                stop_url: None,
            },
        };

        let geom = feature.geometry.ok_or_else(|| RawReadError::GeoJson {
            file_name: "locations.geojson".to_owned(),
            message: format!("location '{id_str}' is missing geometry"),
        })?;

        let geometry = match &geom.value {
            geojson::Value::Polygon(rings) => {
                let mut parsed_rings = Vec::with_capacity(rings.len());
                for ring in rings {
                    parsed_rings.push(convert_ring(ring)?);
                }
                RawFlexGeometry::Polygon(parsed_rings)
            }
            geojson::Value::MultiPolygon(polys) => {
                let mut parsed_polys = Vec::with_capacity(polys.len());
                for rings in polys {
                    let mut parsed_rings = Vec::with_capacity(rings.len());
                    for ring in rings {
                        parsed_rings.push(convert_ring(ring)?);
                    }
                    parsed_polys.push(parsed_rings);
                }
                RawFlexGeometry::MultiPolygon(parsed_polys)
            }
            other => {
                return Err(RawReadError::GeoJson {
                    file_name: "locations.geojson".to_owned(),
                    message: format!(
                        "location '{id_str}' has invalid geometry type '{}' (expected Polygon or MultiPolygon)",
                        other.type_name()
                    ),
                });
            }
        };

        locations.push(RawFlexLocation {
            id: LocationId::new(id_str),
            stop_name: props.stop_name,
            stop_desc: props.stop_desc,
            zone_id: props.zone_id,
            stop_url: props.stop_url,
            geometry,
        });
    }

    Ok(locations)
}

// --- Self-describing GtfsTable and TableBundle implementations for GTFS-Flex ---

crate::impl_gtfs_csv_table!(RawBookingRule, "booking_rules.txt", mandatory = false);
crate::impl_gtfs_csv_table!(RawLocationGroup, "location_groups.txt", mandatory = false);
crate::impl_gtfs_csv_table!(
    RawLocationGroupStop,
    "location_group_stops.txt",
    mandatory = false
);

impl crate::raw::GtfsTable for RawFlexLocation {
    const FILE_NAME: &'static str = "locations.geojson";
    const MANDATORY: bool = false;

    fn read_table<R: std::io::Read + std::io::Seek>(
        archive: &mut crate::raw::GtfsArchive<R>,
    ) -> Result<Vec<Self>, RawReadError> {
        let contents = archive.read_file_to_string(Self::FILE_NAME)?;
        parse_locations_geojson(&contents)
    }
}

/// Composable [`TableBundle`](crate::raw::TableBundle) encapsulating all GTFS-Flex extension tables.
///
/// Can be loaded standalone via `archive.load::<FlexTables>()?` or as part of [`RawFeed`](crate::raw::RawFeed).
#[derive(Clone, Debug, Default)]
pub struct FlexTables {
    /// Rows from `booking_rules.txt` (optional).
    pub booking_rules: Option<Vec<RawBookingRule>>,
    /// Features from `locations.geojson` (optional).
    pub locations: Option<Vec<RawFlexLocation>>,
    /// Rows from `location_groups.txt` (optional).
    pub location_groups: Option<Vec<RawLocationGroup>>,
    /// Rows from `location_group_stops.txt` (optional).
    pub location_group_stops: Option<Vec<RawLocationGroupStop>>,
}

impl crate::raw::TableBundle for FlexTables {
    fn from_archive<R: std::io::Read + std::io::Seek>(
        archive: &mut crate::raw::GtfsArchive<R>,
    ) -> Result<Self, RawReadError> {
        Ok(Self {
            booking_rules: archive.load()?,
            locations: archive.load()?,
            location_groups: archive.load()?,
            location_group_stops: archive.load()?,
        })
    }
}
