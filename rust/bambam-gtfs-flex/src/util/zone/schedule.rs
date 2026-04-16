use chrono::NaiveTime;

/// a time range of supported GTFS-Flex activity between two zones.
/// ordering is checked only by the start_time value. this type is
/// used as the item in an ordered skip list and so it is also used
/// for a ZoneSchedule.query(time) where end_time is ignored.
#[derive(Clone, Debug)]
pub struct ZoneSchedule {
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
}

impl ZoneSchedule {
    pub fn new(start_time: NaiveTime, end_time: NaiveTime) -> Self {
        Self {
            start_time,
            end_time,
        }
    }

    /// set up a skip list query. [ZoneSchedule] ordering is determined
    /// based only on the start time value so end_time is trivially set
    /// to equal start_time and ignored.
    pub fn query(time: NaiveTime) -> Self {
        Self {
            start_time: time,
            end_time: time,
        }
    }

    /// tests if the provided [NaiveTime] value is contained within the
    /// exclusive time range [start, end).
    pub fn contains(&self, time: &NaiveTime) -> bool {
        &self.start_time <= time && time < &self.end_time
    }
}

impl Ord for ZoneSchedule {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.start_time.cmp(&other.end_time)
    }
}

impl PartialOrd for ZoneSchedule {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ZoneSchedule {
    fn eq(&self, other: &Self) -> bool {
        self.start_time == other.start_time && self.end_time == other.end_time
    }
}

impl Eq for ZoneSchedule {}
