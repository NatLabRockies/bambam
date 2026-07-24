use super::GbfsTraversalConfig;

use routee_compass_core::model::traversal::TraversalModelError;

pub struct GbfsTraversalEngine {
    config: GbfsTraversalConfig,
}

impl TryFrom<GbfsTraversalConfig> for GbfsTraversalEngine {
    type Error = TraversalModelError;

    fn try_from(config: GbfsTraversalConfig) -> Result<Self, Self::Error> {
        Ok(Self { config })
    }
}
