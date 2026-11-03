use std::path::Path;

use bambam_network::CommonNetworkImportArgs;
use clap::{Args, Parser, Subcommand};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::collection::OvertureMapsCollectionError;

/// Command line tool for batch downloading and summarizing of OMF (Overture Maps Foundation) data
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct OmfApp {
    #[command(subcommand)]
    pub op: OmfOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize, Subcommand, JsonSchema)]
pub enum OmfOperation {
    /// download all of the OMF transportation data
    Network(OmfNetworkArgs),
}

#[derive(Debug, Clone, Serialize, Deserialize, Args, JsonSchema)]
pub struct OmfNetworkArgs {
    #[command(flatten)]
    #[serde(flatten)]
    pub common: CommonNetworkImportArgs,
}

impl OmfNetworkArgs {
    pub fn run(&self) -> Result<(), OvertureMapsCollectionError> {
        if self.common.local_source.is_none() && self.common.extent_file.is_none() {
            return Err(OvertureMapsCollectionError::InvalidUserInput(
                "must provide --extent-file when --local-source is not set".to_string(),
            ));
        }
        let filter_cfg = self.common.load_filter_config()?;
        let outdir = self.common.resolved_output_directory();
        let local = self.common.local_source.as_ref().map(Path::new);
        let extent_and_bbox = self.common.load_extent_and_bbox()?;
        let (extent, bbox) = match extent_and_bbox {
            Some((geom, bb)) => (Some(geom), Some(bb)),
            None => (None, None),
        };
        let match_distance = self.common.resolved_match_distance(&filter_cfg);

        crate::app::network::run(
            bbox.as_ref(),
            &filter_cfg.edge_lists,
            outdir,
            local,
            filter_cfg.island_algorithm_configuration,
            extent,
            match_distance,
        )
    }
}

impl OmfOperation {
    pub fn run(&self) -> Result<(), OvertureMapsCollectionError> {
        match self {
            OmfOperation::Network(args) => args.run(),
        }
    }
}
