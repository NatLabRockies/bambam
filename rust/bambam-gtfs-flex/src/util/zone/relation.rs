use std::ops::Bound;

use chrono::NaiveTime;
use skiplist::OrderedSkipList;

use crate::util::zone::ZoneSchedule;

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
#[derive(Debug)]
pub enum ZonalRelation {
    /// relation to a zone (possibly a self-loop) where time is ignored.
    ToZone { dst_zone_id: ZoneId },
    /// relation to a zone (possibly a self-loop) with a time window constraint.
    ToZoneScheduled {
        dst_zone_id: ZoneId,
        schedule: ZoneSchedule,
    },
    /// relation to a zone (possibly a self-loop) with multiple time window constraints.
    ToZoneMultipleSchedules {
        dst_zone_id: ZoneId,
        schedules: OrderedSkipList<ZoneSchedule>,
    },
}

impl ZonalRelation {
    /// the id to lookup when considering the vehicle's current location and whether it is a
    /// valid relation for this agency. this may be the zone id of a self-loop or the dst zone
    /// id of some zone-to-zone relation.
    pub fn lookup_id(&self) -> &ZoneId {
        match self {
            Self::ToZone { dst_zone_id } => dst_zone_id,
            Self::ToZoneScheduled { dst_zone_id, .. } => dst_zone_id,
            Self::ToZoneMultipleSchedules { dst_zone_id, .. } => dst_zone_id,
        }
    }

    /// tests if the provided datetime is valid for this particular [ZonalRelation].
    pub fn valid_time(&self, current_time: &NaiveTime) -> bool {
        match self {
            Self::ToZone { .. } => true,
            Self::ToZoneScheduled { schedule, .. } => schedule.contains(current_time),
            Self::ToZoneMultipleSchedules { schedules, .. } => {
                let query = Bound::Included(&ZoneSchedule::query(*current_time));
                match schedules.lower_bound(query) {
                    Some(schedule) => schedule.contains(current_time),
                    None => false,
                }
            }
        }
    }

    /// append a time range to an already-existing [ZonalRelation]. may upscale the
    /// variant to accomodate the additional time range.
    pub fn add_schedule(&mut self, schedule: ZoneSchedule) {
        match self {
            ZonalRelation::ToZone { dst_zone_id } => {
                *self = Self::ToZoneScheduled {
                    dst_zone_id: dst_zone_id.clone(),
                    schedule,
                }
            }
            ZonalRelation::ToZoneScheduled {
                dst_zone_id,
                schedule: prev_schedule,
            } => {
                *self = Self::ToZoneMultipleSchedules {
                    dst_zone_id: dst_zone_id.clone(),
                    schedules: OrderedSkipList::from_iter([prev_schedule.clone(), schedule]),
                };
            }
            ZonalRelation::ToZoneMultipleSchedules { schedules, .. } => {
                schedules.insert(schedule);
            }
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
            (None, None, None) => Ok(Self::ToZone {
                dst_zone_id: record.src_zone_id.clone(),
            }),
            (Some(d_id), None, None) => Ok(Self::ToZone {
                dst_zone_id: d_id.clone(),
            }),
            (Some(d_id), Some(s_t), Some(e_t)) => Ok(Self::ToZoneScheduled {
                dst_zone_id: d_id.clone(),
                schedule: ZoneSchedule::new(*s_t, *e_t),
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
