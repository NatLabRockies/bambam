use super::GbfsConstraintConfig;

use routee_compass_core::model::constraint::ConstraintModelError;

pub struct GbfsConstraintEngine {
    config: GbfsConstraintConfig,
}

impl TryFrom<GbfsConstraintConfig> for GbfsConstraintEngine {
    type Error = ConstraintModelError;

    fn try_from(config: GbfsConstraintConfig) -> Result<Self, Self::Error> {
        Ok(Self { config })
    }
}
