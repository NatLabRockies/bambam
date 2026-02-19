use super::bambam_ops;
use routee_compass_core::model::state::{StateModel, StateModelError, StateVariable};
use serde::{Deserialize, Serialize};
use uom::si::f64::Time;

/// a configuration describing the time bounds for a "ring" of an isochrone.
/// time values are in minutes.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct TimeBin {
    pub min_time: u64,
    pub max_time: u64,
}

impl TimeBin {
    // construct a TimeBin. if no min is provided, use "0" minutes
    pub fn new(min: Option<u64>, max: u64) -> TimeBin {
        TimeBin {
            min_time: min.unwrap_or_default(),
            max_time: max,
        }
    }

    pub fn key(&self) -> String {
        format!("{}", self.max_time)
    }

    /// grab the time bin's lower bound as a Time value in a specified time unit
    pub fn min_time(&self) -> Time {
        Time::new::<uom::si::time::minute>(self.min_time as f64)
    }

    /// grab the time bin's upper bound as a Time value in a specified time unit
    pub fn max_time(&self) -> Time {
        Time::new::<uom::si::time::minute>(self.max_time as f64)
    }

    pub fn state_time_within_bin(
        &self,
        state: &[StateVariable],
        state_model: &StateModel,
    ) -> Result<bool, StateModelError> {
        let time = bambam_ops::get_reachability_time(state, state_model)?;
        let minutes = time.get::<uom::si::time::minute>() as u64;
        let within_bin = self.min_time <= minutes && minutes < self.max_time;
        Ok(within_bin)
    }
}
