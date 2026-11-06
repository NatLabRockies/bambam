use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use uom::{si::f64::Length, ConstZero};

/// a row in the schedules CSV file representing, for a given route,
/// the time of departure from some source stop location and arrival at some destination
/// stop location, along some EdgeId in the RouteE Compass Graph. its unique namespace
/// is defined by its feed_id, agency_id, service_id and route_id.
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
    /// walking distance from the source road vertex to the boarding GTFS stop.
    #[serde(default)]
    pub src_walk_distance: Length,
    /// walking distance from the alighting GTFS stop to the destination road vertex.
    #[serde(default)]
    pub dst_walk_distance: Length,
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
        Self::new_with_walk_distance(
            edge_id,
            feed_id,
            route_id,
            service_id,
            agency_id,
            src_departure_time,
            dst_arrival_time,
            Length::ZERO,
            Length::ZERO,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_walk_distance(
        edge_id: usize,
        feed_id: Option<String>,
        route_id: String,
        service_id: String,
        agency_id: Option<String>,
        src_departure_time: NaiveDateTime,
        dst_arrival_time: NaiveDateTime,
        src_walk_distance: Length,
        dst_walk_distance: Length,
    ) -> ScheduleRow {
        ScheduleRow {
            edge_id,
            feed_id,
            route_id,
            service_id,
            agency_id,
            src_departure_time,
            dst_arrival_time,
            src_walk_distance,
            dst_walk_distance,
        }
    }
}
