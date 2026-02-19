use chrono::TimeDelta;

use super::{ZoneError, ZoneId, ZoneRecord};

/// note:
/// you know, the zone times for type 3 are time delta values (hh:mm:ss).
/// in order to know what day of the year the relation is supported for,
/// we probably need to look up the Trip via trip_id, then use the calendar
/// things to look up a date match via the service_id.
#[derive(Clone, Debug)]
pub enum ZonalRelation {
    SelfLoop(ZoneId),
    ToZone {
        dst_zone_id: ZoneId,
    },
    ToZoneScheduled {
        dst_zone_id: ZoneId,
        start_time: TimeDelta,
        end_time: TimeDelta,
    },
}

impl ZonalRelation {
    /// the id to lookup when considering the vehicle's current location and whether it is a
    /// valid relation for this agency. this may be the zone id of a self-loop or the dst zone
    /// id of some zone-to-zone relation.
    pub fn lookup_id(&self) -> &ZoneId {
        match self {
            ZonalRelation::SelfLoop(zone_id) => zone_id,
            ZonalRelation::ToZone { dst_zone_id } => dst_zone_id,
            ZonalRelation::ToZoneScheduled { dst_zone_id, .. } => dst_zone_id,
        }
    }
}

impl TryFrom<&ZoneRecord> for ZonalRelation {
    type Error = ZoneError;

    fn try_from(record: &ZoneRecord) -> Result<Self, Self::Error> {
        // depending on the presence of fields on the incoming record we build
        // a different kind of relation
        let dst_zone_id = record.dst_zone_id.as_ref();
        let start_time = record.start_time.as_ref();
        let end_time = record.end_time.as_ref();

        match (dst_zone_id, start_time, end_time) {
            (None, None, None) => Ok(Self::SelfLoop(record.src_zone_id.clone())),
            (Some(d_id), None, None) => Ok(Self::ToZone {
                dst_zone_id: d_id.clone(),
            }),
            (Some(d_id), Some(s_t), Some(e_t)) => Ok(Self::ToZoneScheduled {
                dst_zone_id: d_id.clone(),
                start_time: *s_t,
                end_time: *e_t,
            }),
            _ => {
                let msg = format!(
                    "GTFS-Flex trip {} has invalid combination of optional fields",
                    record.trip_id
                );
                Err(ZoneError::Build(msg))
            }
        }
    }
}

impl std::fmt::Display for ZonalRelation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // use the debug implementation
        let s = format!("{self:?}");
        write!(f, "{s}")
    }
}
