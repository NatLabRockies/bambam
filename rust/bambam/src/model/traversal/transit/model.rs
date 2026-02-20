use std::sync::Arc;

use crate::model::state::variable::EMPTY;
use crate::model::traversal::transit::transit_ops;
use crate::model::traversal::transit::{engine::TransitTraversalEngine, schedule::Departure};
use bambam_core::model::bambam_state;
use chrono::{Duration, NaiveDate, NaiveDateTime};
use routee_compass_core::model::traversal::TraversalModelError;
use routee_compass_core::model::{
    state::StateVariableConfig,
    traversal::{default::fieldname, TraversalModel},
};
use uom::{
    si::f64::{Length, Time},
    ConstZero,
};

pub struct TransitTraversalModel {
    engine: Arc<TransitTraversalEngine>,
    start_datetime: NaiveDateTime,
    record_dwell_time: bool,
}

impl TransitTraversalModel {
    pub fn new(
        engine: Arc<TransitTraversalEngine>,
        start_datetime: NaiveDateTime,
        record_dwell_time: bool,
    ) -> Self {
        Self {
            engine,
            start_datetime,
            record_dwell_time,
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
                    value: EMPTY,
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
        trajectory: (
            &routee_compass_core::model::network::Vertex,
            &routee_compass_core::model::network::Edge,
            &routee_compass_core::model::network::Vertex,
        ),
        state: &mut Vec<routee_compass_core::model::state::StateVariable>,
        tree: &routee_compass_core::algorithm::search::SearchTree,
        state_model: &routee_compass_core::model::state::StateModel,
    ) -> Result<(), routee_compass_core::model::traversal::TraversalModelError> {
        let current_edge_id = trajectory.1.edge_id;
        let current_route_id = state_model.get_custom_i64(state, bambam_state::ROUTE_ID)?;
        let current_datetime =
            transit_ops::get_current_time(&self.start_datetime, state, state_model)?;

        // get the next departure.
        // in the case that no schedules are found, a sentinel value is returned set
        // far in the future (an "infinity" value). this indicates that this edge should not
        // have been accepted by the ConstraintModel. but at this point, we do not have a
        // transit frontier model, so "infinity" must solve the same problem.
        let (next_route, next_departure) = self
            .engine
            .get_next_departure(current_edge_id.as_usize(), &current_datetime)?;
        let next_departure_route_id = next_route;

        // update the state. a bunch of features are modified here.
        // NOTE: wait_time is "time waiting in the transit stop" OR "time waiting sitting on the bus during scheduled dwell time"
        let wait_time = Time::new::<uom::si::time::second>(
            (next_departure.src_departure_time - current_datetime).as_seconds_f64(),
        );
        let travel_time = Time::new::<uom::si::time::second>(
            (next_departure.dst_arrival_time - next_departure.src_departure_time).as_seconds_f64(),
        );
        let total_time = wait_time + travel_time;

        // Update state
        state_model.add_time(state, fieldname::TRIP_TIME, &total_time);
        state_model.add_time(state, fieldname::EDGE_TIME, &total_time);
        state_model.set_custom_i64(state, bambam_state::ROUTE_ID, &next_departure_route_id);

        // TRANSIT_BOARDING_TIME accumulates time waiting at transit stops, but not dwell time
        if current_route_id != next_departure_route_id {
            state_model.add_time(state, bambam_state::TRANSIT_BOARDING_TIME, &wait_time);
        } else if self.record_dwell_time {
            state_model.add_time(state, bambam_state::DWELL_TIME, &wait_time);
        }

        Ok(())
    }

    fn estimate_traversal(
        &self,
        od: (
            &routee_compass_core::model::network::Vertex,
            &routee_compass_core::model::network::Vertex,
        ),
        state: &mut Vec<routee_compass_core::model::state::StateVariable>,
        tree: &routee_compass_core::algorithm::search::SearchTree,
        state_model: &routee_compass_core::model::state::StateModel,
    ) -> Result<(), routee_compass_core::model::traversal::TraversalModelError> {
        Ok(())
    }
}
