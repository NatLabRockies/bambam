use std::{collections::HashMap, path::Path};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    consts::GLOBAL_AVG_SPEED_KEY, error::NetworkImportError, serialize_ops::VectorizedEdgeList,
    taxonomy::CliBoundingBox,
};

/// Summarizes an imported road network (`summary.toml`).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NetworkGraphSummary {
    /// Information describing how this dataset was generated.
    pub source: NetworkGraphSource,
    /// Statistics gathered on the graph dataset.
    pub stats: NetworkGraphStats,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct NetworkGraphSource {
    /// Location or release of the imported dataset.
    pub release: String,
    /// User-provided name for the network / study region.
    pub study_region: String,
    /// Date and time this network was created (RFC 3339).
    pub created: String,
    /// Bounding box query used when run.
    pub bbox: Option<CliBoundingBox>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct NetworkGraphStats {
    /// Number of vertices in the network.
    pub vertices: usize,
    /// Details for each mode-specific edge list.
    pub edge_list: IndexMap<String, EdgeListStats>,
}

impl NetworkGraphSource {
    pub fn new(release: &str, study_region: &str, bbox: Option<&CliBoundingBox>) -> Self {
        let created = chrono::Utc::now().to_rfc3339();
        Self {
            release: release.to_string(),
            study_region: study_region.to_string(),
            created,
            bbox: bbox.cloned(),
        }
    }
}

/// Writes `summary.toml` into `output_directory`.
pub fn write_summary(
    output_directory: &Path,
    summary: &NetworkGraphSummary,
) -> Result<(), NetworkImportError> {
    let summary_toml = toml::to_string_pretty(summary).map_err(|e| {
        NetworkImportError::InternalError(format!("failure serializing summary TOML: {e}"))
    })?;
    let summary_path = output_directory.join("summary.toml");
    std::fs::write(&summary_path, &summary_toml).map_err(|e| {
        NetworkImportError::CsvWriteError(format!(
            "failed to write '{}': {e}",
            summary_path.display()
        ))
    })
}

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
