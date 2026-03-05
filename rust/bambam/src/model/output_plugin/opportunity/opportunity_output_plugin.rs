use std::collections::HashMap;

use super::opportunity_model::OpportunityModel;
use super::OpportunityPluginConfig;
use bambam_core::model::bambam_typed::{self, BambamOutputRow};
use bambam_core::model::destination::{self, DestinationFilter};
use bambam_core::model::output_plugin::opportunity::{opportunity_ops, OpportunityFormat};
use routee_compass::app::{compass::CompassAppError, search::SearchAppResult};
use routee_compass::plugin::output::OutputPlugin;
use routee_compass::plugin::output::OutputPluginError;
use routee_compass_core::algorithm::search::SearchInstance;
use routee_compass_core::util::duration_extension::DurationExtension;
use std::time::{Duration, Instant};

/// RouteE Compass output plugin that appends opportunities to a search result row.
/// uses the loaded [`OpportunityModel`] to look up points-of-interest and
/// appends these results either aggregated or disaggregate, based on the chosen
/// [`OpportunityFormat`]. this is run for each expected bin in the search row.
pub struct OpportunityOutputPlugin {
    pub model: OpportunityModel,
    pub totals: HashMap<String, f64>,
}

impl OutputPlugin for OpportunityOutputPlugin {
    /// tags a result with opportunity counts
    fn process(
        &self,
        output: &mut serde_json::Value,
        result: &Result<(SearchAppResult, SearchInstance), CompassAppError>,
    ) -> Result<(), OutputPluginError> {
        let start_time = Instant::now();
        let (app_result, si) = match result {
            Ok((r, si)) => (r, si),
            Err(_) => {
                let mut row = BambamOutputRow::new(output);
                let mut info = row.info_mut()?;
                info.set_opportunity_runtime(Duration::ZERO.hhmmss())?;
                return Ok(());
            }
        };

        // grab parameters for this run from the row
        let mut row = BambamOutputRow::new(output);
        let info = row.info_ref()?;
        let format = info.get_opportunity_format()?
            .ok_or_else(|| {
                let msg = String::from("opportunity plugin called on row with no opportunity_format set. the 'bambam' plugin should always run before this plugin.");
                OutputPluginError::OutputPluginFailed(msg)
            })?;

        let mut info = row.info_mut()?;

        // set globals on row
        info.set_activity_types(&self.model.activity_types())?;
        row.set_opportunity_totals(&self.totals)?;

        // read destination filter from the row info
        let filter = row
            .info_ref()?
            .get_destination_filter()?
            .map(DestinationFilter);

        match format {
            OpportunityFormat::Aggregate => {
                process_aggregate_opportunities(&mut row, app_result, si, self, filter.as_ref())?;
            }
            OpportunityFormat::Disaggregate => {
                process_disaggregate_opportunities(
                    &mut row,
                    app_result,
                    si,
                    self,
                    filter.as_ref(),
                )?;
            }
        }

        // write the plugin runtime
        let dur = Instant::now().duration_since(start_time);
        {
            let mut info = row.info_mut()?;
            info.set_opportunity_runtime(dur.hhmmss())?;
        }
        Ok(())
    }
}

impl TryFrom<&OpportunityPluginConfig> for OpportunityOutputPlugin {
    type Error = OutputPluginError;

    fn try_from(value: &OpportunityPluginConfig) -> Result<Self, Self::Error> {
        let model = value.model.build()?;
        let totals = model.opportunity_totals().map_err(|e| {
            OutputPluginError::BuildFailed(format!("failed to collect opportunity totals: {e}"))
        })?;
        for (act, total) in totals.iter() {
            if total == &0.0 {
                return Err(OutputPluginError::BuildFailed(format!(
                    "opportunity totals for activity type {act} are zero, which is invalid"
                )));
            }
        }
        let plugin = OpportunityOutputPlugin { model, totals };
        Ok(plugin)
    }
}

fn process_disaggregate_opportunities(
    row: &mut BambamOutputRow<'_>,
    result: &SearchAppResult,
    instance: &SearchInstance,
    plugin: &OpportunityOutputPlugin,
    filter: Option<&DestinationFilter>,
) -> Result<(), OutputPluginError> {
    let destinations_iter =
        destination::iter::new_destinations_iterator(result, None, filter, &instance.state_model);
    let opportunities = plugin
        .model
        .collect_trip_opportunities(destinations_iter, instance)?;
    let opps =
        opportunity_ops::collect_disaggregate(&opportunities, &plugin.model.activity_types())?;
    let mut dis = row.disaggregate()?;
    dis.set_opportunities(&opps)?;
    Ok(())
}

/// for aggregate opportunity formats, we collect all opportunities within each bin
/// and bundle them together into a single output row.
fn process_aggregate_opportunities(
    row: &mut BambamOutputRow<'_>,
    result: &SearchAppResult,
    instance: &SearchInstance,
    plugin: &OpportunityOutputPlugin,
    filter: Option<&DestinationFilter>,
) -> Result<(), OutputPluginError> {
    // expect bin configuration for aggregate format
    let bin_config = row.info_ref()?.get_bin_range()?.ok_or_else(|| {
        OutputPluginError::OutputPluginFailed(
            "row with aggregate opportunities has no bin range config".to_string(),
        )
    })?;

    let mut agg = row.aggregate()?;
    for bin in bin_config.build_bins().into_iter() {
        let start_time = Instant::now();
        let bin_key = bin.bin_key();

        // collect all opportunities from destinations within this bin
        let destinations_iter = destination::iter::new_destinations_iterator(
            result,
            Some(&bin),
            filter,
            &instance.state_model,
        );

        // collect aggregated opportunities and write to output
        let destination_opportunities = plugin
            .model
            .collect_trip_opportunities(destinations_iter, instance)?;
        let opps = opportunity_ops::collect_aggregate(
            &destination_opportunities,
            &plugin.model.activity_types(),
        )?;
        agg.set_opportunities(&bin_key, &opps)?;

        // write bin-level runtime
        let runtime = Instant::now().duration_since(start_time);
        agg.set_bin_runtime(&bin_key, runtime.hhmmss())?;
    }
    Ok(())
}
