use std::collections::HashMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{consts::GLOBAL_AVG_SPEED_KEY, serialize_ops::VectorizedEdgeList};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct EdgeListStats {
    /// number of edges in the network
    pub edges: usize,
    /// sum of all miles of roadways
    pub miles: f64,
    /// average speed of all segments in this edge list
    pub avg_speed_mph: Option<f64>,
    /// count and mileage of roadways by road class
    pub road_class_stats: IndexMap<String, ClassStats>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct ClassStats {
    /// number of segments
    pub count: usize,
    /// total miles of counted segments
    pub distance_miles: f64,
    /// average speed observed over the counted segments
    pub avg_speed_mph: Option<f64>,
}

struct ClassStatsAcc {
    pub count: usize,
    pub sum_distance: uom::si::f64::Length,
}

impl EdgeListStats {
    pub fn from_edge_list<C, F>(value: &VectorizedEdgeList<C>, class_to_str: F) -> Self
    where
        F: Fn(&C) -> String,
    {
        let edges = value.edges.len();
        let miles = if edges == 0 {
            0.0
        } else {
            value
                .edges
                .0
                .iter()
                .map(|e| e.distance.get::<uom::si::length::mile>())
                .sum()
        };

        let mut class_stats_accumulators: HashMap<String, ClassStatsAcc> = HashMap::new();
        let edge_iter = value.edges.0.iter().zip(value.classes.iter());
        for (edge, class_val) in edge_iter {
            let road_class = class_to_str(class_val);
            match class_stats_accumulators.get_mut(&road_class) {
                Some(cnt) => {
                    cnt.add(edge.distance);
                }
                None => {
                    let acc = ClassStatsAcc::new(edge.distance);
                    class_stats_accumulators.insert(road_class, acc);
                }
            }
        }
        let road_class_stats: IndexMap<String, ClassStats> = class_stats_accumulators
            .into_iter()
            .map(|(k, v)| {
                let avg_speed = value.speed_lookup.get(&k).cloned();
                (k, ClassStats::new(v, avg_speed))
            })
            .collect();
        let avg_speed_mph = value.speed_lookup.get(GLOBAL_AVG_SPEED_KEY).cloned();

        Self {
            edges,
            miles,
            road_class_stats,
            avg_speed_mph,
        }
    }
}

impl ClassStats {
    fn new(acc: ClassStatsAcc, avg_speed_mph: Option<f64>) -> Self {
        Self {
            count: acc.count,
            distance_miles: acc.sum_distance.get::<uom::si::length::mile>(),
            avg_speed_mph,
        }
    }
}

impl ClassStatsAcc {
    pub fn new(distance: uom::si::f64::Length) -> Self {
        Self {
            count: 1,
            sum_distance: distance,
        }
    }
    pub fn add(&mut self, distance: uom::si::f64::Length) {
        self.count += 1;
        self.sum_distance += distance;
    }
}
