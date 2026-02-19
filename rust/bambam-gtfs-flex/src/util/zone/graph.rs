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

type ZoneGraphImpl = HashMap<ZoneId, HashMap<ZoneId, ZonalRelation>>;

impl ZoneGraph {
    pub fn valid_departure(
        &self,
        _src_zone_id: &ZoneId,
        _current_time: &NaiveDateTime,
    ) -> Result<bool, ZoneError> {
        todo!()
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
            let relation = ZonalRelation::try_from(row)?;
            let lookup_id = relation.lookup_id();
            match graph.get_mut(&row.src_zone_id) {
                // case where there are existing relations for this src_zone_id
                Some(relations) => {
                    let insert_result = relations.insert(lookup_id.clone(), relation.clone());
                    match insert_result {
                        None => {}
                        Some(prev) => {
                            let msg = format!(
                                "GTFS-Flex trip_id {} collided with an existing zonal relation ({})->({})",
                                row.trip_id,
                                lookup_id,
                                &prev
                            );
                            return Err(ZoneError::Build(msg));
                        }
                    }
                }
                // we must initialize the relations for this src_zone_id
                None => {
                    let _ = graph.insert(
                        row.src_zone_id.clone(),
                        HashMap::from([(lookup_id.clone(), relation)]),
                    );
                }
            }
        }

        Ok(Self(graph))
    }
}
