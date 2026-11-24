use std::sync::Arc;

use routee_compass::{
    app::search::SearchApp,
    plugin::input::{InputPlugin, InputPluginError},
};
use serde_json::Value;

use super::trip_modes_config::CompiledTripMode;

pub struct TripModesInputPlugin {
    pub modes: Vec<CompiledTripMode>,
}

impl TripModesInputPlugin {
    pub fn new(modes: Vec<CompiledTripMode>) -> Self {
        Self { modes }
    }
}

impl InputPlugin for TripModesInputPlugin {
    fn process(
        &self,
        input: &mut Value,
        _search_app: Arc<SearchApp>,
    ) -> Result<(), InputPluginError> {
        match input {
            Value::Object(base_map) => {
                let mut expanded = Vec::with_capacity(self.modes.len());
                for mode in &self.modes {
                    let mut row = base_map.clone();
                    for (k, v) in &mode.fields {
                        row.insert(k.clone(), v.clone());
                    }
                    expanded.push(Value::Object(row));
                }
                *input = Value::Array(expanded);
                Ok(())
            }
            Value::Array(items) => {
                let mut expanded = Vec::with_capacity(items.len() * self.modes.len());
                for item in items.iter() {
                    let base_map = item.as_object().ok_or_else(|| {
                        InputPluginError::InputPluginFailed(
                            "trip_modes input plugin expected each array element to be a JSON object"
                                .to_string(),
                        )
                    })?;
                    for mode in &self.modes {
                        let mut row = base_map.clone();
                        for (k, v) in &mode.fields {
                            row.insert(k.clone(), v.clone());
                        }
                        expanded.push(Value::Object(row));
                    }
                }
                *input = Value::Array(expanded);
                Ok(())
            }
            _ => Err(InputPluginError::InputPluginFailed(
                "trip_modes input plugin expected a JSON object or array of JSON objects"
                    .to_string(),
            )),
        }
    }

    fn name(&self) -> &str {
        "trip_modes"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::input_plugin::trip_modes::trip_modes_config::TripModesConfig;
    use serde_json::json;

    #[test]
    fn test_trip_modes_compile_and_expand() {
        let config_json = json!({
            "defaults": {
                "constraints": [
                    { "type": "time_limit", "max": 40.0, "unit": "minutes" }
                ]
            },
            "modes": {
                "walk": {
                    "intensity": { "energy": 0.0, "cost": 0.0 },
                    "constraints": [
                        { "type": "allowed_modes", "values": ["walk"] }
                    ]
                },
                "drive": {
                    "intensity": { "energy": 0.9, "cost": 0.48 },
                    "constraints": [
                        { "type": "mode_leg_distance_limit", "mode": "walk", "leg": "first", "max": 0.5, "unit": "kilometers" },
                        { "type": "exact_sequences", "values": [["drive"], ["walk", "drive"]] }
                    ]
                }
            }
        });

        let config: TripModesConfig = serde_json::from_value(config_json).unwrap();
        let compiled = config.compile().unwrap();
        assert_eq!(compiled.len(), 2);

        // "drive" is first alphabetically
        assert_eq!(compiled[0].trip_mode, "drive");
        assert_eq!(compiled[0].fields["max_trip_legs"], json!(2));
        assert_eq!(
            compiled[0].fields["constraints"].as_array().unwrap().len(),
            3
        );

        // Also verify that the serialized constraints inside each compiled mode round-trip
        // cleanly back into MultimodalConstraintModelQuery
        for mode in &compiled {
            let mut query_obj = serde_json::Map::new();
            for (k, v) in &mode.fields {
                query_obj.insert(k.clone(), v.clone());
            }
            let query_val = serde_json::Value::Object(query_obj);
            let parsed: crate::model::constraint::multimodal::MultimodalConstraintModelQuery =
                serde_json::from_value(query_val).expect(
                    "compiled mode fields should round-trip into MultimodalConstraintModelQuery",
                );
            let mode_to_state = bambam_core::model::state::CategoricalMapping::new(&[
                "walk".to_string(),
                "drive".to_string(),
            ])
            .unwrap();
            assert!(parsed.build_constraints(&mode_to_state).is_ok());
        }
    }

    #[test]
    fn test_example_trip_modes_toml() {
        use crate::model::input_plugin::trip_modes::trip_modes_input_plugin_builder::TripModesInputPluginBuilder;
        use routee_compass::plugin::input::InputPluginBuilder;

        let toml_str = include_str!("example_trip_modes.toml");
        let toml_val: toml::Value = toml::from_str(toml_str).expect("valid TOML");
        let json_val: serde_json::Value =
            serde_json::to_value(toml_val).expect("converts to JSON value");

        let builder = TripModesInputPluginBuilder {};
        let _plugin = builder
            .build(&json_val)
            .expect("example_trip_modes.toml should build cleanly");

        let mut clean = json_val;
        clean.as_object_mut().unwrap().remove("type");
        let config: TripModesConfig = serde_json::from_value(clean).unwrap();
        let compiled = config.compile().unwrap();
        assert_eq!(compiled.len(), 4);

        let mode_to_state = bambam_core::model::state::CategoricalMapping::new(&[
            "walk".to_string(),
            "bike".to_string(),
            "drive".to_string(),
            "transit".to_string(),
        ])
        .unwrap();

        for mode in &compiled {
            let query_val = serde_json::Value::Object(mode.fields.clone());
            let parsed: crate::model::constraint::multimodal::MultimodalConstraintModelQuery =
                serde_json::from_value(query_val).unwrap();
            assert!(parsed.build_constraints(&mode_to_state).is_ok());
        }
    }
}
