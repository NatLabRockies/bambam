use crate::{
    algorithm::truncation::ComponentFilter,
    model::{osm::graph::osm_element_filter::ElementFilter, OsmCliError},
};
use serde::{Deserialize, Serialize};

/// defines behaviors for an OSM network import
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct OsmImportConfiguration {
    #[serde(default)]
    pub component_filter: ComponentFilter,
    #[serde(default)]
    pub element_filter: ElementFilter,
    #[serde(default)]
    pub ignore_osm_parsing_errors: bool,
    #[serde(default = "default_true")]
    pub truncate_by_edge: bool,
    #[serde(default = "default_true")]
    pub simplify: bool,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub infrastructure_match_distance_meters: Option<f64>,
}

fn default_true() -> bool {
    true
}

impl Default for OsmImportConfiguration {
    fn default() -> Self {
        Self {
            component_filter: Default::default(),
            element_filter: Default::default(),
            ignore_osm_parsing_errors: false,
            truncate_by_edge: true,
            simplify: true,
            overwrite: false,
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
