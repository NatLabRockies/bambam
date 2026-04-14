use std::{num::NonZeroU64, sync::Arc};

use crate::model::constraint::multimodal::Constraint;
use bambam_core::model::state::MultimodalStateMapping;

#[derive(Debug)]
pub struct MultimodalConstraintEngine {
    pub mode: String,
    pub mode_to_state: Arc<MultimodalStateMapping>,
}
