use crate::model::{
    destination::{BinRangeConfig, DestinationPredicate},
    output_plugin::{
        isochrone::{GeometryModelConfig, IsochroneAlgorithm, IsochroneOutputFormat},
        opportunity::OpportunityOrientation,
    },
};
use serde::{Deserialize, Serialize};

///
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum BambamOutputConfig {
    Aggregate {
        /// the method for binning the output. required when opportunity_format == Aggregate.
        binning: BinRangeConfig,
        /// any additional filters to apply when selecting destinations. optional for both
        /// opportunity_formats.
        destination_filter: Option<Vec<DestinationPredicate>>,
        /// algorithm for assigning physical destination locations from a search tree branch.
        /// used in the isochrone drawing procedure.
        geometry_model: GeometryModelConfig,
        /// algorithm used to draw isochrones from the destination points.
        isochrone_algorithm: IsochroneAlgorithm,
        /// geometry format to use when writing isochrones.
        isochrone_format: IsochroneOutputFormat,
        /// location along a roadway where the opportunity is map matched. by default,
        /// assign opportunities at the destination vertex of an edge.
        #[serde(default)]
        opportunity_orientation: OpportunityOrientation,
    },
    Disaggregate {
        /// any additional filters to apply when selecting destinations. optional for both
        /// opportunity_formats.
        destination_filter: Option<Vec<DestinationPredicate>>,
        /// location along a roadway where the opportunity is map matched. by default,
        /// assign opportunities at the destination vertex of an edge.
        #[serde(default)]
        opportunity_orientation: OpportunityOrientation,
    },
}
