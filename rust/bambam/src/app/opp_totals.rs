use crate::model::output_plugin::opportunity::OpportunityModelConfig;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::BTreeMap, path::Path};

/// Controls whether the output `opportunity_totals` mapping is serialized as a free-standing
/// `[opportunity_totals]` section or namespaced under `[plugin.output_plugins.opportunity_totals]`
/// for direct inclusion in a BAMBAM configuration file.
#[derive(ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TotalsNamespace {
    /// Free-standing `[opportunity_totals]` table (for an `OpportunityPluginConfig`)
    #[default]
    Standalone,
    /// Namespaced under `[plugin.output_plugins.opportunity_totals]` for pasting into a BAMBAM config
    OutputPlugins,
}

/// Runs the opportunity model defined in `config_file`, sums opportunities by activity type,
/// and writes the serialized totals to `output_file` (or stdout if `None`).
pub fn run(
    config_file: &str,
    vertices_file: Option<&str>,
    namespace: TotalsNamespace,
    output_file: Option<&str>,
) -> Result<(), String> {
    let mut model_config = OpportunityModelConfig::try_from(Path::new(config_file))?;
    if let Some(vf) = vertices_file {
        model_config.set_vertices_file(vf);
    }

    let model = model_config.build().map_err(|e| e.to_string())?;
    let totals_map = model.opportunity_totals()?;
    let sorted_totals: BTreeMap<String, f64> = totals_map.into_iter().collect();

    let is_json = output_file
        .and_then(|f| Path::new(f).extension())
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("json"))
        .unwrap_or(false);

    let serialized = format_opportunity_totals(&sorted_totals, namespace, is_json)?;

    match output_file {
        Some(path) => {
            std::fs::write(path, &serialized)
                .map_err(|e| format!("failed to write opportunity totals to '{path}': {e}"))?;
            log::info!("wrote opportunity totals to {path}");
        }
        None => {
            print!("{serialized}");
        }
    }

    Ok(())
}

/// Formats `totals` according to the requested [`TotalsNamespace`] and file format (TOML or JSON).
pub fn format_opportunity_totals(
    totals: &BTreeMap<String, f64>,
    namespace: TotalsNamespace,
    as_json: bool,
) -> Result<String, String> {
    let value = match namespace {
        TotalsNamespace::Standalone => json!({
            "opportunity_totals": totals,
        }),
        TotalsNamespace::OutputPlugins => json!({
            "plugin": {
                "output_plugins": {
                    "opportunity_totals": totals,
                }
            }
        }),
    };

    if as_json {
        serde_json::to_string_pretty(&value)
            .map_err(|e| format!("failed to serialize opportunity totals as JSON: {e}"))
    } else {
        toml::to_string_pretty(&value)
            .map_err(|e| format!("failed to serialize opportunity totals as TOML: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::output_plugin::opportunity::OpportunityPluginConfig;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_from_file_standalone_and_namespaced() {
        let dir = tempdir().unwrap();
        let opp_csv = dir.path().join("opps.csv");
        {
            let mut f = std::fs::File::create(&opp_csv).unwrap();
            writeln!(f, "retail,food,jobs").unwrap();
            writeln!(f, "10.0,25.5,100.0").unwrap();
            writeln!(f, "5.0,4.5,50.0").unwrap();
        }
        let opp_csv_str = opp_csv.to_string_lossy().replace('\\', "/");

        // 1. Free-standing OpportunityModelConfig TOML
        let standalone_toml_path = dir.path().join("standalone_model.toml");
        let standalone_toml = format!(
            r#"
type = "file"
opportunity_input_file = "{opp_csv_str}"
activity_column_names = ["retail", "food", "jobs"]
table_orientation = "destination_vertex_oriented"
"#
        );
        std::fs::write(&standalone_toml_path, standalone_toml).unwrap();

        let cfg1 = OpportunityModelConfig::try_from(standalone_toml_path.as_path()).unwrap();
        let totals1 = cfg1.build().unwrap().opportunity_totals().unwrap();
        assert_eq!(totals1.get("retail"), Some(&15.0));
        assert_eq!(totals1.get("food"), Some(&30.0));
        assert_eq!(totals1.get("jobs"), Some(&150.0));

        // 2. Namespaced under [[plugin.output_plugins]]
        let namespaced_toml_path = dir.path().join("namespaced.toml");
        let namespaced_toml = format!(
            r#"
[[plugin.output_plugins]]
type = "isochrone"

[[plugin.output_plugins]]
type = "opportunity"

[plugin.output_plugins.model]
type = "combined"

[[plugin.output_plugins.model.models]]
type = "file"
opportunity_input_file = "{opp_csv_str}"
activity_column_names = ["retail", "food"]
table_orientation = "destination_vertex_oriented"

[[plugin.output_plugins.model.models]]
type = "file"
opportunity_input_file = "{opp_csv_str}"
activity_column_names = ["jobs"]
table_orientation = "destination_vertex_oriented"
"#
        );
        std::fs::write(&namespaced_toml_path, namespaced_toml).unwrap();

        let cfg2 = OpportunityModelConfig::try_from(namespaced_toml_path.as_path()).unwrap();
        let totals2 = cfg2.build().unwrap().opportunity_totals().unwrap();
        assert_eq!(totals2.get("retail"), Some(&15.0));
        assert_eq!(totals2.get("food"), Some(&30.0));
        assert_eq!(totals2.get("jobs"), Some(&150.0));
    }

    #[test]
    fn test_set_vertices_file_recursive() {
        let raw_toml = r#"
type = "combined"

[[models]]
type = "api"
table_orientation = "destination_vertex_oriented"
activity_column_names = ["jobs"]

[models.opportunity_source]
type = "lodes"
year = 2022
job_type = "JT00"
data_granularity = "census_tract"
edition = "LODES8"
segment = "S000"
study_region = { type = "national" }
activity_mapping = { CNS01 = ["jobs"] }
"#;
        let val: serde_json::Value = toml::from_str(raw_toml).unwrap();
        let mut cfg = OpportunityModelConfig::try_from(&val).unwrap();
        cfg.set_vertices_file("us_network/vertices-compass.csv.gz");

        match cfg {
            OpportunityModelConfig::Combined { models } => match &*models[0] {
                OpportunityModelConfig::ApiSource {
                    vertex_input_file, ..
                } => {
                    assert_eq!(vertex_input_file, "us_network/vertices-compass.csv.gz");
                }
                _ => panic!("expected ApiSource"),
            },
            _ => panic!("expected Combined"),
        }
    }

    #[test]
    fn test_format_opportunity_totals_standalone_and_output_plugins() {
        let mut totals = BTreeMap::new();
        totals.insert("food".to_string(), 120.0);
        totals.insert("jobs".to_string(), 500.0);

        let standalone_str =
            format_opportunity_totals(&totals, TotalsNamespace::Standalone, false).unwrap();
        assert!(standalone_str.contains("[opportunity_totals]"));
        assert!(standalone_str.contains("food = 120.0"));
        assert!(standalone_str.contains("jobs = 500.0"));

        // Verify standalone snippet combines cleanly into OpportunityPluginConfig
        let full_plugin_toml = format!(
            r#"
{standalone_str}
[model]
type = "file"
opportunity_input_file = "dummy.csv"
activity_column_names = ["food", "jobs"]
table_orientation = "destination_vertex_oriented"
"#
        );
        let plugin_cfg: OpportunityPluginConfig = toml::from_str(&full_plugin_toml).unwrap();
        assert_eq!(
            plugin_cfg
                .opportunity_totals
                .as_ref()
                .and_then(|t| t.get("food")),
            Some(&120.0)
        );

        let namespaced_str =
            format_opportunity_totals(&totals, TotalsNamespace::OutputPlugins, false).unwrap();
        assert!(namespaced_str.contains("[plugin.output_plugins.opportunity_totals]"));
        assert!(namespaced_str.contains("food = 120.0"));
        assert!(namespaced_str.contains("jobs = 500.0"));
    }
}
