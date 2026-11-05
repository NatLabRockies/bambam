use std::sync::Arc;

use crate::model::traversal::transit::engine::TransitTraversalEngine;
use crate::model::traversal::transit::transit_ops;
use bambam_core::model::bambam_state;
use bambam_core::model::state::variable;
use bambam_gtfs::EdgeDeparture;
use chrono::NaiveDateTime;
use routee_compass_core::model::state::{StateModel, StateVariable};
use routee_compass_core::model::traversal::{EdgeFrontierContext, TraversalModelError};
use routee_compass_core::model::{
    state::StateVariableConfig,
    traversal::{default::fieldname, TraversalModel},
};
use uom::{
    si::f64::{Time, Velocity},
    ConstZero,
};

pub struct TransitTraversalModel {
    engine: Arc<TransitTraversalEngine>,
    start_datetime: NaiveDateTime,
    record_dwell_time: bool,
    walk_speed: Option<Velocity>,
}

impl TransitTraversalModel {
    pub fn new(
        engine: Arc<TransitTraversalEngine>,
        start_datetime: NaiveDateTime,
        record_dwell_time: bool,
    ) -> Self {
        Self::new_with_walk_speed(engine, start_datetime, record_dwell_time, None)
    }

    pub fn new_with_walk_speed(
        engine: Arc<TransitTraversalEngine>,
        start_datetime: NaiveDateTime,
        record_dwell_time: bool,
        walk_speed: Option<Velocity>,
    ) -> Self {
        Self {
            engine,
            start_datetime,
            record_dwell_time,
            walk_speed,
        }
    }
}

impl TraversalModel for TransitTraversalModel {
    fn name(&self) -> String {
        "transit_traversal".to_string()
    }

    fn input_features(&self) -> Vec<routee_compass_core::model::state::InputFeature> {
        vec![]
    }

    fn output_features(
        &self,
    ) -> Vec<(
        String,
        routee_compass_core::model::state::StateVariableConfig,
    )> {
        let mut out = vec![
            (
                String::from(fieldname::TRIP_TIME),
                StateVariableConfig::Time {
                    initial: Time::ZERO,
                    output_unit: None,
                    accumulator: true,
                },
            ),
            (
                String::from(fieldname::EDGE_TIME),
                StateVariableConfig::Time {
                    initial: Time::ZERO,
                    output_unit: None,
                    accumulator: false,
                },
            ),
            (
                String::from(bambam_state::ROUTE_ID),
                StateVariableConfig::Custom {
                    custom_type: "RouteId".to_string(),
                    value: variable::EMPTY_VARIABLE_CONFIG,
                    accumulator: true,
                },
            ),
            (
                String::from(bambam_state::TRANSIT_BOARDING_TIME),
                StateVariableConfig::Time {
                    initial: Time::ZERO,
                    accumulator: false,
                    output_unit: None,
                },
            ),
        ];

        if self.record_dwell_time {
            out.push((
                String::from(bambam_state::DWELL_TIME),
                StateVariableConfig::Time {
                    initial: Time::ZERO,
                    accumulator: false,
                    output_unit: None,
                },
            ));
        }

        out
    }

    fn traverse_edge(
        &self,
        ctx: &EdgeFrontierContext,
        state: &mut Vec<StateVariable>,
        state_model: &StateModel,
    ) -> Result<(), routee_compass_core::model::traversal::TraversalModelError> {
        run_transit_traversal(
            ctx,
            state,
            state_model,
            &self.engine,
            &self.start_datetime,
            self.record_dwell_time,
            self.walk_speed,
        )
    }

    fn estimate_traversal(
        &self,
        _od: (
            &routee_compass_core::model::network::Vertex,
            &routee_compass_core::model::network::Vertex,
        ),
        _state: &mut Vec<routee_compass_core::model::state::StateVariable>,
        _tree: &routee_compass_core::algorithm::search::SearchTree,
        _state_model: &routee_compass_core::model::state::StateModel,
    ) -> Result<(), routee_compass_core::model::traversal::TraversalModelError> {
        Ok(())
    }
}

/// runs a single edge traversal in this agency.
fn run_transit_traversal(
    ctx: &EdgeFrontierContext,
    state: &mut [StateVariable],
    state_model: &StateModel,
    engine: &TransitTraversalEngine,
    start_datetime: &NaiveDateTime,
    record_dwell_time: bool,
    walk_speed: Option<Velocity>,
) -> Result<(), TraversalModelError> {
    let current_route_id = state_model.get_custom_i64(state, bambam_state::ROUTE_ID)?;
    let current_datetime = transit_ops::get_current_time(start_datetime, state, state_model)?;

    // get the next departure.
    // in the case that no schedules are found, a sentinel value is returned set
    // far in the future (an "infinity" value). this indicates that this edge should not
    // have been accepted by the ConstraintModel. but at this point, we do not have a
    // transit frontier model, so "infinity" must solve the same problem.
    let next_dep_opt = engine.get_next_departure(
        ctx.edge.edge_id.as_usize(),
        &current_datetime,
        Some(current_route_id),
        walk_speed,
    )?;

    let (next_departure_route_id, next_departure, _boarding_delay) = match next_dep_opt {
        Some(triple) => triple,
        None => {
            // no valid departures, add a 1-week penalty time value and exit prematurely.
            let sentinel: Time = Time::new::<uom::si::time::day>(NOT_FOUND_TIME_PENALTY_DAYS);
            state_model.add_time(state, fieldname::EDGE_TIME, &sentinel)?;
            state_model.add_time(state, fieldname::TRIP_TIME, &sentinel)?;
            return Ok(());
        }
    };

    // NOTE: wait_time is "time walking to & waiting at the transit stop" OR "time waiting sitting on the bus during scheduled dwell time"
    let wait_time = compute_wait_time(
        ctx,
        &current_datetime,
        current_route_id,
        next_departure_route_id,
        start_datetime,
        &next_departure,
    )?;
    let travel_time = compute_travel_time(ctx, &next_departure)?;
    let total_time = wait_time + travel_time;

    // Update state
    state_model.add_time(state, fieldname::TRIP_TIME, &total_time)?;
    state_model.add_time(state, fieldname::EDGE_TIME, &total_time)?;
    state_model.set_custom_i64(state, bambam_state::ROUTE_ID, &next_departure_route_id)?;

    // TRANSIT_BOARDING_TIME accumulates time walking to & waiting at transit stops, but not dwell time
    if current_route_id != next_departure_route_id {
        state_model.add_time(state, bambam_state::TRANSIT_BOARDING_TIME, &wait_time)?;
    } else if record_dwell_time {
        state_model.add_time(state, bambam_state::DWELL_TIME, &wait_time)?;
    }

    Ok(())
}

/// when no departure schedules are found, we are in a pickle, as we are already forcing an
/// edge traversal here. so, in order to comply with RouteE Compass' API, we have to update
/// the state to reflect that something happened. here, we inject a travel time of seven
/// days, so that, in the result, the user can filter these values in post-processing. future
/// work: wire in a constraint model for transit that uses the same schedules, ideally without
/// loading two copies into memory.
const NOT_FOUND_TIME_PENALTY_DAYS: f64 = 7.0;

/// helper function to compute the wait time between the current datetime and
/// the next scheduled departure. Returns an error if the departure is in the past.
fn compute_wait_time(
    ctx: &EdgeFrontierContext,
    current_datetime: &NaiveDateTime,
    current_route_id: i64,
    next_departure_route_id: i64,
    start_datetime: &NaiveDateTime,
    departure: &EdgeDeparture,
) -> Result<Time, TraversalModelError> {
    let wait_duration = (departure.src_departure_time - *current_datetime).as_seconds_f64();
    if wait_duration < 0.0 {
        return Err(TraversalModelError::InternalError(format!(
            "fatal: caught departure in the past; edge_id: {}, start_datetime: {}, current_datetime: {}, current_route_id: {}, next_departure_route_id: {}, src_departure_time: {}, wait_duration_seconds: {}",
            ctx.edge.edge_id,
            start_datetime,
            current_datetime,
            current_route_id,
            next_departure_route_id,
            departure.src_departure_time,
            wait_duration
        )));
    }
    Ok(Time::new::<uom::si::time::second>(wait_duration))
}

/// helper function to compute the travel time between the scheduled departure and arrival.
/// Returns an error if arrival precedes departure.
fn compute_travel_time(
    ctx: &EdgeFrontierContext,
    departure: &EdgeDeparture,
) -> Result<Time, TraversalModelError> {
    let travel_duration =
        (departure.dst_arrival_time - departure.src_departure_time).as_seconds_f64();
    if travel_duration < 0.0 {
        return Err(TraversalModelError::InternalError(format!(
            "fatal: caught negative travel time in schedule; edge_id: {}, src_departure_time: {}, dst_arrival_time: {}, travel_duration_seconds: {}",
            ctx.edge.edge_id,
            departure.src_departure_time,
            departure.dst_arrival_time,
            travel_duration
        )));
    }
    Ok(Time::new::<uom::si::time::second>(travel_duration))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;
    use routee_compass_core::algorithm::search::{Direction, SearchTree};
    use routee_compass_core::model::label::Label;
    use routee_compass_core::model::network::{Edge, EdgeId, EdgeListId, Vertex, VertexId};
    use routee_compass_core::model::state::{StateModel, StateVariableConfig};
    use uom::si::f64::{Length, Time};

    fn mock_state_model(record_dwell_time: bool) -> StateModel {
        let mut features = vec![
            (
                fieldname::TRIP_TIME.to_string(),
                StateVariableConfig::Time {
                    initial: Time::ZERO,
                    output_unit: None,
                    accumulator: true,
                },
            ),
            (
                fieldname::EDGE_TIME.to_string(),
                StateVariableConfig::Time {
                    initial: Time::ZERO,
                    output_unit: None,
                    accumulator: false,
                },
            ),
            (
                bambam_state::ROUTE_ID.to_string(),
                StateVariableConfig::Custom {
                    custom_type: "RouteId".to_string(),
                    value: variable::EMPTY_VARIABLE_CONFIG,
                    accumulator: true,
                },
            ),
            (
                bambam_state::TRANSIT_BOARDING_TIME.to_string(),
                StateVariableConfig::Time {
                    initial: Time::ZERO,
                    accumulator: false,
                    output_unit: None,
                },
            ),
        ];

        if record_dwell_time {
            features.push((
                bambam_state::DWELL_TIME.to_string(),
                StateVariableConfig::Time {
                    initial: Time::ZERO,
                    accumulator: false,
                    output_unit: None,
                },
            ));
        }

        StateModel::new(features)
    }

    fn internal_date(string: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(&format!("20250101 {string}"), "%Y%m%d %H:%M:%S").unwrap()
    }

    /// mocks the data for an EdgeFrontierContext and provides a method `context` to
    /// retrieve an EdgeFrontierContext with the stateful values referenced and lifetime
    /// requirements/borrowed status properly modeled.
    struct MockContext {
        edge: Edge,
        src: Vertex,
        dst: Vertex,
        label: Label,
        tree: SearchTree,
    }

    impl MockContext {
        fn new(edge_id: usize) -> Self {
            let edge = Edge {
                edge_id: EdgeId(edge_id),
                edge_list_id: EdgeListId(0),
                src_vertex_id: VertexId(0),
                dst_vertex_id: VertexId(1),
                distance: Length::new::<uom::si::length::meter>(100.0),
            };
            let src = Vertex::new(0, 0.0, 0.0);
            let dst = Vertex::new(1, 0.0, 0.0);
            let label = Label::new_u8_state(VertexId(0), &[]).unwrap();
            let tree = SearchTree::new_stateful(Direction::Forward);

            Self {
                edge,
                src,
                dst,
                label,
                tree,
            }
        }

        fn context<'a>(&'a self) -> EdgeFrontierContext<'a> {
            EdgeFrontierContext {
                edge: &self.edge,
                src: &self.src,
                dst: &self.dst,
                parent_label: &self.label,
                tree: &self.tree,
            }
        }
    }

    // Helper to simulate Compass advancing vertexes (only carrying over accumulators)
    fn advance_state(state: &[StateVariable], state_model: &StateModel) -> Vec<StateVariable> {
        let mut next_state = state_model
            .initial_state(None)
            .expect("failed to spawn state");

        // Copy standard accumulators
        state_model
            .set_time(
                &mut next_state,
                fieldname::TRIP_TIME,
                &state_model.get_time(state, fieldname::TRIP_TIME).unwrap(),
            )
            .unwrap();

        state_model
            .set_custom_i64(
                &mut next_state,
                bambam_state::ROUTE_ID,
                &state_model
                    .get_custom_i64(state, bambam_state::ROUTE_ID)
                    .unwrap(),
            )
            .unwrap();

        next_state
    }

    #[test]
    fn test_transfer_vs_dwell_edge_metrics() {
        use bambam_gtfs::EdgeTimetable;

        // Record dwell time = true
        let state_model = mock_state_model(true);
        let start_datetime = internal_date("12:00:00");

        let mut builder = EdgeTimetable::builder();
        // Edge 0 has Route 1
        builder.add_departure(
            0,
            1,
            EdgeDeparture::without_walk_distance(
                internal_date("12:05:00"),
                internal_date("12:10:00"),
            ),
        );
        // Edge 1 has Route 1 (Dwell simulation)
        builder.add_departure(
            1,
            1,
            EdgeDeparture::without_walk_distance(
                internal_date("12:15:00"),
                internal_date("12:20:00"),
            ),
        );
        // Edge 2 has Route 2 (Transfer simulation)
        builder.add_departure(
            2,
            2,
            EdgeDeparture::without_walk_distance(
                internal_date("12:25:00"),
                internal_date("12:30:00"),
            ),
        );

        let engine = Arc::new(TransitTraversalEngine::new(builder.build().unwrap(), None));

        let traversal_model = TransitTraversalModel::new(engine, start_datetime, true);
        let mut state = state_model
            .initial_state(None)
            .expect("failed to spawn state");

        // Edge 0 (First Boarding) - Wait 5m
        let ctx0 = MockContext::new(0);
        traversal_model
            .traverse_edge(&ctx0.context(), &mut state, &state_model)
            .unwrap();
        // Wait 300s. Because current_route (EMPTY) != next_route (1), it logs as generic board wait.
        assert_eq!(
            state_model
                .get_time(&state, bambam_state::TRANSIT_BOARDING_TIME)
                .unwrap(),
            Time::new::<uom::si::time::second>(300.0)
        );
        assert_eq!(
            state_model
                .get_time(&state, bambam_state::DWELL_TIME)
                .unwrap(),
            Time::new::<uom::si::time::second>(0.0)
        );

        // Advance to Edge 1
        let mut state = advance_state(&state, &state_model);

        // Edge 1 (Stay on Route 1, DWELL) - Wait 5m
        let ctx1 = MockContext::new(1);
        traversal_model
            .traverse_edge(&ctx1.context(), &mut state, &state_model)
            .unwrap();

        // BOARDING TIME on this edge is zero, DWELL TIME receives the 300s wait
        assert_eq!(
            state_model
                .get_time(&state, bambam_state::TRANSIT_BOARDING_TIME)
                .unwrap(),
            Time::new::<uom::si::time::second>(0.0)
        );
        assert_eq!(
            state_model
                .get_time(&state, bambam_state::DWELL_TIME)
                .unwrap(),
            Time::new::<uom::si::time::second>(300.0)
        );

        // Advance to Edge 2
        let mut state = advance_state(&state, &state_model);

        // Edge 2 (Transfer to Route 2) - Wait 5m
        let ctx2 = MockContext::new(2);
        traversal_model
            .traverse_edge(&ctx2.context(), &mut state, &state_model)
            .unwrap();

        // BOARDING TIME receives the 300s wait since changing routes, DWELL TIME is 0
        assert_eq!(
            state_model
                .get_time(&state, bambam_state::TRANSIT_BOARDING_TIME)
                .unwrap(),
            Time::new::<uom::si::time::second>(300.0)
        );
        assert_eq!(
            state_model
                .get_time(&state, bambam_state::DWELL_TIME)
                .unwrap(),
            Time::new::<uom::si::time::second>(0.0)
        );
    }

    #[test]
    fn test_no_departure_adds_not_found_penalty() {
        use bambam_gtfs::EdgeTimetable;

        let state_model = mock_state_model(false);
        let start_datetime = internal_date("12:00:00");

        // Edge 0 exists but has no departures for any route.
        let mut builder = EdgeTimetable::builder();
        builder.ensure_edge(0);
        let engine = Arc::new(TransitTraversalEngine::new(builder.build().unwrap(), None));

        let traversal_model = TransitTraversalModel::new(engine, start_datetime, false);
        let mut state = state_model
            .initial_state(None)
            .expect("failed to spawn state");
        let initial_route_id = state_model
            .get_custom_i64(&state, bambam_state::ROUTE_ID)
            .unwrap();

        let ctx0 = MockContext::new(0);
        traversal_model
            .traverse_edge(&ctx0.context(), &mut state, &state_model)
            .unwrap();

        let penalty = Time::new::<uom::si::time::day>(NOT_FOUND_TIME_PENALTY_DAYS);
        assert_eq!(
            state_model.get_time(&state, fieldname::EDGE_TIME).unwrap(),
            penalty
        );
        assert_eq!(
            state_model.get_time(&state, fieldname::TRIP_TIME).unwrap(),
            penalty
        );

        // No route or wait-state metrics should be updated on not-found traversal.
        assert_eq!(
            state_model
                .get_custom_i64(&state, bambam_state::ROUTE_ID)
                .unwrap(),
            initial_route_id
        );
        assert_eq!(
            state_model
                .get_time(&state, bambam_state::TRANSIT_BOARDING_TIME)
                .unwrap(),
            Time::ZERO
        );
    }

    #[test]
    fn test_no_departure_penalty_accumulates_existing_trip_time() {
        use bambam_gtfs::EdgeTimetable;

        let state_model = mock_state_model(false);
        let start_datetime = internal_date("12:00:00");

        // Edge 0 exists but has no departures for any route.
        let mut builder = EdgeTimetable::builder();
        builder.ensure_edge(0);
        let engine = Arc::new(TransitTraversalEngine::new(builder.build().unwrap(), None));

        let traversal_model = TransitTraversalModel::new(engine, start_datetime, false);
        let mut state = state_model
            .initial_state(None)
            .expect("failed to spawn state");

        let existing_trip_time = Time::new::<uom::si::time::minute>(15.0);
        state_model
            .set_time(&mut state, fieldname::TRIP_TIME, &existing_trip_time)
            .unwrap();

        let ctx = MockContext::new(0);
        traversal_model
            .traverse_edge(&ctx.context(), &mut state, &state_model)
            .unwrap();

        let penalty = Time::new::<uom::si::time::day>(NOT_FOUND_TIME_PENALTY_DAYS);
        assert_eq!(
            state_model.get_time(&state, fieldname::TRIP_TIME).unwrap(),
            existing_trip_time + penalty
        );
        assert_eq!(
            state_model.get_time(&state, fieldname::EDGE_TIME).unwrap(),
            penalty
        );
    }

    #[test]
    fn test_boarding_walk_delay_vs_already_boarded_continuation() {
        use bambam_gtfs::EdgeTimetable;
        use uom::si::length::meter;
        use uom::si::velocity::meter_per_second;

        let state_model = mock_state_model(true);
        let start_datetime = internal_date("12:00:00");
        let walk_speed = Velocity::new::<meter_per_second>(1.5); // 1.5 m/s -> 180m takes 120s (2 mins)

        let mut builder = EdgeTimetable::builder();
        // Edge 0, Route 1: stop is 180m from vertex (2 min walk).
        // Departure A at 12:01:00 (arr 12:05:00) -> unreachable when walking from vertex at 12:00:00!
        // Departure B at 12:03:00 (arr 12:08:00) -> reachable (ready at 12:02:00).
        builder.add_departure(
            0,
            1,
            EdgeDeparture::new(
                internal_date("12:01:00"),
                internal_date("12:05:00"),
                Length::new::<meter>(180.0),
                Length::new::<meter>(60.0),
            ),
        );
        builder.add_departure(
            0,
            1,
            EdgeDeparture::new(
                internal_date("12:03:00"),
                internal_date("12:08:00"),
                Length::new::<meter>(180.0),
                Length::new::<meter>(60.0),
            ),
        );

        // Edge 1, Route 1: next stop is also 180m from road vertex, departure at 12:09:00 (1 min dwell after 12:08:00 arrival).
        // Because rider is already on Route 1, the 180m walk delay is skipped!
        builder.add_departure(
            1,
            1,
            EdgeDeparture::new(
                internal_date("12:09:00"),
                internal_date("12:14:00"),
                Length::new::<meter>(180.0),
                Length::new::<meter>(60.0),
            ),
        );

        let engine = Arc::new(TransitTraversalEngine::new(
            builder.build().unwrap(),
            Some(walk_speed),
        ));
        let traversal_model = TransitTraversalModel::new(engine, start_datetime, true);

        let mut state = state_model.initial_state(None).unwrap();
        let ctx0 = MockContext::new(0);
        traversal_model
            .traverse_edge(&ctx0.context(), &mut state, &state_model)
            .unwrap();

        // Caught 12:03:00 departure (180s total boarding wait = 120s walk + 60s platform wait; 300s travel)
        assert_eq!(
            state_model
                .get_time(&state, bambam_state::TRANSIT_BOARDING_TIME)
                .unwrap(),
            Time::new::<uom::si::time::second>(180.0)
        );
        assert_eq!(
            state_model.get_time(&state, fieldname::TRIP_TIME).unwrap(),
            Time::new::<uom::si::time::second>(480.0)
        );

        // Advance to Edge 1 (already on Route 1 at 12:08:00)
        let mut state = advance_state(&state, &state_model);
        let ctx1 = MockContext::new(1);
        traversal_model
            .traverse_edge(&ctx1.context(), &mut state, &state_model)
            .unwrap();

        // Catches 12:09:00 departure (60s dwell, 0s boarding wait!)
        assert_eq!(
            state_model
                .get_time(&state, bambam_state::TRANSIT_BOARDING_TIME)
                .unwrap(),
            Time::new::<uom::si::time::second>(0.0)
        );
        assert_eq!(
            state_model
                .get_time(&state, bambam_state::DWELL_TIME)
                .unwrap(),
            Time::new::<uom::si::time::second>(60.0)
        );
    }
}
