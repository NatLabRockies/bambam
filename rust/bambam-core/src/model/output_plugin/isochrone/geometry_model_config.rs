use routee_compass_core::model::unit::DistanceUnit;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum GeometryModelConfig {
    DestinationPoint,
    LinestringCoordinates,
    LinestringStride {
        stride: f64,
        distance_unit: DistanceUnit,
    },
    BufferedLinestring {
        buffer_radius: f64,
        buffer_stride: f64,
        distance_unit: DistanceUnit,
    },
    BufferedDestinationPoint {
        buffer_radius: f64,
        buffer_stride: f64,
        distance_unit: DistanceUnit,
    },
}
