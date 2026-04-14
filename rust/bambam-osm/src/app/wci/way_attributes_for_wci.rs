//! WayAttributesForWCI struct is used to store information needed to calculate the Walking Comfort Index (wci.rs)
//! Information in the struct is derived from OSM data and neighbors in the RTree
//! August 2025 EG

use super::WayGeometryData;
use crate::model::feature::highway::Highway;
use geo::prelude::*;
use rstar::RTree;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct WayAttributesForWCI {
    pub speed_imp: Option<i32>,
    pub sidewalk_exists: Option<bool>,
    pub cycleway_exists: Option<(String, i32)>,
    pub traffic_signals_exists: Option<bool>,
    pub stops_exists: Option<bool>,
    pub dedicated_foot: Option<bool>,
    pub no_adjacent_roads: Option<bool>,
    pub walk_eligible: Option<bool>,
}

impl WayAttributesForWCI {
    pub fn new(
        centroid: geo::Point<f32>,
        rtree: &RTree<WayGeometryData>,
        geo_data: &WayGeometryData,
    ) -> Option<WayAttributesForWCI> {
        let query_pointf32 = [centroid.x(), centroid.y()];
        let query_point = geo::Point::new(centroid.x(), centroid.y());

        let mut sidewalk = match &geo_data.data.sidewalk {
            Some(string) => !(string == "no" || string == "none"),
            _ => false,
        };

        let foot = match &geo_data.data.footway {
            Some(string) => !(string == "no" || string == "none"),
            _ => false,
        };

        if geo_data.data.footway == Some("sidewalk".to_string()) {
            sidewalk = true;
        }

        let walk_el = walk_eligible(rtree, geo_data, query_pointf32, sidewalk, foot);
        if !walk_el {
            // return immediately
            return None;
        }

        let mut neighbors = vec![];
        for neighbor in rtree.locate_within_distance(query_pointf32, 0.0001378) {
            neighbors.push(neighbor);
        }
        let no_adj = neighbors.is_empty();

        let cycle = match &geo_data.data.cycleway {
            Some(string) => {
                if string == "lane" || string == "designated" || string == "track" {
                    ("dedicated", 2)
                } else if string == "crossing" || string == "shared" || string == "shared_lane" {
                    ("some_cycleway", 0)
                } else {
                    ("no_cycleway", -2)
                }
            }
            _ => {
                // neighbor weighting
                // let weighted_cycle = 0;
                let mut total_lengths: f32 = 0.0;
                let mut cyclescores = vec![];
                for neighbor in rtree.locate_within_distance(query_pointf32, 0.0001378) {
                    let origin = neighbor.geo.centroid();
                    if let Some(origin) = origin {
                        let int_length = Euclidean::distance(&geo::Euclidean, origin, query_point);
                        total_lengths += int_length;
                        if let Some(ref cycleway) = neighbor.data.cycleway {
                            let neighbor_cycle_score = if cycleway == "lane"
                                || cycleway == "designated"
                                || cycleway == "track"
                            {
                                2
                            } else if cycleway == "crossing"
                                || cycleway == "shared"
                                || cycleway == "shared_lane"
                            {
                                0
                            } else {
                                -2
                            };
                            cyclescores.push((neighbor_cycle_score, int_length));
                        }
                    } else {
                        continue;
                    }
                }
                let mut result_cycle: f32 = 0.0;
                for (neighbor_cyclescore, length) in &cyclescores {
                    let weight = length / total_lengths;
                    result_cycle += (*neighbor_cyclescore as f32) * weight;
                }
                if !cyclescores.is_empty() && total_lengths != 0.0 {
                    ("from_neighbors", result_cycle as i32)
                } else {
                    ("no_cycleway", -2)
                }
            }
        };

        let speed: i32 = match geo_data.data.maxspeed.clone() {
            Some(speed_str) => speed_str.parse::<i32>().unwrap_or_default(),
            None => {
                // look at neighbors, weighted average
                let mut speeds = vec![];
                let mut total_lengths: f32 = 0.0;
                for neighbor in rtree.locate_within_distance(query_pointf32, 0.0001378) {
                    if let Some(origin) = neighbor.geo.centroid() {
                        let int_length = Euclidean::distance(&geo::Euclidean, origin, query_point);
                        total_lengths += int_length;
                        if let Some(neighbor_speed_str) = &neighbor.data.maxspeed {
                            if let Ok(int_neighbor_speed) = neighbor_speed_str.parse::<i32>() {
                                speeds.push((int_neighbor_speed, int_length));
                            }
                        }
                    }
                }
                let mut result_speed = 0.0;
                for (neighbor_speed, length) in &speeds {
                    let weight = length / total_lengths;
                    result_speed += (*neighbor_speed as f32) * weight;
                }
                if !speeds.is_empty() && total_lengths != 0.0 {
                    result_speed as i32
                } else {
                    0
                }
            }
        };

        let way_info = WayAttributesForWCI {
            speed_imp: Some(speed),
            sidewalk_exists: Some(sidewalk),
            cycleway_exists: Some((cycle.0.to_string(), cycle.1)),
            traffic_signals_exists: Some(geo_data.traf_sig),
            stops_exists: Some(geo_data.stop),
            dedicated_foot: Some(foot),
            no_adjacent_roads: Some(no_adj),
            walk_eligible: Some(walk_el),
        };

        Some(way_info)
    }

    /// Calculate the Walk Comfort Index (WCI) for a given way
    pub fn wci_calculate(self) -> Option<i32> {
        if self.walk_eligible == Some(false) {
            None
        } else if self.dedicated_foot == Some(true)
            || (self.no_adjacent_roads == Some(true) && self.sidewalk_exists == Some(true))
        {
            Some(super::MAX_WCI_SCORE)
        } else {
            // Speed: 0-25 mph: 2, 25-30 mph: 1, 30-40 mph: 0, 40-45 mph: -1, 45+ mph: -2
            fn speed_score(way: &WayAttributesForWCI) -> i32 {
                match way.speed_imp {
                    Some(speed) => {
                        let mph = (speed as f64 / 1.61).round();
                        if mph <= 25.0 {
                            2
                        } else if mph > 25.0 && mph <= 30.0 {
                            1
                        } else if mph > 30.0 && mph <= 40.0 {
                            0
                        } else if mph > 40.0 && mph <= 45.0 {
                            -1
                        } else {
                            -2
                        }
                    }
                    None => -2,
                }
            }

            // Sidewalk: +2 if present, -2 if not
            fn sidewalk_score(way: &WayAttributesForWCI) -> i32 {
                match way.sidewalk_exists {
                    Some(value) => {
                        if value {
                            2
                        } else {
                            -2
                        }
                    }
                    None => -2,
                }
            }

            // Cycleway: +2 if dedicated, 0 if some, -2 if none, or weighted from neihgbors
            fn cycleway_score(way: &WayAttributesForWCI) -> i32 {
                match way.cycleway_exists.as_ref() {
                    Some(cycle_score) => {
                        if cycle_score.0 == "dedicated" {
                            2
                        } else if cycle_score.0 == "some_cycleway" {
                            0
                        } else if cycle_score.0 == "from_neighbors" {
                            cycle_score.1 //check this works
                        } else {
                            -2
                        }
                    }
                    None => -2,
                }
            }

            // Traffic Signals: +2 if traffic signals exists, 1 if stops exist, 0 if neither
            fn signal_or_stop_score(way: &WayAttributesForWCI) -> i32 {
                if way.traffic_signals_exists == Some(true) {
                    2
                } else if way.stops_exists == Some(true) {
                    1
                } else {
                    0
                }
            }

            // Final Score: Speed + Sidewalk + Signal + Stop + Cycle
            let final_score = speed_score(&self)
                + sidewalk_score(&self)
                + cycleway_score(&self)
                + signal_or_stop_score(&self);
            Some(final_score)
        }
    }
}

/// determines if the road is eligible for walking comfort index calculation
/// if one is true: has sidewalk, has footway, has correct highway type, or adjacent sidewalk
fn walk_eligible(
    rtree: &RTree<WayGeometryData>,
    geo_data: &WayGeometryData,
    query_pointf32: [f32; 2],
    sidewalk: bool,
    foot: bool,
) -> bool {
    let this_highway: Highway = geo_data.data.highway.clone();

    if sidewalk || foot {
        return true;
    } else if matches!(
        this_highway,
        Highway::Residential
            | Highway::Unclassified
            | Highway::LivingStreet
            | Highway::Service
            | Highway::Pedestrian
            | Highway::Trailhead
            | Highway::Track
            | Highway::Footway
            | Highway::Bridleway
            | Highway::Steps
            | Highway::Corridor
            | Highway::Path
            | Highway::Elevator
    ) {
        return true;
    } else {
        // check for adjacent sidewalks
        for neighbor in rtree.locate_within_distance(query_pointf32, 0.0001378) {
            if let Some(ref sidewalk) = neighbor.data.sidewalk {
                if sidewalk != "no" && sidewalk != "none" {
                    return true;
                }
            } // could also be neighboring footway=sidewalk
            if neighbor.data.footway == Some("sidewalk".to_string()) {
                return true;
            }
        }
    }
    false
}
