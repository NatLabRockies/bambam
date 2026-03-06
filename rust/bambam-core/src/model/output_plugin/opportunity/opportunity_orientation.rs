use serde::{Deserialize, Serialize};

/// An enumeration representing how activities are tagged to the graph.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Hash, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OpportunityOrientation {
    OriginVertexOriented,
    #[default]
    DestinationVertexOriented,
    EdgeOriented,
}

impl std::fmt::Display for OpportunityOrientation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self)
            .unwrap_or(String::from(""))
            .replace('\"', "");
        write!(f, "{s}")
    }
}
