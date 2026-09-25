use std::{fs, path::Path};

use clap::{Args, Parser, Subcommand};
use config::{Config, File};
use geo::{BoundingRect, MapCoords};
use geozero::{wkt::Wkt as WktReader, ToGeo};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    app::{network::NetworkEdgeListConfiguration, CliBoundingBox},
    collection::OvertureMapsCollectionError,
    graph::island_detection::IslandDetectionAlgorithm,
};

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
    /// descriptive user-provided name for this import region.
    #[arg(short, long)]
    pub name: String,

    /// configuration file defining how the network is imported and separated
    /// into mode-specific edge lists.
    #[arg(short, long)]
    pub configuration_file: String,

    /// location on disk to write output files. if not provided,
    /// use the current working directory.
    #[arg(short, long)]
    pub output_directory: Option<String>,

    /// use a stored raw data export from a previous run of OmfOperation::Network
    /// which is a JSON file containing a TransportationCollection.
    #[arg(short, long)]
    pub local_source: Option<String>,

    /// write the raw OMF dataset as a JSON blob to the output directory.
    #[arg(short, long)]
    #[serde(default)]
    pub store_raw: bool,

    /// write the list of segment and connector IDs for each edge created
    #[arg(long)]
    #[serde(default)]
    pub omf_ids: bool,

    /// Optional WKT extent in json format. expects a json file with a single "extent" key
    #[arg(short, long)]
    pub extent_file: Option<String>,
}

impl OmfNetworkArgs {
    pub fn run(&self) -> Result<(), OvertureMapsCollectionError> {
        let filepath = Path::new(&self.configuration_file);
        let config = Config::builder()
            .add_source(File::from(filepath))
            .build()
            .map_err(|e| {
                let msg = format!("file '{}' produced error: {e}", self.configuration_file);
                OvertureMapsCollectionError::InvalidUserInput(msg)
            })?;
        let network_config = config
            .get::<Vec<NetworkEdgeListConfiguration>>("edge_lists")
            .map_err(|e| {
                let msg = format!(
                    "error reading 'edge_lists' key in '{}': {e}",
                    self.configuration_file
                );
                OvertureMapsCollectionError::InvalidUserInput(msg)
            })?;
        let island_algorithm_configuration = config
            .get::<Option<IslandDetectionAlgorithm>>("island_algorithm_configuration")
            .map_err(|e| {
                let msg = format!(
                    "error reading 'island_algorithm_configuration' key in '{}': {e}",
                    self.configuration_file
                );
                OvertureMapsCollectionError::InvalidUserInput(msg)
            })?;
        let outdir = match &self.output_directory {
            Some(out) => Path::new(out),
            None => Path::new(""),
        };
        let local = self.local_source.as_ref().map(Path::new);
        let extent = self
            .extent_file
            .as_ref()
            .map(|extent_path| {
                let wkt_str = fs::read_to_string(extent_path).map_err(|e| {
                    OvertureMapsCollectionError::InvalidUserInput(format!(
                        "failed to load extent file {extent_path}: {e}"
                    ))
                })?;

                let geometry_f64 = WktReader(wkt_str.trim()).to_geo().map_err(|e| {
                    OvertureMapsCollectionError::InvalidUserInput(format!(
                        "failed to parse string into WKT from {extent_path}: {e}"
                    ))
                })?;
                let polygon = geometry_f64.map_coords(|geo::Coord { x, y }| geo::Coord {
                    x: x as f32,
                    y: y as f32,
                });

                Ok(polygon)
            })
            .transpose()?;
        let bbox = match &extent {
            Some(geom) => match geom.bounding_rect() {
                Some(rect) => CliBoundingBox::from_coords(&rect.min(), &rect.max())
                    .map_err(OvertureMapsCollectionError::InvalidGeometry)
                    .map(Some),
                None => {
                    let msg = "provided extent does not have a bbox (maybe empty?)".to_string();
                    Err(OvertureMapsCollectionError::InvalidGeometry(msg))
                }
            },
            None => Ok(None),
        }?;

        crate::app::network::run(
            &self.name,
            bbox.as_ref(),
            &network_config,
            outdir,
            local,
            self.store_raw,
            island_algorithm_configuration,
            self.omf_ids,
            extent,
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
