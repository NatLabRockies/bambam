use crate::model::constraint::time_limit::{TimeLimitConfig, TimeLimitConstraintConfig};

use super::time_limit_frontier_model::TimeLimitConstraintModel;
use routee_compass_core::config::ConfigJsonExtensions;
use routee_compass_core::model::{
    constraint::{ConstraintModel, ConstraintModelError, ConstraintModelService},
    state::StateModel,
    unit::TimeUnit,
};
use std::sync::Arc;
use uom::si::f64::Time;

pub struct TimeLimitConstraintService {
    time_limit: TimeLimitConfig,
}

impl TimeLimitConstraintService {
    pub fn new(conf: &TimeLimitConstraintConfig) -> TimeLimitConstraintService {
        TimeLimitConstraintService {
            time_limit: conf.time_limit.clone(),
        }
    }
}

impl ConstraintModelService for TimeLimitConstraintService {
    fn build(
        &self,
        query: &serde_json::Value,
        _state_model: Arc<StateModel>,
    ) -> Result<Arc<dyn ConstraintModel>, ConstraintModelError> {
        log::debug!("begin ConstraintModelService::build for TimeLimitConstraintService");
        let conf = match query.get(super::TIME_LIMIT_FIELD) {
            None => Ok(self.time_limit.clone()),
            Some(time_limit_json) => {
                let time_limit: TimeLimitConfig = serde_json::from_value(time_limit_json.clone())
                    .map_err(|e| {
                    ConstraintModelError::ConstraintModelError(format!(
                        "failure reading query time_limit for isochrone frontier model: {e}"
                    ))
                })?;
                Ok(time_limit)
            }
        }?;

        let time_limit = conf.time_limit()?;
        let model = TimeLimitConstraintModel { time_limit };
        Ok(Arc::new(model))
    }
}
