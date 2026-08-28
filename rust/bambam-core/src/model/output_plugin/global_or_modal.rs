use std::collections::HashMap;

use routee_compass::plugin::output::OutputPluginError;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum GlobalOrModal<T> {
    Global {
        value: T,
    },
    Modal {
        values: HashMap<String, T>,
        fallback: Option<T>,
    },
}

impl<T> GlobalOrModal<T> {
    /// gets the configuration value. if the user has provided a global value, we are done.
    /// if the user has provided modal values, we first check if this mode has a matching config.
    /// if not, we attempt to draw from the fallback value.
    pub fn get_value(&self, mode: &str) -> Result<&T, OutputPluginError> {
        match self {
            GlobalOrModal::Global { value } => Ok(value),
            GlobalOrModal::Modal { values, fallback } => {
                if let Some(value) = values.get(mode) {
                    return Ok(value);
                }
                fallback.as_ref().ok_or_else(|| {
                    let name = std::any::type_name::<T>();
                    let msg = format!("user specified modal configuration for {name} but mode '{mode}' not found and no fallback specified.");
                    OutputPluginError::OutputPluginFailed(msg)
                })
            }
        }
    }
}
