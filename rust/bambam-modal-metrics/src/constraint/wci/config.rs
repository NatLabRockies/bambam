use serde::{Deserialize, Serialize};

/// configuration of the WCI service
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct WciConfig {
    /// file containing WCI values for an edge list
    pub wci_input_file: String,
}
