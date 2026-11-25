use super::LtsConfig;

use routee_compass_core::model::traversal::TraversalModelError;

pub struct LtsEngine {
    config: LtsConfig,
}

impl TryFrom<LtsConfig> for LtsEngine {
    type Error = TraversalModelError;

    fn try_from(config: LtsConfig) -> Result<Self, Self::Error> {
        Ok(Self { config })
    }
}
