use std::{fs, path::Path};

use clap::Args;
use config::{Config, File};
use geo::{BoundingRect, Coord, Geometry, MapCoords};
use geozero::{wkt::Wkt as WktReader, ToGeo};
use routee_compass_core::model::unit::DistanceUnit;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    error::NetworkImportError,
    island_detection::{ComponentsAlgorithmType, IslandDetectionAlgorithm},
    taxonomy::{CliBoundingBox, NetworkEdgeListConfiguration},
};

/// Shared configuration file structure (`bambam-network-config.json` / `.toml`)
/// defining per-mode edge list filters and island detection across both OMF and OSM.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NetworkImportFilterConfig {
    /// Mode-specific edge list filter configurations (e.g., `walk`, `bike`, `drive`).
    pub edge_lists: Vec<NetworkEdgeListConfiguration>,
    /// Connected-components island detection configuration.
    #[serde(default)]
    pub island_algorithm_configuration: Option<IslandDetectionAlgorithm>,
    /// Optional Haversine distance (in meters) for attaching infrastructure features to vertices.
    #[serde(default)]
    pub infrastructure_match_distance_meters: Option<f64>,
}

impl Default for NetworkImportFilterConfig {
    fn default() -> Self {
        Self {
            edge_lists: NetworkEdgeListConfiguration::default_multimodal(),
            island_algorithm_configuration: Some(IslandDetectionAlgorithm {
                min_distance: 2.0,
                distance_unit: DistanceUnit::Miles,
                algorithm_type: ComponentsAlgorithmType::Wcc,
            }),
            infrastructure_match_distance_meters: None,
        }
    }
}

impl NetworkImportFilterConfig {
    /// Reads a [`NetworkImportFilterConfig`] from a `.json` or `.toml` file using `config::Config`.
    pub fn from_file(filepath: &Path) -> Result<Self, NetworkImportError> {
        let display_path = filepath.display();
        let cfg = Config::builder()
            .add_source(File::from(filepath))
            .build()
            .map_err(|e| {
                NetworkImportError::InvalidUserInput(format!(
                    "file '{display_path}' produced error: {e}"
                ))
            })?;

        let edge_lists = cfg
            .get::<Vec<NetworkEdgeListConfiguration>>("edge_lists")
            .map_err(|e| {
                NetworkImportError::InvalidUserInput(format!(
                    "error reading 'edge_lists' key in '{display_path}': {e}"
                ))
            })?;

        let island_algorithm_configuration = cfg
            .get::<Option<IslandDetectionAlgorithm>>("island_algorithm_configuration")
            .map_err(|e| {
                NetworkImportError::InvalidUserInput(format!(
                    "error reading 'island_algorithm_configuration' key in '{display_path}': {e}"
                ))
            })?;

        let infrastructure_match_distance_meters = cfg
            .get::<Option<f64>>("infrastructure_match_distance_meters")
            .unwrap_or(None);

        Ok(Self {
            edge_lists,
            island_algorithm_configuration,
            infrastructure_match_distance_meters,
        })
    }
}

/// Shared CLI arguments used by both `bambam-omf network` and `bambam-osm network`.
#[derive(Debug, Clone, Serialize, Deserialize, Args, JsonSchema)]
pub struct CommonNetworkImportArgs {
    /// Configuration file (`bambam-network-config.json`) defining how the network is imported
    /// and separated into mode-specific edge lists.
    #[arg(short, long)]
    pub configuration_file: String,

    /// Location on disk to write output files.
    #[arg(short, long)]
    pub output_directory: String,

    /// Use a stored raw data export (`raw/` Parquet directory) from a previous run.
    #[arg(short, long)]
    pub local_source: Option<String>,

    /// Optional WKT extent file path (plain WKT or JSON with an `"extent"` key).
    #[arg(short, long)]
    pub extent_file: Option<String>,

    /// Maximum distance in meters for matching infrastructure features to connector vertices (defaults to 15.0).
    #[arg(long)]
    #[serde(default)]
    pub infrastructure_match_distance_meters: Option<f64>,
}

impl CommonNetworkImportArgs {
    /// Loads the [`NetworkImportFilterConfig`] from `self.configuration_file`.
    pub fn load_filter_config(&self) -> Result<NetworkImportFilterConfig, NetworkImportError> {
        NetworkImportFilterConfig::from_file(Path::new(&self.configuration_file))
    }

    /// Resolves `self.output_directory` as a `Path`.
    pub fn resolved_output_directory(&self) -> &Path {
        Path::new(&self.output_directory)
    }

    /// Resolves the infrastructure match distance from the CLI flag or the configuration file.
    pub fn resolved_match_distance(&self, cfg: &NetworkImportFilterConfig) -> Option<f64> {
        self.infrastructure_match_distance_meters
            .or(cfg.infrastructure_match_distance_meters)
    }

    /// Loads `self.extent_file` (if provided) as a `(Geometry<f32>, CliBoundingBox)`.
    pub fn load_extent_and_bbox(
        &self,
    ) -> Result<Option<(Geometry<f32>, CliBoundingBox)>, NetworkImportError> {
        self.extent_file
            .as_deref()
            .map(read_extent_file_to_geometry_and_bbox)
            .transpose()
    }
}

/// Parses a WKT string (plain WKT or JSON with an `{"extent": "POLYGON(...)"}` wrapper)
/// into a `Geometry<f32>` and its `CliBoundingBox`.
pub fn parse_wkt_str_to_geometry_and_bbox(
    raw: &str,
) -> Result<(Geometry<f32>, CliBoundingBox), NetworkImportError> {
    let trimmed = raw.trim();
    let wkt_str: String = if trimmed.starts_with('{') {
        let json_val: serde_json::Value = serde_json::from_str(trimmed).map_err(|e| {
            NetworkImportError::InvalidUserInput(format!(
                "failed to parse JSON extent wrapper: {e}"
            ))
        })?;
        json_val
            .get("extent")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                NetworkImportError::InvalidUserInput(
                    "JSON extent object missing string key 'extent'".to_string(),
                )
            })?
            .trim()
            .to_string()
    } else {
        trimmed.to_string()
    };

    let geom_f64 = WktReader(wkt_str.as_str()).to_geo().map_err(|e| {
        NetworkImportError::InvalidUserInput(format!("failed to parse string into WKT: {e}"))
    })?;
    let geom_f32: Geometry<f32> = geom_f64.map_coords(|Coord { x, y }| Coord {
        x: x as f32,
        y: y as f32,
    });

    match &geom_f32 {
        Geometry::Polygon(_) | Geometry::MultiPolygon(_) => {}
        other => {
            return Err(NetworkImportError::InvalidGeometry(format!(
                "extent WKT must be a POLYGON or MULTIPOLYGON, got {other:?}"
            )))
        }
    }

    let rect = geom_f32.bounding_rect().ok_or_else(|| {
        NetworkImportError::InvalidGeometry(
            "provided extent does not have a bbox (maybe empty?)".to_string(),
        )
    })?;
    let bbox = CliBoundingBox::from_coords(&rect.min(), &rect.max())
        .map_err(NetworkImportError::InvalidGeometry)?;
    Ok((geom_f32, bbox))
}

/// Reads a WKT file (or JSON extent file) from disk and returns its `Geometry<f32>` and `CliBoundingBox`.
pub fn read_extent_file_to_geometry_and_bbox(
    extent_path: &str,
) -> Result<(Geometry<f32>, CliBoundingBox), NetworkImportError> {
    let raw = fs::read_to_string(extent_path).map_err(|e| {
        NetworkImportError::InvalidUserInput(format!(
            "failed to load extent file {extent_path}: {e}"
        ))
    })?;
    parse_wkt_str_to_geometry_and_bbox(&raw)
}
