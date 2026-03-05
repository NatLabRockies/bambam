use bambam_core::model::{
    bambam_typed,
    output_plugin::{opportunity::OpportunityFormat, BambamOutputConfig},
};
use routee_compass::{
    app::{compass::CompassAppError, search::SearchAppResult},
    plugin::output::{OutputPlugin, OutputPluginError},
};
use routee_compass_core::algorithm::search::SearchInstance;

/// scaffolds the output row with fields that are parameters to the downstream
/// BAMBAM output plugins.
pub struct BambamOutputPlugin(pub BambamOutputConfig);

impl OutputPlugin for BambamOutputPlugin {
    fn process(
        &self,
        output: &mut serde_json::Value,
        result: &Result<(SearchAppResult, SearchInstance), CompassAppError>,
    ) -> Result<(), OutputPluginError> {
        let mut row = bambam_typed::BambamOutputRow::new(output);
        let mut info = row.info_mut()?;

        match &self.0 {
            BambamOutputConfig::Aggregate {
                binning,
                destination_filter,
                geometry_model,
                isochrone_algorithm,
                isochrone_format,
                opportunity_orientation,
            } => {
                info.set_opportunity_format(OpportunityFormat::Aggregate)?;
                info.set_opportunity_orientation(*opportunity_orientation)?;
                info.set_bin_range(binning)?;
                if let Some(f) = destination_filter {
                    info.set_destination_filter(f)?;
                }
                info.set_geometry_model(geometry_model)?;
                info.set_isochrone_algorithm(isochrone_algorithm)?;
                info.set_isochrone_format(isochrone_format)?;
            }
            BambamOutputConfig::Disaggregate {
                destination_filter,
                opportunity_orientation,
            } => {
                info.set_opportunity_format(OpportunityFormat::Disaggregate)?;
                info.set_opportunity_orientation(*opportunity_orientation)?;
                if let Some(f) = destination_filter {
                    info.set_destination_filter(f)?;
                }
            }
        }
        Ok(())
    }
}
