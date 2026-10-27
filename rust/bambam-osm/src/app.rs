use bambam_network::CommonNetworkImportArgs;
use clap::{Args, Parser, Subcommand};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    config::OsmImportConfiguration,
    model::{
        osm::{graph::CompassWriter, OsmSource},
        OsmCliError,
    },
};

/// Command line tool for importing OpenStreetMap (`.osm.pbf`) data into RouteE Compass format.
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct OsmApp {
    #[command(subcommand)]
    pub op: OsmOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize, Subcommand, JsonSchema)]
pub enum OsmOperation {
    /// Import an OpenStreetMap road network into RouteE Compass format.
    #[command(alias = "pbf")]
    Network(OsmNetworkArgs),
}

#[derive(Debug, Clone, Serialize, Deserialize, Args, JsonSchema)]
pub struct OsmNetworkArgs {
    /// Path to `.osm.pbf` file for import (required unless `--local-source` is provided).
    #[arg(short, long)]
    #[serde(default)]
    pub pbf_file: Option<String>,

    /// Shared network import arguments (`--name`, `--configuration-file`, `--output-directory`,
    /// `--local-source`, `--extent-file`, `--infrastructure-match-distance-meters`).
    #[command(flatten)]
    #[serde(flatten)]
    pub common: CommonNetworkImportArgs,
}

impl OsmNetworkArgs {
    pub fn run(&self) -> Result<(), OsmCliError> {
        let conf = OsmImportConfiguration::try_from(&self.common.configuration_file)?;
        let out_path = self.common.resolved_output_directory();
        let edge_lists = conf.resolved_edge_lists();
        let island_alg = conf.resolved_island_detection();
        let element_filter = conf.resolved_element_filter();
        let match_distance = self
            .common
            .infrastructure_match_distance_meters
            .or(conf.infrastructure_match_distance_meters);

        let osm_source = if let Some(ref local_src) = self.common.local_source {
            OsmSource::LocalParquet {
                local_source: local_src.clone(),
                study_region: Some(self.common.name.clone()),
                edge_lists: Some(edge_lists),
                island_algorithm_configuration: island_alg,
                extent_filter_filepath: self.common.extent_file.clone(),
                component_filter: Some(conf.component_filter),
                truncate_by_edge: conf.truncate_by_edge,
                ignore_errors: conf.ignore_osm_parsing_errors,
                simplify: conf.simplify,
                infrastructure_match_distance_meters: match_distance,
            }
        } else if let Some(ref pbf_path) = self.pbf_file {
            OsmSource::Pbf {
                pbf_filepath: pbf_path.clone(),
                study_region: Some(self.common.name.clone()),
                extent_filter_filepath: self.common.extent_file.clone(),
                network_filter: Some(element_filter),
                edge_lists: Some(edge_lists),
                island_algorithm_configuration: island_alg,
                component_filter: Some(conf.component_filter),
                truncate_by_edge: conf.truncate_by_edge,
                ignore_errors: conf.ignore_osm_parsing_errors,
                simplify: conf.simplify,
                infrastructure_match_distance_meters: match_distance,
            }
        } else {
            return Err(OsmCliError::ConfigurationError(
                "must provide either --pbf-file or --local-source".to_string(),
            ));
        };

        let graph = osm_source.import()?;
        graph.write_compass(out_path, conf.overwrite)?;
        Ok(())
    }
}

impl OsmOperation {
    pub fn run(&self) -> Result<(), OsmCliError> {
        match self {
            OsmOperation::Network(args) => args.run(),
        }
    }
}
