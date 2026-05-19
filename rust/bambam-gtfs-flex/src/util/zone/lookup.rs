use std::path::PathBuf;

use crate::util::zone::{
    ZonalRelationRecord, ZoneError, ZoneGeometry, ZoneGraph, ZoneId, ZoneLookupConfig,
};

use bambam_core::{model::state::CategoricalMapping, util::geo_utils::try_convert_f32};
use chrono::NaiveDateTime;
use geozero::{wkt::Wkt, ToGeo};
use kdam::BarBuilder;
use routee_compass_core::{
    model::{constraint::ConstraintModelError, network::Vertex, traversal::TraversalModelError},
    util::{fs::read_utils, geo::PolygonalRTree},
};

/// top-level API for working with GTFS-Flex zonal data.
pub struct ZoneLookup {
    /// mapping from ZoneId to an integer value in [0, i64::MAX).
    /// the value -1 is reserved to model EMPTY (unassigned) in the state vector.
    pub mapping: CategoricalMapping<ZoneId, i64>,
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
        let mapping = read_zone_ids(&config.zone_ids_input_file)?;
        let graph = read_records(&config.zone_record_input_file)?;
        let rtree = read_geometries(&config.zone_geometry_input_file)?;
        Ok(ZoneLookup {
            mapping,
            graph,
            rtree,
        })
    }
}

/// reads the zone ids from an enumerated file into a Categorical Mapping from i64 to ZoneId.
fn read_zone_ids(zone_ids_input_file: &str) -> Result<CategoricalMapping<ZoneId, i64>, ZoneError> {
    let bb = BarBuilder::default().desc("reading zone ids");
    let zone_ids: Box<[ZoneId]> =
        read_utils::read_raw_file(&zone_ids_input_file, parse_zone_id, Some(bb), None).map_err(
            |e| {
                let msg = format!("failure reading zone records: {e}");
                ZoneError::Build(msg)
            },
        )?;
    let mapping = CategoricalMapping::new(&zone_ids).map_err(|e| {
        let msg = format!("failure reading zone ids from '{zone_ids_input_file}': {e}");
        ZoneError::Build(msg)
    })?;
    Ok(mapping)
}

/// reads the records and builds a ZoneGraph from them.
fn read_records(zone_record_input_file: &str) -> Result<ZoneGraph, ZoneError> {
    let bb = BarBuilder::default().desc("reading zone records");
    let zone_records: Box<[ZonalRelationRecord]> =
        read_utils::from_csv(&zone_record_input_file, true, Some(bb), None).map_err(|e| {
            let msg = format!("failure reading zone records: {e}");
            ZoneError::Build(msg)
        })?;
    let graph = ZoneGraph::try_from(&zone_records[..])?;
    Ok(graph)
}

/// reads zonal geometries and ZoneIds from a CSV geometry collection.
fn read_geometries(geometry_input_file: &str) -> Result<PolygonalRTree<f32, ZoneId>, ZoneError> {
    let bb = BarBuilder::default().desc("reading zone geometries");
    let zone_records: Box<[ZoneGeometry]> =
        read_utils::from_csv(&geometry_input_file, true, Some(bb), None).map_err(|e| {
            let msg = format!("failure reading zone geometries: {e}");
            ZoneError::Build(msg)
        })?;
    let rtree_data = zone_records
        .iter()
        .enumerate()
        .map(|(idx, zg)| {
            let geometry = Wkt(&zg.geometry)
                .to_geo()
                .map_err(|e| ZoneError::Deserialize {
                    col: "geometry".to_string(),
                    path: PathBuf::from(geometry_input_file),
                    message: format!(
                        "failure reading geometry for ZoneId {} at row {idx}: {e}",
                        zg.zone_id
                    ),
                })?;
            let geom_f32 = try_convert_f32(&geometry).map_err(|e| ZoneError::Deserialize {
                col: "geometry".to_string(),
                path: PathBuf::from(geometry_input_file),
                message: format!(
                    "failure converting geometry to 32-bit FP representation for ZoneId {}: {e}",
                    zg.zone_id
                ),
            })?;
            Ok((geom_f32, zg.zone_id.clone()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let rtree = PolygonalRTree::new(rtree_data).map_err(|e| {
        let msg = format!("failure building spatial index for GTFS Flex zones: {e}");
        ZoneError::Build(msg)
    })?;
    Ok(rtree)
}

pub fn parse_zone_id(idx: usize, row: String) -> Result<ZoneId, std::io::Error> {
    ZoneId::try_from(row.as_str()).map_err(|e| {
        let msg = format!("failure decoding ZoneId at row {idx}. error: {e}");
        std::io::Error::new(std::io::ErrorKind::InvalidData, msg)
    })
}
