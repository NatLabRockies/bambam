use chrono::NaiveDateTime;
use clap::Args;

use crate::app::gtfs_flex_config::{
    GraphConfigType, GtfsFlexConfigError, GtfsFlexConfigType, MapConfigType,
};

#[derive(Args, Clone, Debug)]
pub struct CliGtfsFlexConfigApp {
    /// path to the config file we are adding GTFS-Flex to.
    #[arg(long)]
    base_file: String,
    /// path where the result should be written, a file.
    #[arg(long)]
    out_file: String,
    /// file containing processed GTFS-Flex data, generated via the bambam_gtfs_flex CLI.
    #[arg(long)]
    flex_directory: String,

    /// file containing the travel mode configuration for gtfs-flex, added to the grid_search
    /// plugin value.
    #[arg(long)]
    flex_mode_config: Option<String>,

    /// start time argument for the gtfs-flex trip. some zones are defined for a specific
    /// date and time which must be explicitly set by the user. should be provided in
    /// '%Y-%m-%dT%H:%M:%S' format.
    #[arg(long)]
    start_time: String,

    #[command(flatten)]
    graph: CliGraphConfig,

    #[command(flatten)]
    map: CliMappingConfig,

    #[command(flatten)]
    flex: CliGtfsFlexConfig,

    /// if true, allow overwriting the write file location.
    #[arg(long)]
    overwrite: bool,
}

#[derive(Args, Debug, Clone)]
#[group(required = true, multiple = false)]
pub struct CliGraphConfig {
    /// source of the graph topology for the network used by GTFS-Flex,
    /// an edge_list_input_file at the given index in the base config.
    #[arg(long)]
    pub graph_edge_list: Option<usize>,
    /// source of the graph topology for the network used by GTFS-Flex,
    /// a file in the file system.
    #[arg(long)]
    pub graph_edge_list_input_file: Option<String>,
}

#[derive(Args, Debug, Clone)]
#[group(required = true, multiple = false)]
pub struct CliMappingConfig {
    /// source of the link geometries for the network used by GTFS-Flex,
    /// a geometries_input_file entry at the given index in the base config.
    #[arg(long)]
    pub map_edge_list: Option<usize>,
    /// source of the link geometries for the network used by GTFS-Flex,
    /// a file in the file system.
    #[arg(long)]
    pub map_geometries_input_file: Option<String>,
}

#[derive(Args, Debug, Clone)]
#[group(required = true, multiple = false)]
pub struct CliGtfsFlexConfig {
    /// source of the traversal and constraint configuration for the GTFS-Flex edge list,
    /// a [SearchConfig] at the given index in the base config.
    #[arg(long)]
    pub gtfs_flex_edge_list: Option<usize>,
    /// source of the traversal and constraint configuration for the GTFS-Flex edge list,
    /// a file (JSON|YAML|TOML) in the file system.
    #[arg(long)]
    pub gtfs_flex_search_config_input_file: Option<String>,
}

impl CliGtfsFlexConfigApp {
    pub fn run(self) -> Result<(), GtfsFlexConfigError> {
        let graph_config = GraphConfigType::try_from(self.graph)?;
        let map_config = MapConfigType::try_from(self.map)?;
        let gtfs_config = GtfsFlexConfigType::try_from(self.flex)?;
        let start_time = NaiveDateTime::parse_from_str(&self.start_time, "%Y-%m-%dT%H:%M:%S")
            .map_err(|e| {
                let msg = format!("invalid start_time argument provided, must be in '%Y-%m-%dT%H:%M:%S' format: {e}");
                GtfsFlexConfigError::RunFailure(msg)
            })?;
        crate::app::gtfs_flex_config::run(
            &self.base_file,
            &self.out_file,
            &self.flex_directory,
            self.flex_mode_config.as_ref(),
            start_time,
            graph_config,
            map_config,
            gtfs_config,
            self.overwrite,
        )
    }
}
