use itertools::Itertools;
use serde::{Deserialize, Serialize};

use super::{OsmNodeData, OsmNodeId};

/// used for IO in flat (CSV) format
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct OsmNodeDataSerializable {
    pub osmid: OsmNodeId,
    pub x: f32,
    pub y: f32,
    pub highway: Option<String>,
    pub ele: Option<String>,
    pub junction: Option<String>,
    pub railway: Option<String>,
    pub _ref: Option<String>,
    pub consolidated_ids: Option<String>,
}

impl OsmNodeDataSerializable {
    /// a delimter for aggregated fields which does not collide with CSV delimiters
    /// which can be used to replace OsmNodeData::VALUE_DELIMITER
    pub const VALUE_DELIMITER: &'static str = ";";
}

impl From<&OsmNodeData> for OsmNodeDataSerializable {
    fn from(value: &OsmNodeData) -> Self {
        Self {
            osmid: value.osmid,
            x: value.x,
            y: value.y,
            highway: replace_delimiter(&value.highway, Self::VALUE_DELIMITER),
            ele: replace_delimiter(&value.ele, Self::VALUE_DELIMITER),
            junction: replace_delimiter(&value.junction, Self::VALUE_DELIMITER),
            railway: replace_delimiter(&value.railway, Self::VALUE_DELIMITER),
            _ref: replace_delimiter(&value._ref, Self::VALUE_DELIMITER),
            consolidated_ids: join_ids(&value.consolidated_ids, Self::VALUE_DELIMITER),
        }
    }
}

fn replace_delimiter(value: &Option<String>, delimiter: &'static str) -> Option<String> {
    value
        .as_ref()
        .map(|v| v.replace(OsmNodeData::VALUE_DELIMITER, delimiter))
}

fn join_ids(value: &[OsmNodeId], delimiter: &'static str) -> Option<String> {
    match value[..] {
        [] => None,
        _ => {
            let joined = value.iter().map(|id| format!("{id}")).join(delimiter);
            Some(joined)
        }
    }
}
