use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// a row in the schedules CSV file representing, for a given route,
/// the time of departure from some source stop location and arrival at some destination
/// stop location, along some EdgeId in the RouteE Compass Graph. its unique namespace
/// is defined by it's edge_list_id, feed_id, agency_id, service_id and route_id.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ScheduleRow {
    /// edge in Compass graph this row corresponds to.
    pub edge_id: usize,
    /// the GTFS feed or archive identifier
    pub feed_id: Option<String>,
    /// the unique name of this route within this GTFS Agency
    pub route_id: String,
    /// the unique name of the service schedule attached to this Route. a Route may
    /// correspond with multiple service ids.
    pub service_id: String,
    /// the agency providing this route, if listed.
    pub agency_id: Option<String>,
    /// departure time at beginning of this edge.
    pub src_departure_time: NaiveDateTime,
    /// arrival time at end of this edge.
    pub dst_arrival_time: NaiveDateTime,
}

impl ScheduleRow {
    pub fn new(
        edge_id: usize,
        feed_id: Option<String>,
        route_id: String,
        service_id: String,
        agency_id: Option<String>,
        src_departure_time: NaiveDateTime,
        dst_arrival_time: NaiveDateTime,
    ) -> ScheduleRow {
        ScheduleRow {
            edge_id,
            feed_id,
            route_id,
            service_id,
            agency_id,
            src_departure_time,
            dst_arrival_time,
        }
    }
}
