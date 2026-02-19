use std::path::Path;

use crate::util::zone::{ZoneError, ZoneGraph, ZoneId, ZoneLookupConfig, ZoneRecord};

use bambam_core::util::geo_utils::try_convert_f32;
use chrono::NaiveDateTime;
use geo::Geometry;
use kdam::BarBuilder;
use routee_compass_core::{
    model::{frontier::FrontierModelError, network::Vertex, traversal::TraversalModelError},
    util::{fs::read_utils, geo::PolygonalRTree},
};

pub struct ZoneLookup {
    pub graph: ZoneGraph,
    pub rtree: PolygonalRTree<f32, ZoneId>,
}

impl ZoneLookup {
    /// is it valid to begin a trip in this zone at this time?
    pub fn valid_departure(
        &self,
        src_zone_id: &ZoneId,
        current_time: &NaiveDateTime,
    ) -> Result<bool, FrontierModelError> {
        self.graph
            .valid_departure(src_zone_id, current_time)
            .map_err(|e| FrontierModelError::FrontierModelError(e.to_string()))
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
        let rtree = read_geometries(&config.zone_geometry_input_file, &config.zone_id_column)?;
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

/// reads zonal geometries and ZoneIds from a GeoJSON geometry collection
fn read_geometries(
    geometry_input_file: &str,
    zone_id_col: &str,
) -> Result<PolygonalRTree<f32, ZoneId>, ZoneError> {
    let geom_path = Path::new(geometry_input_file);
    let geojson_str = std::fs::read_to_string(geom_path).map_err(|e| ZoneError::Read {
        path: geom_path.to_path_buf(),
        source: e,
    })?;
    let geojson_value = geojson_str
        .parse::<geojson::GeoJson>()
        .map_err(|e| ZoneError::Parse {
            message: e.to_string(),
            path: geom_path.to_path_buf(),
        })?;

    let zone_geometries = match geojson_value {
        geojson::GeoJson::FeatureCollection(feature_collection) => {
            let n_features = feature_collection.features.len();
            let mut tree_nodes = Vec::with_capacity(n_features);
            for (n, feature) in feature_collection.features.iter().enumerate() {
                let zone_id_str = feature
                    .property(zone_id_col)
                    .ok_or_else(|| ZoneError::Deserialize {
                        col: zone_id_col.to_string(),
                        path: geom_path.to_path_buf(),
                        message: "column missing".to_string(),
                    })?
                    .as_str()
                    .ok_or_else(|| ZoneError::Deserialize {
                        col: zone_id_col.to_string(),
                        path: geom_path.to_path_buf(),
                        message: "cannot read as string".to_string(),
                    })?;
                let zone_id = ZoneId(zone_id_str.to_string());

                let geom_json = feature
                    .geometry
                    .clone()
                    .ok_or_else(|| ZoneError::Deserialize {
                        col: "geometry".to_string(),
                        path: geom_path.to_path_buf(),
                        message: format!("no geometry in feature {n}"),
                    })?;
                let geometry: Geometry = geom_json.try_into().map_err(|e| {
                    ZoneError::Deserialize { col: "geometry".to_string(), path: geom_path.to_path_buf(), message: format!("failure decoding GeoJson geometry to geo-types for ZoneId {zone_id}: {e}") }
                })?;
                let geom_f32 = try_convert_f32(&geometry).map_err(|e| {
                    ZoneError::Deserialize { col: "geometry".to_string(), path: geom_path.to_path_buf(), message: format!("failure converting geometry to 32-bit FP representation for ZoneId {zone_id}: {e}") }
                })?;
                tree_nodes.push((geom_f32, zone_id));
            }
            Ok(tree_nodes)
        }
        _ => Err(ZoneError::Parse {
            path: geom_path.to_path_buf(),
            message: "geojson in file must be a FeatureCollection".to_string(),
        }),
    }?;

    let rtree = PolygonalRTree::new(zone_geometries).map_err(|e| {
        let msg = format!("failure building spatial index for GTFS Flex zones: {e}");
        ZoneError::Build(msg)
    })?;

    Ok(rtree)
}
