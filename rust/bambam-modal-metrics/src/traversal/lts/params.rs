use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct LtsParams {
    /// optional mapping override, allows query-time declaration of the
    /// mapping from LTS scores to the operations that modify speed.
    pub mapping: Option<super::config::LtsMappingConfig>,
}
