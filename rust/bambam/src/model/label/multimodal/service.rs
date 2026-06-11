use crate::model::label::multimodal::{MultimodalLabelConfig, MultimodalLabelModel};
use bambam_core::model::state::{CategoricalMapping, LegIdx};
use routee_compass_core::model::{
    label::{
        label_model_error::LabelModelError, label_model_service::LabelModelService, Label,
        LabelModel,
    },
    network::VertexId,
    state::{StateModel, StateVariable},
};
use serde_json::Value;
use std::{num::NonZeroU64, sync::Arc};

pub struct MultimodalLabelService {
    config: MultimodalLabelConfig,
}

impl LabelModelService for MultimodalLabelService {
    fn build(
        &self,
        query: &serde_json::Value,
        state_model: Arc<StateModel>,
    ) -> Result<Arc<dyn LabelModel>, LabelModelError> {
        let optional_conf: MultimodalLabelConfig =
            serde_json::from_value(query.clone()).map_err(|e| {
                LabelModelError::LabelModelError(String::from(
                    "internal error: MultimodalLabelConfig must only have optional fields",
                ))
            })?;
        let max_trip_legs = self.get_max_trip_legs(&optional_conf)?;
        let modes = self.get_modes(&optional_conf)?;
        let mapping = CategoricalMapping::new(modes)?;
        let model = MultimodalLabelModel::new(mapping, max_trip_legs);
        Ok(Arc::new(model))
    }
}

impl MultimodalLabelService {
    pub fn new(config: MultimodalLabelConfig) -> Self {
        Self { config }
    }

    /// get the max trip legs from the query or fallback to the service config
    pub fn get_max_trip_legs(
        &self,
        query_conf: &MultimodalLabelConfig,
    ) -> Result<NonZeroU64, LabelModelError> {
        self.get_conf(query_conf, "max_trip_legs", |c| c.max_trip_legs)
    }

    /// get the modes from the query or fallback to the service config
    pub fn get_modes<'a>(
        &'a self,
        query_conf: &'a MultimodalLabelConfig,
    ) -> Result<&'a [String], LabelModelError> {
        if let Some(modes) = &query_conf.modes {
            Ok(modes.as_slice())
        } else if let Some(modes) = &self.config.modes {
            Ok(modes.as_slice())
        } else {
            Err(LabelModelError::LabelModelError(
                "'modes' must be provided either via app [label] section or via search query"
                    .to_string(),
            ))
        }
    }

    /// helper to attempt to get a value from either the query configuration or the service configuration
    /// as all fields are optional
    pub fn get_conf<T>(
        &self,
        query_conf: &MultimodalLabelConfig,
        field: &str,
        f: impl Fn(&MultimodalLabelConfig) -> Option<T>,
    ) -> Result<T, LabelModelError> {
        f(query_conf).or_else(|| f(&self.config)).ok_or_else(|| {
            LabelModelError::LabelModelError(format!(
                "'{field}' must be provided either via app [label] section or via search query"
            ))
        })
    }
}
