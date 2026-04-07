//! constructors for [`StateVariableConfig`] instances in multimodal routing.
use crate::model::state::{fieldname, LegIdx};
use routee_compass_core::model::{
    state::{CustomVariableConfig, InputFeature, StateVariableConfig},
    unit::{DistanceUnit, TimeUnit},
};
use uom::{
    si::f64::{Length, Time},
    ConstZero,
};

/// config value representing an empty LegIdx, Mode, or RouteId.
pub const EMPTY: CustomVariableConfig = CustomVariableConfig::SignedInteger { initial: -1 };

pub fn active_leg_input_feature() -> InputFeature {
    InputFeature::Custom {
        name: "active_leg".to_string(),
        unit: "signed_integer".to_string(),
    }
}

pub fn active_leg_variable_config() -> StateVariableConfig {
    StateVariableConfig::Custom {
        custom_type: "ActiveLeg".to_string(),
        value: EMPTY,
        accumulator: true,
    }
}

pub fn leg_mode_input_feature(leg_idx: LegIdx) -> InputFeature {
    InputFeature::Custom {
        name: fieldname::leg_mode_fieldname(leg_idx),
        unit: "signed_integer".to_string(),
    }
}

/// creates configuration for mode state variables
pub fn leg_mode_variable_config() -> StateVariableConfig {
    StateVariableConfig::Custom {
        custom_type: "Mode".to_string(),
        value: EMPTY,
        accumulator: true,
    }
}

/// creates configuration for distance state variables
pub fn multimodal_distance_variable_config(
    output_unit: Option<DistanceUnit>,
) -> StateVariableConfig {
    StateVariableConfig::Distance {
        initial: Length::ZERO,
        accumulator: true,
        output_unit,
    }
}

/// creates configuration for time state variables
pub fn multimodal_time_variable_config(output_unit: Option<TimeUnit>) -> StateVariableConfig {
    StateVariableConfig::Time {
        initial: Time::ZERO,
        accumulator: true,
        output_unit,
    }
}

/// creates configuration for route_id state variables
pub fn route_id_input_feature() -> InputFeature {
    InputFeature::Custom {
        name: "route_id".to_string(),
        unit: "signed_integer".to_string(),
    }
}

/// creates configuration for route_id state variables
pub fn route_id_variable_config() -> StateVariableConfig {
    StateVariableConfig::Custom {
        custom_type: "RouteId".to_string(),
        value: EMPTY,
        accumulator: true,
    }
}
