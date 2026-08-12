use num_traits::CheckedAdd;

use super::ops::*;
use crate::{
    app::network::common::{
        cycleway_tag::CyclewayTag, ops::traffic_speed_from_maxspeed, way_rtree_entry::WayRTreeEntry,
    },
    model::osm::graph::{OsmNodeDataSerializable, OsmWayDataSerializable},
};
pub const MIN_WCI: i32 = -6;
pub const MAX_WCI: i32 = 9;

#[derive(Default, Eq, PartialEq, PartialOrd, Debug)]
pub struct Wci(i32);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WciError {
    #[error("Wci '{0}' must be in the integer range: [-6..9]")]
    ValueError(i32),
}

// borrowed + borrowed -> owned
impl<'a> std::ops::Add<&'a Wci> for &'a Wci {
    type Output = Wci;

    fn add(self, rhs: Self) -> Self::Output {
        let sum = self.0 + rhs.0;
        Wci::new(sum).unwrap_or_else(|_| Wci(sum.clamp(MIN_WCI, MAX_WCI)))
    }
}

// owned + borrowed -> owned
impl std::ops::Add<&Wci> for Wci {
    type Output = Self;

    fn add(self, rhs: &Wci) -> Self::Output {
        let sum = self.0 + rhs.0;
        Wci::new(sum).unwrap_or_else(|_| Wci(sum.clamp(MIN_WCI, MAX_WCI)))
    }
}

// owned + owned -> owned
impl std::ops::Add<Wci> for Wci {
    type Output = Self;

    fn add(self, rhs: Wci) -> Self::Output {
        let sum = self.0 + rhs.0;
        Wci::new(sum).unwrap_or_else(|_| Wci(sum.clamp(MIN_WCI, MAX_WCI)))
    }
}

// checked: owned + owned -> owned
impl CheckedAdd for Wci {
    fn checked_add(&self, rhs: &Self) -> Option<Self> {
        let sum = self.0 + rhs.0;
        if (MIN_WCI..=MAX_WCI).contains(&sum) {
            Some(Wci(sum))
        } else {
            None
        }
    }
}

impl std::fmt::Display for Wci {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Wci {
    pub const ZERO: Wci = Wci(0);

    pub fn new(value: i32) -> Result<Wci, WciError> {
        if (MIN_WCI..=MAX_WCI).contains(&value) {
            Ok(Wci(value))
        } else {
            Err(WciError::ValueError(value))
        }
    }

    /// Computes the walkability `Wci` for a way.
    pub fn walkability(way: &OsmWayDataSerializable) -> Wci {
        if way_is_sidewalk(way) || way_is_footway(way) {
            Wci(2)
        } else {
            Wci(-2)
        }
    }

    /// Computes the traffic signal `Wci` for a way.
    pub fn traffic_signal_comfort(src_node: &OsmNodeDataSerializable) -> Wci {
        if has_traffic_signals(src_node) {
            Wci(2)
        } else if has_stop_sign(src_node) {
            Wci(1)
        } else {
            Wci(0)
        }
    }

    /// Computes the cycleway `Wci` for a way.
    pub fn cycleway_comfort(entry: &WayRTreeEntry, neighboring_ways: &Vec<&WayRTreeEntry>) -> Wci {
        // if the way has a cycleway tag (string), use that, otherwise, use neighbors
        match &entry.way.cycleway {
            Some(tag) => Wci(cycleway_comfort_from_tag(&CyclewayTag::new(tag))),
            None => Wci(cycleway_comfort_from_neighbors(entry, neighboring_ways)),
        }
    }

    /// Computes the traffic speed `Wci` for a way
    pub fn traffic_speed_comfort(entry: &WayRTreeEntry, neighbors: &Vec<&WayRTreeEntry>) -> Wci {
        Wci(traffic_speed_from_maxspeed(entry)
            .map(|speed_mph| traffic_speed_comfort_from_speed(speed_mph.round() as i32))
            .unwrap_or_else(|| traffic_speed_comfort_from_neighbors(entry, neighbors)))
    }
}
