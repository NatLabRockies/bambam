use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::schedule::{fq_ops, ScheduleRow};

/// a row in the schedules CSV file representing, for a given route,
/// the time of departure from some source stop location and arrival at some destination
/// stop location, along some EdgeId in the RouteE Compass Graph. its unique namespace
/// is defined by it's edge_list_id, feed_id, agency_id, service_id and route_id.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FullyQualifiedScheduleRow {
    pub fully_qualified_id: String,
    /// edge in Compass graph this row corresponds to.
    pub edge_id: usize,
    /// edge list in Compass graph this row corresponds to.
    pub edge_list_id: usize,
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

impl FullyQualifiedScheduleRow {
    pub fn new(row: &ScheduleRow, edge_list_id: usize) -> FullyQualifiedScheduleRow {
        let fully_qualified_id = fq_ops::get_fully_qualified_route_id(
            row.feed_id.as_deref(),
            row.agency_id.as_deref(),
            &row.route_id,
            &row.service_id,
            edge_list_id,
        );
        FullyQualifiedScheduleRow {
            fully_qualified_id,
            edge_list_id,
            edge_id: row.edge_id,
            feed_id: row.feed_id.clone(),
            route_id: row.route_id.clone(),
            service_id: row.service_id.clone(),
            agency_id: row.agency_id.clone(),
            src_departure_time: row.src_departure_time,
            dst_arrival_time: row.dst_arrival_time,
        }
    }
}
