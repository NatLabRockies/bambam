use bambam_network::CliBoundingBox;
use serde::{Deserialize, Serialize};

use crate::collection::OvertureMapsCollectionError;

#[derive(Clone, Debug, Serialize, Deserialize, Copy, PartialEq)]
pub struct Bbox {
    pub xmin: f32,
    pub xmax: f32,
    pub ymin: f32,
    pub ymax: f32,
}

impl Bbox {
    pub fn new(xmin: f32, xmax: f32, ymin: f32, ymax: f32) -> Self {
        Self {
            xmin,
            xmax,
            ymin,
            ymax,
        }
    }

    pub fn validate(&self) -> Result<(), OvertureMapsCollectionError> {
        if self.xmax < self.xmin || self.ymax < self.ymin {
            return Err(OvertureMapsCollectionError::InvalidUserInput(format!(
                "The provided Bbox is invalid: {self:?}"
            )));
        }

        Ok(())
    }
}

impl From<&CliBoundingBox> for Bbox {
    fn from(value: &CliBoundingBox) -> Self {
        Self {
            xmin: value.xmin,
            xmax: value.xmax,
            ymin: value.ymin,
            ymax: value.ymax,
        }
    }
}

impl From<Bbox> for CliBoundingBox {
    fn from(value: Bbox) -> Self {
        Self {
            xmin: value.xmin,
            xmax: value.xmax,
            ymin: value.ymin,
            ymax: value.ymax,
        }
    }
}
