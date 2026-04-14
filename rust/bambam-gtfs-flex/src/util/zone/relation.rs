use chrono::NaiveTime;

use super::{ZoneError, ZoneId, ZoneRecord};

/// A directed travel relation between GTFS-Flex zones.
///
/// # Date invariant
///
/// All `ZonalRelation` values are constructed from records that have already been
/// filtered to a single service date by the GTFS-Flex preprocessor
/// (`flex_processor::process_gtfs_flex_bundle`). That step joins `calendar.txt`
/// and `calendar_dates.txt` against the requested date and writes only the
/// active trips to the `valid-zones.csv` output. By the time a `ZonalRelation`
/// is built, the date-of-service question is fully resolved.
///
/// The only remaining time variability is the intra-day pickup/drop-off window
/// stored in `ToZoneScheduled`. `valid_time` checks that window against the
/// wall-clock component of the current datetime; the date component is ignored.
#[derive(Clone, Debug)]
pub enum ZonalRelation {
    SelfLoop(ZoneId),
    ToZone {
        dst_zone_id: ZoneId,
    },
    ToZoneScheduled {
        dst_zone_id: ZoneId,
        start_time: NaiveTime,
        end_time: NaiveTime,
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

    /// tests if the provided datetime is valid for this particular [ZonalRelation]
    pub fn valid_time(&self, current_time: &NaiveTime) -> bool {
        match self {
            ZonalRelation::ToZoneScheduled {
                start_time,
                end_time,
                ..
            } => start_time <= current_time && current_time < end_time,
            // without a scheduled time range, the time is valid by default
            _ => true,
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
