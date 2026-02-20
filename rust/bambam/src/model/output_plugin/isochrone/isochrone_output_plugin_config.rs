use crate::model::output_plugin::isochrone::{
    isochrone_algorithm::IsochroneAlgorithm, time_bin_type::TimeBinType,
    DestinationPointGeneratorConfig,
};
use bambam_core::model::output_plugin::isochrone::IsochroneOutputFormat;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IsochroneOutputPluginConfig {
    pub time_bin: TimeBinType,
    pub isochrone_algorithm: IsochroneAlgorithm,
    pub isochrone_output_format: IsochroneOutputFormat,
    pub destination_point_generator: DestinationPointGeneratorConfig,
}
