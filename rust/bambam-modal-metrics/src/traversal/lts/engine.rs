use super::LtsConfig;

use routee_compass_core::model::traversal::TraversalModelError;

pub struct LtsEngine {}

impl TryFrom<LtsConfig> for LtsEngine {
    type Error = TraversalModelError;

    fn try_from(_config: LtsConfig) -> Result<Self, Self::Error> {
        Ok(Self {})
    }
}
