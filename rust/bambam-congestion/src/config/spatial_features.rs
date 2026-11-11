use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::CensusSourceConfig;

/// Coordinate projection mode for 0.5-mile buffer distance and area calculations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionMode {
    /// True geodesic meters (local tangent plane centered on each edge, area = $\pi R^2 + 2RL$).
    #[default]
    Geodesic,
    /// Legacy Web Mercator (`EPSG:3857`) planar units for exact parity with legacy Python scripts.
    LegacyEpsg3857,
}

/// Configuration for the 0.5-mile spatial feature engine (POIs, traffic controls, and Census densities).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SpatialFeaturesConfig {
    /// Buffer radius in miles around each edge (defaults to `0.5` miles = `804.672` meters).
    #[serde(default = "default_density_radius_miles")]
    pub density_radius_miles: f64,
    /// Coordinate projection mode (`geodesic` or `legacy_epsg3857`).
    #[serde(default)]
    pub projection_mode: ProjectionMode,
    /// Optional source for MEP land-use opportunity points (`overture_maps`, `osm_pbf`, `csv_points`, or `vertex_opportunities_csv`).
    /// Note: can be mixed and matched freely with any Open Network source (e.g. OMF road network + OSM PBF opportunities, or vice versa).
    #[serde(default)]
    pub opportunities_source: Option<OpportunitiesSourceConfig>,
    /// Optional override source for traffic controls (`traffic_signals`, `stop`, `crossing`).
    /// When `None`, traffic controls are automatically loaded from `<open_network>/vertices-complete.csv.gz`
    /// (which is produced by both `bambam-omf` and `bambam-osm`).
    #[serde(default)]
    pub traffic_controls_source: Option<TrafficControlsSourceConfig>,
    /// Optional source for Census population and employment density polygons.
    #[serde(default)]
    pub census_source: Option<CensusSourceConfig>,
}

impl Default for SpatialFeaturesConfig {
    fn default() -> Self {
        Self {
            density_radius_miles: default_density_radius_miles(),
            projection_mode: ProjectionMode::Geodesic,
            opportunities_source: None,
            traffic_controls_source: None,
            census_source: None,
        }
    }
}

/// Data source for MEP land-use opportunities (`Opp Density`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpportunitiesSourceConfig {
    /// Collect MEP land-use opportunities (`places` and `buildings`) directly from Overture Maps
    /// over the network's bounding box (via AWS S3 or a local OMF Parquet directory).
    OvertureMaps {
        /// Optional local directory containing Overture `places` / `buildings` Parquet files;
        /// when `None`, fetches directly from the public Overture Maps S3 bucket for the network's bbox.
        #[serde(default)]
        local_source_dir: Option<String>,
        /// Whether to also collect Overture `buildings` in addition to `places` (defaults to `true`).
        #[serde(default = "default_true")]
        include_buildings: bool,
    },
    /// Extract MEP land-use opportunities (and optionally traffic controls) directly from a local `.osm.pbf` file.
    OsmPbf { pbf_path: String },
    /// Read point opportunities from a CSV or `.csv.gz` file containing `x`/`y` (lon/lat) or WKT `geometry`
    /// and an optional count column.
    CsvPoints {
        path: String,
        #[serde(default = "default_lon_col")]
        x_column: String,
        #[serde(default = "default_lat_col")]
        y_column: String,
        #[serde(default)]
        wkt_column: Option<String>,
        #[serde(default)]
        weight_column: Option<String>,
    },
    /// Read vertex-oriented opportunity counts (e.g. BAMBAM `opportunities.csv`) joined to `vertices-compass.csv.gz`.
    VertexOpportunitiesCsv {
        opportunities_csv_path: String,
        /// Optional list of columns to sum as MEP opportunities; if `None`, sums all numeric columns.
        #[serde(default)]
        columns: Option<Vec<String>>,
    },
}

/// Data source for traffic control points (`Traffic Signal/Crossing/Traffic Stop Density`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TrafficControlsSourceConfig {
    /// Use `<open_network>/vertices-complete.csv.gz` (`traffic_signals`, `stop`, `crossing`).
    VerticesComplete {
        #[serde(default)]
        path: Option<String>,
    },
    /// Extract traffic control nodes directly from a local `.osm.pbf` file.
    OsmPbf { pbf_path: String },
    /// Read traffic control points from a CSV file with `x`, `y` (lon/lat) and optional `weight` column.
    CsvPoints {
        path: String,
        #[serde(default = "default_lon_col")]
        x_column: String,
        #[serde(default = "default_lat_col")]
        y_column: String,
    },
}

fn default_density_radius_miles() -> f64 {
    0.5
}

fn default_true() -> bool {
    true
}

fn default_lon_col() -> String {
    "x".to_string()
}

fn default_lat_col() -> String {
    "y".to_string()
}
