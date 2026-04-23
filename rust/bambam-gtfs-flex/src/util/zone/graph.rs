use std::{collections::HashMap, path::Path};

use chrono::NaiveDateTime;
use kdam::BarBuilder;
use routee_compass_core::util::fs::read_utils;

use crate::util::zone::ZoneError;

use super::{ZonalRelation, ZoneId, ZoneRecord};

/// a directed graph between GTFS-Flex zones. this metadata lookup
/// supports GTFS-Flex traversals, which must first lookup their current
/// location in a spatial index and their source zone in their state
/// vector. if both values exist, the model can call ZoneGraph::valid_zonal_trip
/// to determine whether the current edge is a destination.
pub struct ZoneGraph(ZoneGraphImpl);

/// represents all zone->zone relations where:
///   - the outer [ZoneId] key is a source zone
///   - the inner [ZoneId] key is a destination zone
///   - the inner [ZonalRelation] value is the kind of relation
type ZoneGraphImpl = HashMap<ZoneId, HashMap<ZoneId, ZonalRelation>>;

impl ZoneGraph {
    /// get the complete collection of [ZoneId]s that have relations in this graph.
    /// the keys of the inner hashmap cover all zones that exist.
    pub fn keys<'a>(&'a self) -> Box<dyn Iterator<Item = &'a ZoneId> + 'a> {
        Box::new(self.0.keys())
    }

    /// get the relations from some source [ZoneId], if they exist.
    pub fn get<'a>(&'a self, src_zone_id: &ZoneId) -> Option<&'a HashMap<ZoneId, ZonalRelation>> {
        self.0.get(src_zone_id)
    }

    pub fn valid_departure(
        &self,
        src_zone_id: &ZoneId,
        current_datetime: &NaiveDateTime,
    ) -> Result<bool, ZoneError> {
        // find all zone-to-zone relations starting from the src_zone_id.
        let relations = match self.0.get(src_zone_id) {
            None => return Ok(false), // cannot depart, not a source zone.
            Some(r) => r,
        };

        // accept this is valid if ANY relation treats this as a valid time.
        let current_time = current_datetime.time();
        let valid_time = relations.values().any(|r| r.valid_time(&current_time));

        Ok(valid_time)
    }

    /// confirms that this zone-to-zone trip exists in our zonal graph.
    pub fn valid_zonal_trip(
        &self,
        src_zone_id: &ZoneId,
        dst_zone_id: &ZoneId,
        _current_time: &NaiveDateTime,
    ) -> Result<bool, ZoneError> {
        // find zone-to-zone trips starting from src_zone_id
        let relations = match self.0.get(src_zone_id) {
            Some(r) => r,
            None => return Ok(false),
        };

        // check the destination exists and matches our current time
        match relations.get(dst_zone_id) {
            None => Ok(false),
            Some(_relation) => {
                // if there is no time validation to run, then we are done.
                // todo: run time validation here
                Ok(true)
            }
        }
    }
}

impl TryFrom<&Path> for ZoneGraph {
    type Error = ZoneError;

    fn try_from(value: &Path) -> Result<Self, Self::Error> {
        let bb = BarBuilder::default().desc("zone records");
        let records: Box<[ZoneRecord]> = read_utils::from_csv(&value, true, Some(bb), None)
            .map_err(|e| {
                let msg = format!("failure reading zone records: {e}");
                ZoneError::Build(msg)
            })?;
        ZoneGraph::try_from(&records[..])
    }
}

impl TryFrom<&[ZoneRecord]> for ZoneGraph {
    type Error = ZoneError;

    fn try_from(value: &[ZoneRecord]) -> Result<Self, Self::Error> {
        let mut graph: ZoneGraphImpl = HashMap::new();
        for row in value.iter() {
            insert_row(row, &mut graph)?;
        }
        Ok(Self(graph))
    }
}

/// insert a row into the [ZoneGraphImpl] during construction from a slice of records.
///
/// for each src_zone_id/dst_zone_id pair there exists a single [ZonalRelation] object.
/// however, if multiple rows reference the same relation but different time ranges,
/// we append the time range information to the existing [ZonalRelation].
fn insert_row(row: &ZoneRecord, graph: &mut ZoneGraphImpl) -> Result<(), ZoneError> {
    let relation = ZonalRelation::try_from(row)?;
    let lookup_id = relation.lookup_id();
    let schedule_opt = row.get_zone_schedule();
    match graph.get_mut(&row.src_zone_id) {
        Some(relations) => {
            // case where there are existing relations for this src_zone_id which may require
            // appending a schedule.
            relations
                .entry(lookup_id.clone())
                .and_modify(|r| {
                    // relation to this destination zone already exists, but, if we have a schedule
                    // we need to add it to the relation.
                    if let Some(schedule) = schedule_opt {
                        r.add_schedule(schedule);
                    }
                })
                .or_insert(relation);
            Ok(())
        }
        None => {
            // we must initialize the relations for src/dst
            let _ = graph.insert(
                row.src_zone_id.clone(),
                HashMap::from([(lookup_id.clone(), relation)]),
            );
            Ok(())
        }
    }
}
