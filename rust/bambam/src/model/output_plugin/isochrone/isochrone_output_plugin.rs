use super::destination_point_generator::DestinationPointGenerator;
use super::isochrone_algorithm::IsochroneAlgorithm;
use bambam_core::model::output_plugin::isochrone::IsochroneOutputFormat;
use bambam_core::model::{bambam_field as field, bambam_ops, TimeBin};
use routee_compass::app::{compass::CompassAppError, search::SearchAppResult};
use routee_compass::plugin::output::OutputPlugin;
use routee_compass::plugin::output::OutputPluginError;
use routee_compass_core::algorithm::search::SearchInstance;
use serde_json::json;
use serde_json::Value;

pub struct IsochroneOutputPlugin {
    time_bins: Vec<TimeBin>,
    isochrone_algorithm: IsochroneAlgorithm,
    isochrone_output_format: IsochroneOutputFormat,
    destination_point_generator: DestinationPointGenerator,
}

impl OutputPlugin for IsochroneOutputPlugin {
    /// generates isochrones from this search result.
    /// appends the following structure to the output (assuming bins==(10,20,30,40)):
    ///
    /// {
    ///   "bin": {
    ///     "10": {
    ///       "info": { "time_bin": { .. } },
    ///       "isochrone": {},
    ///     },
    ///     "20": { ... },
    ///     "30": { ... },
    ///     "40": { ... }
    ///   }
    /// }
    fn process(
        &self,
        output: &mut serde_json::Value,
        result: &Result<(SearchAppResult, SearchInstance), CompassAppError>,
    ) -> Result<(), OutputPluginError> {
        output[field::ISOCHRONE_FORMAT] = json![self.isochrone_output_format];
        for time_bin in &self.time_bins {
            // set up this time bin JSON object
            field::scaffold_time_bin(output, time_bin)
                .map_err(OutputPluginError::OutputPluginFailed)?;

            let (isochrone, tree_size) = match result {
                Err(_) => {
                    let empty = self.isochrone_output_format.empty_geometry()?;
                    (json![empty], 0)
                }
                Ok((search_result, si)) => get_isochrone(time_bin, search_result, si, self)?,
            };

            // write result to row
            let time_bin_key = time_bin.key();
            field::insert_nested(
                output,
                &[field::TIME_BINS, &time_bin_key],
                field::ISOCHRONE,
                json!(isochrone),
                true,
            )
            .map_err(OutputPluginError::OutputPluginFailed)?;
            field::insert_nested(
                output,
                &[field::TIME_BINS, &time_bin_key, field::INFO],
                field::TREE_SIZE,
                json!(tree_size),
                true,
            )
            .map_err(OutputPluginError::OutputPluginFailed)?;
        }
        Ok(())
    }
}

impl IsochroneOutputPlugin {
    pub fn new(
        time_bins: Vec<TimeBin>,
        isochrone_algorithm: IsochroneAlgorithm,
        isochrone_output_format: IsochroneOutputFormat,
        destination_point_generator: DestinationPointGenerator,
    ) -> Result<IsochroneOutputPlugin, OutputPluginError> {
        Ok(IsochroneOutputPlugin {
            time_bins,
            isochrone_algorithm,
            isochrone_output_format,
            destination_point_generator,
        })
    }
}

/// collect destinations for this time bin but starting from zero
fn get_isochrone(
    time_bin: &TimeBin,
    search_result: &SearchAppResult,
    si: &SearchInstance,
    plugin: &IsochroneOutputPlugin,
) -> Result<(Value, usize), OutputPluginError> {
    let isochrone_time_bin = TimeBin {
        min_time: 0,
        max_time: time_bin.max_time,
    };
    let tree_destinations: Vec<_> =
        bambam_ops::collect_destinations(search_result, Some(&isochrone_time_bin), &si.state_model)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                OutputPluginError::OutputPluginFailed(format!(
                    "failure collecting destinations: {e}"
                ))
            })?;
    let tree_size = tree_destinations.len();

    // draw isochrone and serialize result
    let tree_mp = plugin
        .destination_point_generator
        .generate_destination_points(&tree_destinations, si.map_model.clone())?;
    let geometry = plugin.isochrone_algorithm.run(tree_mp)?;
    let isochrone = plugin
        .isochrone_output_format
        .serialize_geometry(&geometry)?;
    Ok((json![isochrone], tree_size))
}
