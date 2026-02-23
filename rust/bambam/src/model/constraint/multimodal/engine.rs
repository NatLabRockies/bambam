use std::sync::Arc;

use crate::model::{constraint::multimodal::Constraint, state::MultimodalStateMapping};

#[derive(Debug)]
pub struct MultimodalConstraintEngine {
    pub mode: String,
    pub constraints: Vec<Constraint>,
    pub mode_to_state: Arc<MultimodalStateMapping>,
    pub route_id_to_state: Arc<Option<MultimodalStateMapping>>,
    pub max_trip_legs: u64,
}
