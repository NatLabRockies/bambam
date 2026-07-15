use num_traits::CheckedAdd;

use super::ops::*;
use crate::{
    app::network::common::{cycleway_tag::CyclewayTag, way_rtree_entry::WayRTreeEntry},
    model::osm::graph::{OsmNodeDataSerializable, OsmWayDataSerializable},
};
pub const MIN_WCI_SCORE: i32 = -6;
pub const MAX_WCI_SCORE: i32 = 9;

#[derive(Default, Eq, PartialEq, PartialOrd, Debug)]
pub struct WciScore(i32);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WciError {
    #[error("WciScore value '{0}' must be in the integer range: [-6..9]")]
    ValueError(i32),
}

// borrowed + borrowed -> owned
impl<'a> std::ops::Add<&'a WciScore> for &'a WciScore {
    type Output = WciScore;

    fn add(self, rhs: Self) -> Self::Output {
        let sum = self.0 + rhs.0;
        WciScore::new(sum).unwrap_or_else(|_| WciScore(sum.clamp(MIN_WCI_SCORE, MAX_WCI_SCORE)))
    }
}

// owned + borrowed -> owned
impl std::ops::Add<&WciScore> for WciScore {
    type Output = Self;

    fn add(self, rhs: &WciScore) -> Self::Output {
        let sum = self.0 + rhs.0;
        WciScore::new(sum).unwrap_or_else(|_| WciScore(sum.clamp(MIN_WCI_SCORE, MAX_WCI_SCORE)))
    }
}

// owned + owned -> owned
impl std::ops::Add<WciScore> for WciScore {
    type Output = Self;

    fn add(self, rhs: WciScore) -> Self::Output {
        let sum = self.0 + rhs.0;
        WciScore::new(sum).unwrap_or_else(|_| WciScore(sum.clamp(MIN_WCI_SCORE, MAX_WCI_SCORE)))
    }
}

// checked: owned + owned -> owned
impl CheckedAdd for WciScore {
    fn checked_add(&self, rhs: &Self) -> Option<Self> {
        let sum = self.0 + rhs.0;
        if (MIN_WCI_SCORE..=MAX_WCI_SCORE).contains(&sum) {
            Some(WciScore(sum))
        } else {
            None
        }
    }
}

impl std::fmt::Display for WciScore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl WciScore {
    pub fn new(value: i32) -> Result<WciScore, WciError> {
        if (MIN_WCI_SCORE..=MAX_WCI_SCORE).contains(&value) {
            Ok(WciScore(value))
        } else {
            Err(WciError::ValueError(value))
        }
    }

    /// Computes the walkability `WciScore` for a way.
    pub fn walkability_score(way: &OsmWayDataSerializable) -> WciScore {
        if way_is_sidewalk(way) || way_is_footway(way) {
            WciScore(2)
        } else {
            WciScore(-2)
        }
    }

    /// Computes the traffic signal `WciScore` for a way.
    pub fn traffic_signal_score(src_node: &OsmNodeDataSerializable) -> WciScore {
        if has_traffic_signals(src_node) {
            WciScore(2)
        } else if has_stop_sign(src_node) {
            WciScore(1)
        } else {
            WciScore(0)
        }
    }

    /// Computes the cycleway `WciScore` for a way.
    pub fn cycleway_score(
        entry: &WayRTreeEntry,
        neighboring_ways: &Vec<&WayRTreeEntry>,
    ) -> WciScore {
        // if the way has a cycleway tag (string), use that, otherwise, use neighbors
        match &entry.way.cycleway {
            Some(tag) => WciScore(cycleway_score_from_tag(&CyclewayTag::new(tag))),
            None => WciScore(cycleway_score_from_neighbors(entry, neighboring_ways)),
        }
    }

    /// Computes the traffic speed `WciScore` for a way
    pub fn traffic_speed_score(entry: &WayRTreeEntry, neighbors: &Vec<&WayRTreeEntry>) -> WciScore {
        WciScore(
            traffic_speed_from_maxspeed(entry)
                .map(|speed_mph| traffic_speed_score_from_speed(speed_mph.round() as i32))
                .unwrap_or_else(|| traffic_speed_score_from_neighbors(entry, neighbors)),
        )
    }
}
