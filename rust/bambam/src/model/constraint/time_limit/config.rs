use serde::{Deserialize, Serialize};

use crate::model::constraint::time_limit::TimeLimit;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TimeLimitConstraintConfig {
    pub time_limit: TimeLimit,
}
