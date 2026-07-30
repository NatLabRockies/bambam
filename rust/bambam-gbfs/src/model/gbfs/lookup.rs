use crate::model::gbfs::{GbfsZoneRecord, ZoneState};

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
}

impl GbfsLookupModel {
    pub fn new(zones_input_file: &str, geometries_input_file: &str) -> Result<Self, String> {
        let zonal_records = read_records(zones_input_file)?;
        let geometries = read_geometries(geometries_input_file)?;

        // check sizes match
        if zonal_records.len() != geometries.len() {
            let msg = format!(
                "file {} has {} records (besides global record), but file {} has {} geometries; sizes must match.",
                zones_input_file,
                zonal_records.len(),
                geometries_input_file,
                geometries.len()
            );
            return Err(msg);
        }

        let rows = geometries.into_iter().zip(zonal_records).collect_vec();
        let rtree = PolygonalRTree::new(rows)
            .map_err(|e| format!("failure building spatial index: {e}"))?;
        Ok(Self { rtree })
    }

    /// uses the destination vertex of the current edge traversal to find any intersecting
    /// zones. filters out zonal rules that do not match our trip datetime. if we are boarded on a GBFS
    /// provider we further filter to only zones with a matching ServiceId. combines all
    /// zone record rules in order to produce a single value result.
    pub fn get_zone_rules<'b>(
        &self,
        vertex: &'b Vertex,
        state: &'b [StateVariable],
        state_model: &'b StateModel,
        start_time: DateTime<Utc>,
        service_id: Option<&'b String>,
    ) -> Result<Vec<ZoneState>, String> {
        let query = geo::Geometry::Point(geo::Point(vertex.coordinate.0));
        let time = get_trip_datetime(state, state_model, start_time)?;
        let intersection = self.spatiotemporal_intersection(&query, time, service_id)?;
        let rules = ZoneState::collect_zones(intersection);
        Ok(rules)
    }

    /// inner function that finds all zone matches that intersect a spatiotemporal query.
    fn spatiotemporal_intersection<'a, 'b: 'a>(
        &'a self,
        spatial_query: &'b Geometry<f32>,
        time: DateTime<Utc>,
        system_id: Option<&'b String>,
    ) -> Result<Vec<&'a GbfsZoneRecord>, String> {
        let matches = self
            .rtree
            .intersection(spatial_query)
            .map_err(|e| format!("failure running GBFS zone lookup: {e}"))?
            .filter_map(move |z| zone_matches_search(&z.data, time, system_id));

        Ok(matches.collect_vec())
    }
}

/// times are documented with "If the geofencing zone is always active, this can be omitted."
/// in this way, we optimistically choose `true` when only one of [start,end] are present.
///
/// if we are currently boarded, we only accept zone records with a matching system id.
fn zone_matches_search<'a>(
    record: &'a GbfsZoneRecord,
    time: DateTime<Utc>,
    system_id: Option<&String>,
) -> Option<&'a GbfsZoneRecord> {
    let service_ok = match system_id {
        Some(id) => id == &record.system_id,
        None => true,
    };
    let time_ok = match (record.start, record.end) {
        (Some(s), Some(e)) => s <= time && time < e,
        _ => true,
    };
    if service_ok && time_ok {
        Some(record)
    } else {
        None
    }
}

/// reads the records and builds a ZoneGraph from them. holds aside the global record
fn read_records(zone_record_input_file: &str) -> Result<Vec<GbfsZoneRecord>, String> {
    let bb = BarBuilder::default().desc("reading zone records");
    let zone_records: Box<[GbfsZoneRecord]> =
        read_utils::from_csv(&zone_record_input_file, true, Some(bb), None)
            .map_err(|e| format!("failure reading zone records: {e}"))?;
    Ok(zone_records.to_vec())
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
