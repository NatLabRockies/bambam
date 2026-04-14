use std::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::model::feature::highway::Highway;

use super::OsmWayId;

#[derive(Serialize, Deserialize, Clone, Debug, Eq)]
pub struct OsmSegment {
    pub way_id: OsmWayId,
    pub highway: Option<Highway>,
    pub is_oneway: bool,
}

impl Display for OsmSegment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let oneway = if self.is_oneway {
            "segment that was originally one way"
        } else {
            "segment that was originally undirected"
        };
        let highway = match &self.highway {
            Some(h) => h.to_string(),
            None => String::from("<missing>"),
        };
        write!(
            f,
            "OsmSegment with way_id={}, a {} with highway tag {}",
            self.way_id, highway, oneway
        )
    }
}

impl PartialEq for OsmSegment {
    fn eq(&self, other: &Self) -> bool {
        self.way_id == other.way_id
            && self.highway == other.highway
            && self.is_oneway == other.is_oneway
    }
}

impl PartialOrd for OsmSegment {
    /// defers to the ordering of the highway values if available.
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering as O;
        match (&self.highway, &other.highway) {
            (None, None) => None,
            (None, Some(_)) => Some(O::Less),
            (Some(_), None) => Some(O::Greater),
            (Some(a), Some(b)) => Some(a.cmp(b)),
        }
    }
}

impl Ord for OsmSegment {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap_or(std::cmp::Ordering::Equal)
    }
}

impl OsmSegment {
    pub fn new(way_id: OsmWayId, highway: Option<Highway>, is_oneway: bool) -> OsmSegment {
        OsmSegment {
            way_id,
            highway,
            is_oneway,
        }
    }
}
