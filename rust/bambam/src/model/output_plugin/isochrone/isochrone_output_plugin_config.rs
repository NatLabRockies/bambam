use crate::model::output_plugin::isochrone::time_bin_type::TimeBinType;
use bambam_core::model::output_plugin::isochrone::{
    GeometryModelConfig, IsochroneAlgorithm, IsochroneOutputFormat,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IsochroneOutputPluginConfig {
    pub time_bin: TimeBinType,
    pub isochrone_algorithm: IsochroneAlgorithm,
    pub isochrone_output_format: IsochroneOutputFormat,
    pub destination_point_generator: GeometryModelConfig,
}
