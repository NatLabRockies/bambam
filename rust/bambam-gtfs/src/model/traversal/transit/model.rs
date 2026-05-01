use std::sync::Arc;

use crate::model::traversal::transit::engine::TransitTraversalEngine;
use crate::model::traversal::transit::transit_ops;
use bambam_core::model::bambam_state;
use bambam_core::model::state::variable;
use chrono::NaiveDateTime;
use routee_compass_core::model::state::{StateModel, StateVariable};
use routee_compass_core::model::traversal::{EdgeFrontierContext, TraversalModelError};
use routee_compass_core::model::{
    state::StateVariableConfig,
    traversal::{default::fieldname, TraversalModel},
};
use uom::{si::f64::Time, ConstZero};

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
                    value: variable::EMPTY,
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
    state: &mut Vec<StateVariable>,
    state_model: &StateModel,
    engine: &TransitTraversalEngine,
    start_datetime: &NaiveDateTime,
    record_dwell_time: bool,
) -> Result<(), TraversalModelError> {
    let current_route_id = state_model.get_custom_i64(state, bambam_state::ROUTE_ID)?;
    let current_datetime = transit_ops::get_current_time(start_datetime, state, state_model)?;

    // get the next departure.
    // in the case that no schedules are found, a sentinel value is returned set
    // far in the future (an "infinity" value). this indicates that this edge should not
    // have been accepted by the ConstraintModel. but at this point, we do not have a
    // transit frontier model, so "infinity" must solve the same problem.
    let (next_route, next_departure) =
        engine.get_next_departure(ctx.edge.edge_id.as_usize(), &current_datetime)?;
    let next_departure_route_id = next_route;

    // update the state. a bunch of features are modified here.
    // NOTE: wait_time is "time waiting in the transit stop" OR "time waiting sitting on the bus during scheduled dwell time"
    let wait_duration = (next_departure.src_departure_time - current_datetime).as_seconds_f64();
    if wait_duration < 0.0 {
        return Err(TraversalModelError::InternalError(format!(
            "fatal: caught departure in the past; edge_id: {}, start_datetime: {}, current_datetime: {}, current_route_id: {}, next_departure_route_id: {}, src_departure_time: {}, wait_duration_seconds: {}",
            ctx.edge.edge_id,
            start_datetime,
            current_datetime,
            current_route_id,
            next_departure_route_id,
            next_departure.src_departure_time,
            wait_duration
        )));
    }
    let wait_time = Time::new::<uom::si::time::second>(wait_duration);

    let travel_time = Time::new::<uom::si::time::second>(
        (next_departure.dst_arrival_time - next_departure.src_departure_time)
            .as_seconds_f64()
            .max(0.0),
    );
    let total_time = wait_time + travel_time;

    // Update state
    state_model.add_time(state, fieldname::TRIP_TIME, &total_time)?;
    state_model.add_time(state, fieldname::EDGE_TIME, &total_time)?;
    state_model.set_custom_i64(state, bambam_state::ROUTE_ID, &next_departure_route_id)?;

    // TRANSIT_BOARDING_TIME accumulates time waiting at transit stops, but not dwell time
    if current_route_id != next_departure_route_id {
        state_model.add_time(state, bambam_state::TRANSIT_BOARDING_TIME, &wait_time)?;
    } else if record_dwell_time {
        state_model.add_time(state, bambam_state::DWELL_TIME, &wait_time)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::traversal::transit::schedule::{Departure, Schedule};
    use chrono::{NaiveDate, NaiveDateTime};
    use routee_compass_core::algorithm::search::{Direction, SearchTree};
    use routee_compass_core::model::label::Label;
    use routee_compass_core::model::network::{Edge, EdgeId, EdgeListId, Vertex, VertexId};
    use routee_compass_core::model::state::{StateModel, StateVariableConfig};
    use std::collections::HashMap;
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
                    value: variable::EMPTY,
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

    fn mock_context(edge_id: usize) -> EdgeFrontierContext<'static> {
        // Build a static mock edge - we only care about its edge_id right now
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
        // Use Box::leak to keep the reference 'static as expected by EdgeFrontierContext
        let static_edge: &'static Edge = Box::leak(Box::new(edge));
        let static_src: &'static Vertex = Box::leak(Box::new(src));
        let static_dst: &'static Vertex = Box::leak(Box::new(dst));
        let static_label: &'static Label = Box::leak(Box::new(label));
        let tree = SearchTree::new_stateful(Direction::Forward);
        let static_tree: &'static SearchTree = Box::leak(Box::new(tree));
        EdgeFrontierContext {
            edge: static_edge,
            src: static_src,
            dst: static_dst,
            parent_label: static_label,
            tree: static_tree,
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
        // Record dwell time = true
        let state_model = mock_state_model(true);
        let start_datetime = internal_date("12:00:00");

        let deps = vec![Departure {
            src_departure_time: internal_date("12:05:00"),
            dst_arrival_time: internal_date("12:10:00"),
        }];

        let mut schedules_vec = Vec::new();
        // Edge 0 has Route 1
        schedules_vec.push(HashMap::from([(1, Schedule::from_iter(deps.clone()))]));
        // Edge 1 has Route 1 (Dwell simulation)
        schedules_vec.push(HashMap::from([(
            1,
            Schedule::from_iter(vec![Departure {
                src_departure_time: internal_date("12:15:00"),
                dst_arrival_time: internal_date("12:20:00"),
            }]),
        )]));
        // Edge 2 has Route 2 (Transfer simulation)
        schedules_vec.push(HashMap::from([(
            2,
            Schedule::from_iter(vec![Departure {
                src_departure_time: internal_date("12:25:00"),
                dst_arrival_time: internal_date("12:30:00"),
            }]),
        )]));

        let engine = Arc::new(TransitTraversalEngine {
            edge_schedules: schedules_vec.into_boxed_slice(),
            date_mapping: HashMap::new(),
        });

        let traversal_model = TransitTraversalModel::new(engine, start_datetime, true);
        let mut state = state_model
            .initial_state(None)
            .expect("failed to spawn state");

        // Edge 0 (First Boarding) - Wait 5m
        let ctx0 = mock_context(0);
        traversal_model
            .traverse_edge(&ctx0, &mut state, &state_model)
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
        let ctx1 = mock_context(1);
        traversal_model
            .traverse_edge(&ctx1, &mut state, &state_model)
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
        let ctx2 = mock_context(2);
        traversal_model
            .traverse_edge(&ctx2, &mut state, &state_model)
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
}
