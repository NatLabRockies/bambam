use std::{fs::File, io::BufReader, path::Path, sync::Arc};

use crate::model::traversal::transit::{
    config::TransitTraversalConfig, metadata::GtfsArchiveMetadata,
    schedule_loading_policy::ScheduleLoadingPolicy,
};
use bambam_core::model::state::CategoricalStateMapping;
use bambam_gtfs::{EdgeDeparture, EdgeTimetable, RoutingError};
use chrono::NaiveDateTime;
use routee_compass_core::{model::traversal::TraversalModelError, util::fs::read_utils};
use uom::si::f64::{Time, Velocity};

pub struct TransitTraversalEngine {
    pub timetable: EdgeTimetable,
    pub walk_speed: Option<Velocity>,
}

impl TransitTraversalEngine {
    pub fn new(timetable: EdgeTimetable, walk_speed: Option<Velocity>) -> Self {
        Self {
            timetable,
            walk_speed,
        }
    }

    pub fn num_edges(&self) -> usize {
        self.timetable.num_edges()
    }

    /// traverses the schedule of departing trips on `edge_id` to find the one that arrives soonest
    /// at the destination of this edge when departing at `current_datetime`.
    ///
    /// When `current_route_id` differs from a candidate schedule's `route_label`, the vertex-to-stop
    /// `boarding_walk_delay` is applied using `walk_speed` (or `self.walk_speed`).
    pub fn get_next_departure(
        &self,
        edge_id: usize,
        current_datetime: &NaiveDateTime,
        current_route_id: Option<i64>,
        walk_speed: Option<Velocity>,
    ) -> Result<Option<(i64, EdgeDeparture, Time)>, TraversalModelError> {
        let effective_speed = walk_speed.or(self.walk_speed);
        self.timetable
            .get_next_departure(edge_id, current_datetime, current_route_id, effective_speed)
            .map_err(|e| match e {
                RoutingError::InvalidEdgeId { .. } => {
                    TraversalModelError::InternalError(e.to_string())
                }
                _ => TraversalModelError::TraversalModelFailure(e.to_string()),
            })
    }
}

impl TryFrom<TransitTraversalConfig> for TransitTraversalEngine {
    type Error = TraversalModelError;

    fn try_from(value: TransitTraversalConfig) -> Result<Self, Self::Error> {
        log::debug!(
            "loading transit traversal model from {}",
            value.gtfs_metadata_input_file
        );

        if value.edges_schedules_input_file.ends_with(".rkyv") {
            let timetable =
                EdgeTimetable::load_rkyv(&value.edges_schedules_input_file).map_err(|e| {
                    TraversalModelError::BuildError(format!(
                        "Failed to load rkyv edge timetable from {}: {e}",
                        value.edges_schedules_input_file
                    ))
                })?;
            return Ok(Self {
                timetable,
                walk_speed: value.walk_speed,
            });
        }

        // Deserialize metadata file
        let file = File::open(&value.gtfs_metadata_input_file).map_err(|e| {
            TraversalModelError::BuildError(format!("Failed to read metadata file: {e}"))
        })?;
        let metadata: GtfsArchiveMetadata =
            serde_json::from_reader(BufReader::new(file)).map_err(|e| {
                TraversalModelError::BuildError(format!("Failed to read metadata file: {e}"))
            })?;

        let route_id_to_state = match &value.route_ids_input_file {
            Some(route_ids_input_file) => CategoricalStateMapping::from_enumerated_category_file(
                Path::new(&route_ids_input_file),
            )?,
            None => CategoricalStateMapping::new(&metadata.fq_route_ids)?,
        };

        log::debug!(
            "loaded {} fq route ids into mapping",
            route_id_to_state.n_categories()
        );

        let timetable = read_schedules_from_file(
            value.edges_schedules_input_file,
            Arc::new(route_id_to_state),
            value.schedule_loading_policy,
            metadata.fq_route_ids,
        )?;

        Ok(Self {
            timetable,
            walk_speed: value.walk_speed,
        })
    }
}

/// Reads schedule rows from a CSV file and compiles them into a dense 3-array CSR [`EdgeTimetable`].
fn read_schedules_from_file(
    filename: String,
    route_mapping: Arc<CategoricalStateMapping>,
    schedule_loading_policy: ScheduleLoadingPolicy,
    fq_route_ids: Vec<String>,
) -> Result<EdgeTimetable, TraversalModelError> {
    let rows: Box<[super::RawScheduleRow]> =
        read_utils::from_csv(&Path::new(&filename), true, None, None).map_err(|e| {
            TraversalModelError::BuildError(format!("Error creating reader to schedules file: {e}"))
        })?;

    log::debug!("{filename} - loaded {} raw schedule rows", rows.len());

    let mut builder = EdgeTimetable::builder().with_fq_route_ids(fq_route_ids);
    for record in rows {
        let route_i64 = route_mapping.get_label(&record.fully_qualified_id).ok_or(
            TraversalModelError::BuildError(format!(
                "Cannot find route id mapping for string {}",
                record.fully_qualified_id
            )),
        )?;

        schedule_loading_policy.insert_into_edge_timetable(
            &mut builder,
            record.edge_id,
            *route_i64,
            EdgeDeparture::new(
                record.src_departure_time,
                record.dst_arrival_time,
                record.src_walk_distance,
                record.dst_walk_distance,
            ),
        );
    }

    let timetable = builder.build().map_err(|e| {
        TraversalModelError::BuildError(format!("Invalid schedules file {filename}: {e}"))
    })?;

    log::debug!(
        "{filename} - built CSR EdgeTimetable for {} edges ({} total departures)",
        timetable.num_edges(),
        timetable.departures.len()
    );

    Ok(timetable)
}

#[cfg(test)]
mod test {
    use crate::model::traversal::transit::{
        engine::TransitTraversalEngine,
        schedule::{Departure, Schedule},
    };
    use bambam_gtfs::{EdgeDeparture, EdgeTimetable};
    use chrono::NaiveDateTime;

    fn internal_date(string: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(&format!("20250101 {string}"), "%Y%m%d %H:%M:%S").unwrap()
    }

    fn get_dummy_engine() -> TransitTraversalEngine {
        // Two edges that reverse each other and two routes that move across them:
        // Route 0:
        // 16:00 - 16:05 (A-B) -> 16:05 - 16:10 (B-A) -> 16:10 - 16:25 dwell -> 16:25 - 16:30 (A-B) -> 16:30 - 16:35 (B-A)
        // Route 1:
        // 16:15 - 16:45 (A-B) -> 16:45 - 17:00 (B-A)
        let mut builder = EdgeTimetable::builder();
        for (src, dst) in [("16:00:00", "16:05:00"), ("16:25:00", "16:30:00")] {
            builder.add_departure(
                0,
                0,
                EdgeDeparture::without_walk_distance(internal_date(src), internal_date(dst)),
            );
        }
        builder.add_departure(
            0,
            1,
            EdgeDeparture::without_walk_distance(
                internal_date("16:15:00"),
                internal_date("16:45:00"),
            ),
        );

        for (src, dst) in [("16:05:00", "16:10:00"), ("16:30:00", "16:35:00")] {
            builder.add_departure(
                1,
                0,
                EdgeDeparture::without_walk_distance(internal_date(src), internal_date(dst)),
            );
        }
        builder.add_departure(
            1,
            1,
            EdgeDeparture::without_walk_distance(
                internal_date("16:45:00"),
                internal_date("17:00:00"),
            ),
        );

        TransitTraversalEngine::new(builder.build().unwrap(), None)
    }

    fn dummy_schedule(times: &[(&str, &str)]) -> Schedule {
        let departures = times.iter().map(|(src, dst)| Departure {
            src_departure_time: internal_date(src),
            dst_arrival_time: internal_date(dst),
        });
        Schedule::from_iter(departures)
    }

    #[test]
    fn test_get_next_departure() {
        let engine = get_dummy_engine();

        let mut current_edge: usize = 0;
        let mut current_time = internal_date("15:50:00");
        let mut next_tuple = engine
            .get_next_departure(current_edge, &current_time, None, None)
            .unwrap()
            .unwrap();
        let mut next_route = next_tuple.0;
        let mut next_departure = next_tuple.1;

        assert_eq!(next_route, 0);
        assert_eq!(next_departure.src_departure_time, internal_date("16:00:00"));

        // Traverse 3 times the next edge
        for _ in 0..3 {
            next_tuple = engine
                .get_next_departure(current_edge, &current_time, Some(next_route), None)
                .unwrap()
                .unwrap();
            next_route = next_tuple.0;
            next_departure = next_tuple.1;

            current_time = next_departure.dst_arrival_time;
            current_edge = 1 - current_edge;
        }

        assert_eq!(next_route, 0);
        assert_eq!(current_time, internal_date("16:30:00"));

        // Ride transit one more time
        next_tuple = engine
            .get_next_departure(current_edge, &current_time, Some(next_route), None)
            .unwrap()
            .unwrap();
        next_departure = next_tuple.1;

        current_time = next_departure.dst_arrival_time;
        current_edge = 1 - current_edge;

        // If we wait now, we will find there are no more departures
        let result = engine
            .get_next_departure(current_edge, &current_time, Some(next_route), None)
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_schedule_from_iter() {
        let departures = vec![
            Departure {
                src_departure_time: internal_date("10:00:00"),
                dst_arrival_time: internal_date("10:15:00"),
            },
            Departure {
                src_departure_time: internal_date("08:00:00"),
                dst_arrival_time: internal_date("08:20:00"),
            },
            Departure {
                src_departure_time: internal_date("09:00:00"),
                dst_arrival_time: internal_date("09:10:00"),
            },
        ];

        let schedule = Schedule::from_iter(departures);
        assert_eq!(schedule.len(), 3);

        // Should be ordered automatically
        let ordered: Vec<&Departure> = schedule.iter().collect();
        assert_eq!(ordered[0].src_departure_time, internal_date("08:00:00"));
        assert_eq!(ordered[1].src_departure_time, internal_date("09:00:00"));
        assert_eq!(ordered[2].src_departure_time, internal_date("10:00:00"));
    }

    #[test]
    fn test_schedule_comprehensive_search_scenario() {
        // Create a realistic bus schedule with multiple departures throughout the day
        let schedule = dummy_schedule(&[
            ("06:00:00", "06:25:00"), // Early morning
            ("06:30:00", "06:55:00"),
            ("07:00:00", "07:25:00"), // Rush hour starts
            ("07:15:00", "07:40:00"),
            ("07:30:00", "07:55:00"),
            ("08:00:00", "08:25:00"),
            ("09:00:00", "09:25:00"), // Off-peak
            ("10:00:00", "10:25:00"),
            ("17:00:00", "17:25:00"), // Evening rush
            ("17:30:00", "17:55:00"),
            ("18:00:00", "18:25:00"),
            ("22:00:00", "22:25:00"), // Late evening
        ]);

        // Test various search scenarios
        let test_cases = vec![
            ("05:30:00", Some("06:00:00")), // Before service starts
            ("06:00:00", Some("06:00:00")), // Exact match
            ("06:10:00", Some("06:30:00")), // Between departures
            ("07:20:00", Some("07:30:00")), // During rush hour
            ("12:00:00", Some("17:00:00")), // Large gap in service
            ("21:00:00", Some("22:00:00")), // Evening service
            ("23:00:00", None),             // After service ends
        ];

        for (search_time, expected_time) in test_cases {
            let search_departure = Departure {
                src_departure_time: internal_date(search_time),
                dst_arrival_time: internal_date(search_time),
            };

            let result = schedule.lower_bound(std::ops::Bound::Included(&search_departure));

            match expected_time {
                Some(expected) => {
                    assert!(
                        result.is_some(),
                        "Expected departure at {expected} for search time {search_time}"
                    );
                    assert_eq!(
                        result.unwrap().src_departure_time,
                        internal_date(expected),
                        "Search time {search_time} should find departure at {expected}"
                    );
                }
                None => {
                    assert!(
                        result.is_none(),
                        "Expected no departure for search time {search_time}"
                    );
                }
            }
        }
    }
}
