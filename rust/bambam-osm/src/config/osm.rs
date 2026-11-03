use crate::model::OsmCliError;
use bambam_network::{
    ComponentsAlgorithmType, IslandDetectionAlgorithm, NetworkEdgeListConfiguration,
};
use routee_compass_core::model::unit::DistanceUnit;
use serde::{Deserialize, Serialize};

/// defines behaviors for an OSM network import.
/// Supports both `bambam-osm` TOML/JSON configs and `bambam-network-config.json`
/// (`edge_lists` + `island_algorithm_configuration`).
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct OsmImportConfiguration {
    #[serde(default)]
    pub edge_lists: Option<Vec<NetworkEdgeListConfiguration>>,
    #[serde(default)]
    pub island_algorithm_configuration: Option<IslandDetectionAlgorithm>,
    #[serde(default)]
    pub ignore_osm_parsing_errors: bool,
    #[serde(default = "default_true")]
    pub truncate_by_edge: bool,
    #[serde(default = "default_true")]
    pub simplify: bool,
    #[serde(default = "default_true")]
    pub overwrite: bool,
    #[serde(default)]
    pub infrastructure_match_distance_meters: Option<f64>,
}

fn default_true() -> bool {
    true
}

impl OsmImportConfiguration {
    /// Returns the configured mode edge lists, or the default multimodal (`walk`, `bike`, `drive`)
    /// configurations if none were specified.
    pub fn resolved_edge_lists(&self) -> Vec<NetworkEdgeListConfiguration> {
        match self.edge_lists.as_ref().filter(|e| !e.is_empty()) {
            Some(lists) => lists.clone(),
            None => NetworkEdgeListConfiguration::default_multimodal(),
        }
    }

    /// Returns the configured [`IslandDetectionAlgorithm`].
    pub fn resolved_island_detection(&self) -> Option<IslandDetectionAlgorithm> {
        self.island_algorithm_configuration.clone()
    }
}

impl Default for OsmImportConfiguration {
    fn default() -> Self {
        Self {
            edge_lists: Some(NetworkEdgeListConfiguration::default_multimodal()),
            island_algorithm_configuration: Some(IslandDetectionAlgorithm {
                min_distance: 2.0,
                distance_unit: DistanceUnit::Miles,
                algorithm_type: ComponentsAlgorithmType::Wcc,
            }),
            ignore_osm_parsing_errors: true,
            truncate_by_edge: true,
            simplify: true,
            overwrite: true,
            infrastructure_match_distance_meters: None,
        }
    }
}

impl TryFrom<&String> for OsmImportConfiguration {
    type Error = OsmCliError;

    fn try_from(f: &String) -> Result<Self, Self::Error> {
        if f.ends_with(".toml") {
            let s = std::fs::read_to_string(f).map_err(|e| {
                OsmCliError::ConfigurationError(format!("failure reading {f}: {e}"))
            })?;
            toml::from_str(&s)
                .map_err(|e| OsmCliError::ConfigurationError(format!("failure decoding {f}: {e}")))
        } else if f.ends_with(".json") {
            let s = std::fs::read_to_string(f).map_err(|e| {
                OsmCliError::ConfigurationError(format!("failure reading {f}: {e}"))
            })?;
            serde_json::from_str(&s)
                .map_err(|e| OsmCliError::ConfigurationError(format!("failure decoding {f}: {e}")))
        } else {
            Err(OsmCliError::ConfigurationError(format!(
                "unsupported file type: {f}"
            )))
        }
    }
}
