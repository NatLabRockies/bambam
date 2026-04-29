use std::collections::HashMap;

use bambam_core::model::bambam_typed::BambamOutputRow;
use bambam_core::model::destination::{self, BinInterval, DestinationFilter, DestinationPredicate};
use bambam_core::model::output_plugin::isochrone::{
    GeometryModel, IsochroneAlgorithm, IsochroneOutputFormat,
};
use bambam_core::model::output_plugin::opportunity::OpportunityFormat;
use bambam_core::model::{bambam_field as field, bambam_ops, bambam_typed, TimeBin};
use routee_compass::app::{compass::CompassAppError, search::SearchAppResult};
use routee_compass::plugin::output::OutputPlugin;
use routee_compass::plugin::output::OutputPluginError;
use routee_compass_core::algorithm::search::{SearchInstance, SearchResult};
use serde_json::json;
use serde_json::Value;

pub struct IsochroneOutputPlugin {}

impl OutputPlugin for IsochroneOutputPlugin {
    fn process(
        &self,
        output: &mut serde_json::Value,
        result: &Result<(SearchAppResult, SearchInstance), CompassAppError>,
    ) -> Result<(), OutputPluginError> {
        let (sr, si) = match result {
            Ok(r) => r,
            Err(_) => return Ok(()),
        };
        let mut row = bambam_typed::BambamOutputRow::new(output);
        run_isochrone(row, sr, si)
    }
}

/// generate isochrones for this row of data.
pub fn run_isochrone(
    mut row: BambamOutputRow<'_>,
    sr: &SearchAppResult,
    si: &SearchInstance,
) -> Result<(), OutputPluginError> {
    // only run this plugin for rows requesting Aggregate opportunities
    let info = row.info_ref()?;
    let format = info.get_opportunity_format()?;
    let requires_isochrones = matches!(format, Some(OpportunityFormat::Aggregate));
    if !requires_isochrones {
        return Ok(());
    }

    let get_isochrone_request = GetIsochroneRequest::try_from(&row)?;

    // expect bin configuration if Aggregate
    let bin_config = match info.get_bin_range()? {
        Some(bc) => bc,
        None => {
            let msg = String::from("row with aggregate opportunities has no bin range config");
            return Err(OutputPluginError::OutputPluginFailed(msg));
        }
    };

    let mut agg = row.aggregate()?;
    let bins = bin_config
        .build_bins(false)
        .map_err(|e| OutputPluginError::OutputPluginFailed(e.to_string()))?;
    for bin in bins.into_iter() {
        let bin_key = bin.bin_key();
        let result = get_isochrone_request.run(&bin, sr, si)?;
        agg.set_isochrone(&bin_key, result.isochrone_value);
        agg.set_n_destinations(&bin_key, result.tree_size);
    }

    Ok(())
}

struct GetIsochroneRequest {
    filter: Option<DestinationFilter>,
    geometry_model: GeometryModel,
    isochrone_algorithm: IsochroneAlgorithm,
    isochrone_format: IsochroneOutputFormat,
}

impl<'a> TryFrom<&'a BambamOutputRow<'a>> for GetIsochroneRequest {
    type Error = OutputPluginError;

    fn try_from(value: &'a BambamOutputRow<'a>) -> Result<Self, Self::Error> {
        let info = value.info_ref()?;
        let format = info.get_opportunity_format()?;
        let filter = info.get_destination_filter()?.map(DestinationFilter);
        let geometry_model_config = info
            .get_geometry_model()?
            .ok_or_else(|| missing_expected("info.geometry_model"))?;
        let geometry_model = GeometryModel::try_from(&geometry_model_config)?;
        let isochrone_algorithm = info
            .get_isochrone_algorithm()?
            .ok_or_else(|| missing_expected("info.isochrone_algorithm"))?;
        let isochrone_format = info
            .get_isochrone_format()?
            .ok_or_else(|| missing_expected("info.isochrone_format"))?;
        Ok(Self {
            filter,
            geometry_model,
            isochrone_algorithm,
            isochrone_format,
        })
    }
}

impl GetIsochroneRequest {
    pub fn run(
        &self,
        bin: &BinInterval,
        search_result: &SearchAppResult,
        si: &SearchInstance,
    ) -> Result<GetIsochroneResult, OutputPluginError> {
        let tree_destinations: Vec<_> = destination::iter::new_destinations_iterator(
            search_result,
            Some(bin),
            self.filter.as_ref(),
            &si.state_model,
        )
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            OutputPluginError::OutputPluginFailed(format!("failure collecting destinations: {e}"))
        })?;
        let tree_size = tree_destinations.len();

        // draw isochrone and serialize result
        let tree_mp = self
            .geometry_model
            .generate_destination_points(&tree_destinations, si.map_model.clone())?;
        let geometry = self.isochrone_algorithm.run(tree_mp)?;
        let isochrone = self.isochrone_format.serialize_geometry(&geometry)?;
        let result = GetIsochroneResult {
            isochrone_value: json![isochrone],
            tree_size,
        };
        Ok(result)
    }
}

struct GetIsochroneResult {
    isochrone_value: Value,
    tree_size: usize,
}

/// helper for building a missing field error
fn missing_expected(field: &str) -> OutputPluginError {
    let msg = format!("output row missing expected field '{field}'");
    OutputPluginError::OutputPluginFailed(msg)
}
