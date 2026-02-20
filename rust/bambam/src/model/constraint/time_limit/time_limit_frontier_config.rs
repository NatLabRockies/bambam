use serde::{Deserialize, Serialize};

use crate::model::constraint::time_limit::TimeLimitConfig;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TimeLimitConstraintConfig {
    pub time_limit: TimeLimitConfig,
}
