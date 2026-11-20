use std::{collections::HashMap, num::NonZeroU64};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::model::constraint::multimodal::ConstraintConfig;

/// Configuration for the `trip_modes` input plugin, which fans out each incoming
/// query into one query per configured travel mode, merging shared `defaults`
/// with per-mode overrides and validating multimodal constraints at startup.
///
/// # Example
///
/// ```toml
/// [[plugin.input_plugins]]
/// type = "trip_modes"
///
/// [plugin.input_plugins.defaults]
/// multimodal_label = false
/// constraints = [
///     { type = "time_limit", max = 40.0, unit = "minutes" },
/// ]
///
/// [plugin.input_plugins.modes.walk]
/// intensity = { energy = 0.0, cost = 0.0 }
/// constraints = [
///     { type = "allowed_modes", values = ["walk"] },
/// ]
///
/// [plugin.input_plugins.modes.bike]
/// intensity = { energy = 0.0, cost = 0.0 }
/// constraints = [
///     { type = "allowed_modes", values = ["bike"] },
/// ]
///
/// [plugin.input_plugins.modes.drive]
/// intensity = { energy = 0.9, cost = 0.48 }
/// constraints = [
///     { type = "mode_leg_distance_limit", mode = "walk", leg = "first", max = 0.5, unit = "kilometers" },
///     { type = "exact_sequences", values = [["drive"], ["walk", "drive"]] },
/// ]
///
/// [plugin.input_plugins.modes.transit]
/// multimodal_label = true
/// max_trip_legs = 4
/// intensity = { energy = 0.65, cost = 0.855 }
/// constraints = [
///     { type = "mode_time_limit", mode = "walk", max = 10.0, unit = "minutes" },
///     { type = "mode_leg_time_limit", mode = "transit", min = 3.0, unit = "minutes" },
///     { type = "allowed_modes", values = ["walk", "transit"] },
/// ]
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TripModesConfig {
    /// Optional shared defaults applied to every mode before per-mode fields.
    #[serde(default)]
    pub defaults: TripModeDefaults,
    /// Map from `trip_mode` name (e.g., `"walk"`, `"bike"`, `"drive"`, `"transit"`)
    /// to its mode-specific configuration.
    pub modes: HashMap<String, TripModeEntry>,
}

/// Shared defaults applied across all modes in [`TripModesConfig`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TripModeDefaults {
    #[serde(default)]
    pub max_trip_legs: Option<NonZeroU64>,
    /// When `false`, uses `VertexLabelModel` (`Label::Vertex`) instead of `MultimodalLabelModel`
    /// so that the cost-dominant path to each vertex wins regardless of mode sequence.
    #[serde(default)]
    pub multimodal_label: Option<bool>,
    #[serde(default)]
    pub constraints: Vec<ConstraintConfig>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Per-mode configuration entry in [`TripModesConfig`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TripModeEntry {
    #[serde(default)]
    pub max_trip_legs: Option<NonZeroU64>,
    /// When `false`, uses `VertexLabelModel` (`Label::Vertex`) instead of `MultimodalLabelModel`
    /// so that the cost-dominant path to each vertex wins regardless of mode sequence.
    #[serde(default)]
    pub multimodal_label: Option<bool>,
    #[serde(default)]
    pub constraints: Vec<ConstraintConfig>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Pre-built JSON fields for a single `trip_mode`, ready to be merged into each incoming query.
#[derive(Debug, Clone)]
pub struct CompiledTripMode {
    pub trip_mode: String,
    pub fields: Map<String, Value>,
}

impl TripModesConfig {
    /// Compiles and validates the configured modes into a deterministic list of [`CompiledTripMode`]s.
    pub fn compile(&self) -> Result<Vec<CompiledTripMode>, String> {
        if self.modes.is_empty() {
            return Err(
                "trip_modes input plugin requires at least one entry in 'modes'".to_string(),
            );
        }

        // Sort mode keys deterministically
        let mut mode_names: Vec<&String> = self.modes.keys().collect();
        mode_names.sort();

        let mut compiled = Vec::with_capacity(mode_names.len());
        for mode_name in mode_names {
            let entry = &self.modes[mode_name];
            let mut fields = self.defaults.extra.clone();
            for (k, v) in &entry.extra {
                fields.insert(k.clone(), v.clone());
            }

            // Combine default constraints with mode-specific constraints
            let mut combined_constraints = self.defaults.constraints.clone();
            combined_constraints.extend(entry.constraints.iter().cloned());

            // Resolve max_trip_legs: entry -> defaults -> inferred from exact_sequences -> 1
            let explicit_max_legs = entry.max_trip_legs.or(self.defaults.max_trip_legs);
            let max_seq_len = combined_constraints
                .iter()
                .filter_map(|c| match c {
                    ConstraintConfig::ExactSequences { values } => {
                        values.iter().map(|s| s.len()).max()
                    }
                    _ => None,
                })
                .max();

            let resolved_max_legs = match (explicit_max_legs, max_seq_len) {
                (Some(explicit), Some(seq_len)) if seq_len > explicit.get() as usize => {
                    return Err(format!(
                        "mode '{mode_name}' has an exact_sequences entry of length {seq_len}, which exceeds max_trip_legs ({explicit})"
                    ));
                }
                (Some(explicit), _) => explicit,
                (None, Some(seq_len)) => NonZeroU64::new(seq_len as u64).unwrap_or(NonZeroU64::MIN),
                (None, None) => NonZeroU64::MIN,
            };

            let constraints_json = serde_json::to_value(&combined_constraints).map_err(|e| {
                format!("failed to serialize constraints for mode '{mode_name}': {e}")
            })?;

            fields.insert("trip_mode".to_string(), json!(mode_name));
            fields.insert("max_trip_legs".to_string(), json!(resolved_max_legs));
            fields.insert("constraints".to_string(), constraints_json);
            if let Some(mm_label) = entry.multimodal_label.or(self.defaults.multimodal_label) {
                fields.insert("multimodal_label".to_string(), json!(mm_label));
            }

            compiled.push(CompiledTripMode {
                trip_mode: mode_name.clone(),
                fields,
            });
        }

        Ok(compiled)
    }
}
