pub mod fieldname {
    //! the state variable fieldnames used in GTFS-Flex routing

    pub const TRIP_SRC_ZONE_ID: &str = "trip_src_zone_id";
    pub const EDGE_IS_GTFS_FLEX_DESTINATION: &str = "edge_is_gtfs_flex_destination";
    pub const EDGE_POOLING_DELAY: &str = "edge_pooling_delay";
}

pub mod variable {
    //! the configuration for state variables in GTFS-Flex routing

    use ordered_float::OrderedFloat;
    use routee_compass_core::model::state::{CustomVariableConfig, StateVariableConfig};

    /// stores a zone id in a state variable
    pub fn zone_id() -> StateVariableConfig {
        StateVariableConfig::Custom {
            custom_type: "Option<ZoneId>".to_string(),
            value: empty(),
            accumulator: true,
        }
    }

    pub fn gtfs_flex_destination() -> StateVariableConfig {
        StateVariableConfig::Custom {
            custom_type: "Bool".to_string(),
            value: CustomVariableConfig::Boolean { initial: false },
            accumulator: false,
        }
    }

    pub fn pooling_delay() -> StateVariableConfig {
        StateVariableConfig::Time {
            initial: uom::si::f64::Time::new::<uom::si::time::second>(0.0),
            accumulator: false,
            output_unit: None,
        }
    }

    /// empty value is "-1" for categoricals mapped to real numbers
    pub fn empty() -> CustomVariableConfig {
        CustomVariableConfig::FloatingPoint {
            initial: OrderedFloat(-1.0),
        }
    }
}
