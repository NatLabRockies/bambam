use crate::model::{
    constraint::gbfs_constraint::GbfsConstraintConfig,
    gbfs::{GbfsZoneRecord, ZoneRules},
};

use bambam_core::{model::state::fieldname, util::geo_utils};
use chrono::{DateTime, TimeDelta, Utc};
use geo::Geometry;
use geozero::{ToGeo, wkt::Wkt};
use itertools::Itertools;
use kdam::BarBuilder;
use routee_compass_core::{
    model::{
        network::Vertex,
        state::{StateModel, StateVariable},
    },
    util::{fs::read_utils, geo::PolygonalRTree},
};

pub struct GbfsLookupModel {
    rtree: PolygonalRTree<f32, GbfsZoneRecord>,
    global: Option<GbfsZoneRecord>,
}

impl TryFrom<&GbfsConstraintConfig> for GbfsLookupModel {
    type Error = String;

    fn try_from(config: &GbfsConstraintConfig) -> Result<Self, Self::Error> {
        let (global, zonal_records) = read_records(&config.zones_input_file)?;
        let geometries = read_geometries(&config.geometries_input_file)?;

        // check sizes match
        if zonal_records.len() != geometries.len() {
            let msg = format!(
                "file {} has {} records (besides global record), but file {} has {} geometries; sizes must match.",
                config.zones_input_file,
                zonal_records.len(),
                config.geometries_input_file,
                geometries.len()
            );
            return Err(msg);
        }

        let rows = geometries
            .into_iter()
            .zip(zonal_records.into_iter())
            .collect_vec();
        let rtree = PolygonalRTree::new(rows)
            .map_err(|e| format!("failure building spatial index: {e}"))?;
        Ok(Self { rtree, global })
    }
}

impl GbfsLookupModel {
    /// uses the destination vertex of the current edge traversal to find any intersecting
    /// zones. filters out zonal rules that do not match our trip datetime. combines all
    /// zone record rules in order to produce a single value result.
    pub fn get_zone_rules(
        &self,
        vertex: &Vertex,
        state: &[StateVariable],
        state_model: &StateModel,
        start_time: DateTime<Utc>,
    ) -> Result<Option<ZoneRules>, String> {
        let query = geo::Geometry::Point(geo::Point(vertex.coordinate.0));
        let time = get_trip_datetime(state, state_model, start_time)?;
        let intersection = self.spatiotemporal_intersection(&query, time)?;
        let rules = ZoneRules::from_records(self.global.as_ref(), intersection);
        Ok(rules)
    }

    /// inner function that finds all zone matches that intersect a spatiotemporal query.
    fn spatiotemporal_intersection<'a>(
        &'a self,
        spatial_query: &'a Geometry<f32>,
        time: DateTime<Utc>,
    ) -> Result<Box<dyn Iterator<Item = &'a GbfsZoneRecord> + 'a>, String> {
        let matches = self
            .rtree
            .intersection(&spatial_query)
            .map_err(|e| format!("failure running GBFS zone lookup: {e}"))?
            .filter_map(move |z| {
                if within_time_range(time, &z.data) {
                    Some(&z.data)
                } else {
                    None
                }
            });

        Ok(Box::new(matches))
    }
}

/// reads the records and builds a ZoneGraph from them. holds aside the global record
fn read_records(
    zone_record_input_file: &str,
) -> Result<(Option<GbfsZoneRecord>, Vec<GbfsZoneRecord>), String> {
    let bb = BarBuilder::default().desc("reading zone records");
    let zone_records: Box<[GbfsZoneRecord]> =
        read_utils::from_csv(&zone_record_input_file, true, Some(bb), None)
            .map_err(|e| format!("failure reading zone records: {e}"))?;
    let (mut globals, zonals): (Vec<GbfsZoneRecord>, Vec<GbfsZoneRecord>) = zone_records
        .into_iter()
        .partition(|r| r.feature_index.is_none());
    let globals_n = globals.len();
    if globals_n > 1 {
        return Err(format!(
            "expected exactly one global record in file '{zone_record_input_file}', found {globals_n}"
        ));
    }
    let global = globals.pop();
    Ok((global, zonals))
}

/// reads zonal geometries and ZoneIds from a CSV geometry collection.
fn read_geometries(geometry_input_file: &str) -> Result<Vec<Geometry<f32>>, String> {
    let bb = BarBuilder::default().desc("reading zone geometries");
    let record_strings: Box<[String]> =
        read_utils::from_csv(&geometry_input_file, true, Some(bb), None)
            .map_err(|e| format!("failure reading zone geometries: {e}"))?;
    let rtree_data = record_strings
        .iter()
        .enumerate()
        .map(|(idx, geom_str)| {
            let geometry = Wkt(geom_str)
                .to_geo()
                .map_err(|e| format!("failure reading WKT geometry {idx}: {e}"))?;
            let geom_f32 = geo_utils::try_convert_f32(&geometry).map_err(|e| {
                format!(
                    "failure converting geometry to 32-bit FP representation for index {idx}: {e}",
                )
            })?;
            Ok(geom_f32)
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(rtree_data)
}

/// composes a starting datetime value with the trip time on the provided state producing the
/// current
fn get_trip_datetime(
    state: &[StateVariable],
    state_model: &StateModel,
    start_time: DateTime<Utc>,
) -> Result<DateTime<Utc>, String> {
    let time = state_model
        .get_time(state, fieldname::TRIP_TIME)
        .map_err(|e| format!("failure reading {} from state: {e}", fieldname::TRIP_TIME))?;
    let nanos = time.get::<uom::si::time::nanosecond>() as i64;
    let timedelta = TimeDelta::nanoseconds(nanos);
    start_time.checked_add_signed(timedelta).ok_or_else(|| {
        format!(
            "adding {nanos} ns to {} is out-of-range",
            start_time.to_rfc3339()
        )
    })
}

/// times are documented with "If the geofencing zone is always active, this can be omitted."
/// in this way, we optimistically choose `true` when only one of [start,end] are present.
fn within_time_range(time: DateTime<Utc>, record: &GbfsZoneRecord) -> bool {
    match (record.start, record.end) {
        (Some(s), Some(e)) => s <= time && time < e,
        _ => true,
    }
}
