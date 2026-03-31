use serde::{Deserialize, Serialize};

use crate::model::constraint::multimodal::ConstraintConfig;

/// query-time arguments to the multimodal constraint model
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MultimodalConstraintModelQuery {
    /// constraints to apply when in this mode
    pub constraints: Option<Vec<ConstraintConfig>>,
}
