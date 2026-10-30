use serde::{Deserialize, Serialize};

use crate::model::output_plugin::h3_util::BoundaryGeometryFormat;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case", tag = "utility")]
pub enum H3UtilOutputPluginConfig {
    /// reads an h3 identifier from some path in the output JSON and uses
    /// h3 cellToBoundary to write the h3
    /// see <https://h3geo.org/docs/api/indexing#celltoboundary>.
    H3BoundaryToGeometry {
        from: String,
        to: String,
        format: Option<BoundaryGeometryFormat>,
        overwrite: Option<bool>,
    },
    /// copies an h3 identifier from some JSONPath to another JSONPath,
    /// converting it to the declared parent resolution.
    H3ToParent {
        from: String,
        to: String,
        resolution: u8,
        overwrite: Option<bool>,
    },
}
