use geo::LineString;
use routee_compass_core::model::network::EdgeConfig;

use crate::schedule::ScheduleRow;

/// an edge created in the Compass Graph for some GTFS data.
pub struct GtfsEdge {
    pub edge: EdgeConfig,
    pub geometry: LineString,
    pub schedules: Vec<ScheduleRow>,
}

impl GtfsEdge {
    pub fn new(edge: EdgeConfig, geometry: LineString) -> Self {
        Self {
            edge,
            geometry,
            schedules: vec![],
        }
    }

    pub fn add_schedule(&mut self, schedule: ScheduleRow) {
        self.schedules.push(schedule);
    }
}
