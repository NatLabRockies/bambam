use routee_compass_core::model::label::{label_model_error::LabelModelError, Label};

use crate::model::state::{MultimodalMapping, MultimodalStateMapping};

/// use the configuration of this label model to retrieve the state's
pub fn get_mode_sequence<'a>(
    label: &Label,
    mode_to_state: &'a MultimodalStateMapping,
) -> Result<Vec<&'a str>, LabelModelError> {
    match label {
        Label::VertexWithU8StateVec { vertex_id, state } => {
            let mut modes: Vec<&str> = vec![];
            let state_len = state.state_len as usize;
            for idx in (0..state_len) {
                match state.state.get(idx) {
                    None => {
                        return Err(LabelModelError::LabelModelError(format!(
                            "internal error: state has fewer than state_len={state_len} entries"
                        )))
                    }
                    Some(mode_u8) => {
                        let mode_i64: i64 = (*mode_u8).into();
                        let mode = mode_to_state.get_categorical(mode_i64)?.ok_or_else(|| {
                            LabelModelError::LabelModelError(format!(
                                "mode label {mode_i64} not present in multimodal label mapping"
                            ))
                        })?;
                        modes.push(mode);
                    }
                }
            }
            Ok(modes)
        }
        _ => Err(LabelModelError::LabelModelError(format!(
            "invalid label type, cannot get mode sequence: {label}"
        ))),
    }
}
