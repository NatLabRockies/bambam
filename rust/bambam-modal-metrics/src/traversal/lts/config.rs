use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct LtsConfig {
    /// file with LTS values for each edge.
    pub lts_input_file: String,
}
