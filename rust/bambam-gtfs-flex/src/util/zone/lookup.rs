use std::path::Path;

use crate::util::zone::{ZoneError, ZoneGraph, ZoneId, ZoneLookupConfig, ZoneRecord};

use bambam_core::util::geo_utils::try_convert_f32;
use chrono::NaiveDateTime;
use geo::Geometry;
use geozero::{geojson::GeoJsonString, ToGeo};
use kdam::BarBuilder;
use routee_compass_core::{
    model::{constraint::ConstraintModelError, network::Vertex, traversal::TraversalModelError},
    util::{fs::read_utils, geo::PolygonalRTree},
};

/// top-level API for working with GTFS-Flex zonal data.
pub struct ZoneLookup {
    /// graph of relations between zones.
    pub graph: ZoneGraph,
    /// spatial lookup from the road network into the zone graph.
    pub rtree: PolygonalRTree<f32, ZoneId>,
}

impl ZoneLookup {
    /// look up the [ZoneId] that intersects with some [Vertex]. assumes that
    /// the first overlapping [ZoneId] is correct.
    pub fn get_zone_for_vertex(&self, vertex: &Vertex) -> Result<Option<ZoneId>, String> {
        let point = geo::Point(vertex.coordinate.0);
        let query = geo::Geometry::Point(point);
        let result = self
            .rtree
            .intersection(&query)?
            .next()
            .map(|n| &n.data)
            .cloned();
        Ok(result)
    }

    /// is it valid to begin a trip in this zone at this time?
    pub fn valid_departure(
        &self,
        src_zone_id: &ZoneId,
        current_time: &NaiveDateTime,
    ) -> Result<bool, ConstraintModelError> {
        self.graph
            .valid_departure(src_zone_id, current_time)
            .map_err(|e| ConstraintModelError::ConstraintModelError(e.to_string()))
    }

    /// is it valid to end a trip that began at the src zone and reached this dst zone
    /// at this time?
    pub fn valid_destination(
        &self,
        src_zone_id: &ZoneId,
        current_vertex: &Vertex,
        current_time: &NaiveDateTime,
    ) -> Result<bool, TraversalModelError> {
        let point = geo::Geometry::Point(geo::Point(current_vertex.coordinate.0));

        let zone_iter = self.rtree.intersection(&point).map_err(|e| {
            let msg = format!("failure looking up zone geometry from trip location: {e}");
            TraversalModelError::TraversalModelFailure(msg)
        })?;

        // check if any intersecting destination zones are valid for this trip
        for node in zone_iter {
            let is_valid = self
                .graph
                .valid_zonal_trip(src_zone_id, &node.data, current_time)
                .map_err(|e| TraversalModelError::TraversalModelFailure(e.to_string()))?;
            if is_valid {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl TryFrom<&ZoneLookupConfig> for ZoneLookup {
    type Error = ZoneError;

    fn try_from(config: &ZoneLookupConfig) -> Result<Self, Self::Error> {
        let graph = read_records(&config.zone_record_input_file)?;
        let rtree = read_geometries(
            &config.zone_geometry_input_file,
            config.zone_id_property.as_ref(),
        )?;
        Ok(ZoneLookup { graph, rtree })
    }
}

fn read_records(zone_record_input_file: &str) -> Result<ZoneGraph, ZoneError> {
    let bb = BarBuilder::default().desc("reading zone records");
    let zone_records: Box<[ZoneRecord]> =
        read_utils::from_csv(&zone_record_input_file, true, Some(bb), None).map_err(|e| {
            let msg = format!("failure reading zone records: {e}");
            ZoneError::Build(msg)
        })?;
    let graph = ZoneGraph::try_from(&zone_records[..])?;
    Ok(graph)
}

/// reads zonal geometries and ZoneIds from a GeoJSON geometry collection.
///
/// if the user provides an "id_property" then the ZoneId will be fished out
/// from the feature's properties at `$.properties.{id_property}`. otherwise,
/// the Feature id will be used.
fn read_geometries(
    geometry_input_file: &str,
    id_property: Option<&String>,
) -> Result<PolygonalRTree<f32, ZoneId>, ZoneError> {
    let geom_path = Path::new(geometry_input_file);
    let geojson_str = std::fs::read_to_string(geom_path).map_err(|e| ZoneError::Read {
        path: geom_path.to_path_buf(),
        source: e,
    })?;

    let geojson_value: serde_json::Value =
        serde_json::from_str(&geojson_str).map_err(|e| ZoneError::Parse {
            message: e.to_string(),
            path: geom_path.to_path_buf(),
        })?;
    let features = geojson_value["features"]
        .as_array()
        .ok_or_else(|| ZoneError::Parse {
            message: "zonal geometry input GeoJSON does not have 'features' key as expected"
                .to_string(),
            path: geom_path.to_path_buf(),
        })?;
    let n_features: usize = features.len();
    let mut zone_geometries = Vec::with_capacity(n_features);
    for (idx, feature) in features.iter().enumerate() {
        let zone_id = match id_property {
            Some(property) => get_zone_id_from_property(feature, property, idx, geom_path)?,
            None => get_zone_id_from_feature_id(feature, idx, geom_path)?,
        };
        let geometry_json =
            serde_json::to_string(&feature["geometry"]).map_err(|e| ZoneError::Deserialize {
                col: "geometry".to_string(),
                path: geom_path.to_path_buf(),
                message: format!("failure serializing geometry for GeoJSON Feature [{idx}] with id {zone_id}: {e}"),
            })?;
        let geometry: Geometry =
            GeoJsonString(geometry_json)
                .to_geo()
                .map_err(|e| ZoneError::Deserialize {
                    col: "geometry".to_string(),
                    path: geom_path.to_path_buf(),
                    message: format!(
                        "failure decoding GeoJson geometry to geo-types for ZoneId {zone_id}: {e}"
                    ),
                })?;
        let geom_f32 = try_convert_f32(&geometry).map_err(|e| ZoneError::Deserialize {
            col: "geometry".to_string(),
            path: geom_path.to_path_buf(),
            message: format!(
                "failure converting geometry to 32-bit FP representation for ZoneId {zone_id}: {e}"
            ),
        })?;

        zone_geometries.push((geom_f32, zone_id));
    }

    let rtree = PolygonalRTree::new(zone_geometries).map_err(|e| {
        let msg = format!("failure building spatial index for GTFS Flex zones: {e}");
        ZoneError::Build(msg)
    })?;

    Ok(rtree)
}

fn get_zone_id_from_feature_id(
    feature: &serde_json::Value,
    idx: usize,
    geom_path: &Path,
) -> Result<ZoneId, ZoneError> {
    let zone_id_str = feature
        .get("id")
        .ok_or_else(|| ZoneError::Deserialize {
            col: "id".to_string(),
            path: geom_path.to_path_buf(),
            message: format!("GeoJSON Feature [{idx}] missing 'id' field"),
        })?
        .as_str()
        .ok_or_else(|| ZoneError::Deserialize {
            col: "id".to_string(),
            path: geom_path.to_path_buf(),
            message: format!("cannot read GeoJSON Feature [{idx}] id as a string"),
        })?;
    Ok(ZoneId(zone_id_str.to_string()))
}

fn get_zone_id_from_property(
    feature: &serde_json::Value,
    zone_id_property: &str,
    idx: usize,
    geom_path: &Path,
) -> Result<ZoneId, ZoneError> {
    let zone_id_str = feature
        .get("properties")
        .ok_or_else(|| ZoneError::Deserialize {
            col: "properties".to_string(),
            path: geom_path.to_path_buf(),
            message: String::from("GeoJSON Feature [{idx}] missing 'properties' field"),
        })?
        .get(zone_id_property)
        .ok_or_else(|| ZoneError::Deserialize {
            col: zone_id_property.to_string(),
            path: geom_path.to_path_buf(),
            message: format!("GeoJSON Feature [{idx}] missing property {zone_id_property}"),
        })?
        .as_str()
        .ok_or_else(|| ZoneError::Deserialize {
            col: zone_id_property.to_string(),
            path: geom_path.to_path_buf(),
            message: format!(
                "cannot read GeoJSON Feature [{idx}] property {zone_id_property} as a string"
            ),
        })?;
    Ok(ZoneId(zone_id_str.to_string()))
}
