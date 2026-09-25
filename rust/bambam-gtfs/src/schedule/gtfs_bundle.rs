use std::collections::{HashMap, HashSet};

use routee_compass_core::model::network::{EdgeId, VertexId};

use crate::schedule::{date::DateMapping, GtfsEdge};

/// the result of processing one GTFS archive for Compass
pub struct GtfsBundle {
    pub edges: Vec<GtfsEdge>,
    pub metadata: serde_json::Value,
    pub date_mapping: HashSet<DateMapping>,
}

impl Default for GtfsBundle {
    fn default() -> Self {
        Self::empty()
    }
}

impl GtfsBundle {
    /// create an empty bundle.
    pub fn empty() -> Self {
        Self {
            edges: vec![],
            metadata: serde_json::Value::Null,
            date_mapping: HashSet::new(),
        }
    }

    /// true if no GTFS edges were created or if no schedules were recorded
    /// for any edges in this GTFS bundle.
    pub fn is_empty(&self) -> bool {
        for edge in self.edges.iter() {
            if !edge.schedules.is_empty() {
                return false;
            }
        }
        true
    }

    /// mutably extends this bundle by consuming the rhs bundle,
    /// accumulating edges, merging metadata, and taking the union of date mappings.
    /// NOTE: does not renumber edge IDs; call `consolidate_and_renumber` after all
    /// bundles have been added.
    pub fn extend(&mut self, rhs: GtfsBundle) {
        self.edges.extend(rhs.edges);
        self.date_mapping.extend(rhs.date_mapping);
        merge_metadata(&mut self.metadata, &rhs.metadata);
    }

    /// combines edges between matching (src_vertex_id, dst_vertex_id) pairs,
    /// sorts edges deterministically by (src_vertex_id, dst_vertex_id), and assigns dense
    /// sequential EdgeIds (0..total_edges - 1), updating all child schedule rows to match the new edge_id.
    pub fn consolidate_and_renumber(&mut self) {
        if self.edges.is_empty() {
            return;
        }

        // 1. Group edges by (src_vertex_id, dst_vertex_id)
        let mut edge_map: HashMap<(VertexId, VertexId), GtfsEdge> = HashMap::new();
        for edge in self.edges.drain(..) {
            let key = (edge.edge.src_vertex_id, edge.edge.dst_vertex_id);
            match edge_map.entry(key) {
                std::collections::hash_map::Entry::Vacant(v) => {
                    v.insert(edge);
                }
                std::collections::hash_map::Entry::Occupied(mut o) => {
                    o.get_mut().schedules.extend(edge.schedules);
                }
            }
        }

        // 2. Sort edges deterministically by (src_vertex_id, dst_vertex_id)
        let mut sorted_edges = edge_map.into_values().collect::<Vec<_>>();
        sorted_edges.sort_by_key(|e| (e.edge.src_vertex_id.0, e.edge.dst_vertex_id.0));

        // 3. Renumber edge_ids sequentially starting from 0, updating schedules
        for (idx, gtfs_edge) in sorted_edges.iter_mut().enumerate() {
            gtfs_edge.edge.edge_id = EdgeId(idx);
            gtfs_edge
                .schedules
                .sort_by_key(|s| (s.src_departure_time, s.dst_arrival_time));
            for schedule in gtfs_edge.schedules.iter_mut() {
                schedule.edge_id = idx;
            }
        }

        self.edges = sorted_edges;
    }

    /// merges an iterator of bundles into a single bundle with consolidated edges
    /// and dense sequential edge IDs.
    pub fn merge_all(bundles: impl IntoIterator<Item = GtfsBundle>) -> GtfsBundle {
        let mut merged = GtfsBundle::empty();
        for bundle in bundles {
            merged.extend(bundle);
        }
        merged.consolidate_and_renumber();
        merged
    }
}

/// Helper function to merge source metadata into target metadata.
fn merge_metadata(target: &mut serde_json::Value, source: &serde_json::Value) {
    if target.is_null() {
        *target = source.clone();
        return;
    }
    if source.is_null() {
        return;
    }

    // Normalize if wrapped in a single-element array (from json! [{ ... }])
    if let Some(arr) = target.as_array_mut() {
        if arr.len() == 1 && arr[0].is_object() {
            *target = arr.remove(0);
        }
    }
    let source_obj = if let Some(arr) = source.as_array() {
        if arr.len() == 1 && arr[0].is_object() {
            arr[0].as_object()
        } else {
            source.as_object()
        }
    } else {
        source.as_object()
    };

    if let (Some(target_obj), Some(source_obj)) = (target.as_object_mut(), source_obj) {
        for (key, source_val) in source_obj {
            match key.as_str() {
                "agencies" | "feed_info" => {
                    if let Some(target_arr) = target_obj.get_mut(key).and_then(|v| v.as_array_mut())
                    {
                        if let Some(source_arr) = source_val.as_array() {
                            target_arr.extend(source_arr.clone());
                        }
                    } else {
                        target_obj.insert(key.clone(), source_val.clone());
                    }
                }
                "read_duration" => {
                    let s_secs = target_obj
                        .get("read_duration")
                        .and_then(|d| d.get("secs"))
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    let s_nanos = target_obj
                        .get("read_duration")
                        .and_then(|d| d.get("nanos"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let o_secs = source_val.get("secs").and_then(|v| v.as_i64()).unwrap_or(0);
                    let o_nanos = source_val
                        .get("nanos")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let total_nanos = s_nanos + o_nanos;
                    let carry_secs = (total_nanos / 1_000_000_000) as i64;
                    let rem_nanos = (total_nanos % 1_000_000_000) as u32;
                    let total_secs = s_secs + o_secs + carry_secs;
                    target_obj.insert(
                        "read_duration".to_string(),
                        serde_json::json!({
                            "secs": total_secs,
                            "nanos": rem_nanos,
                        }),
                    );
                }
                "calendar" => {
                    if let Some(target_cal) = target_obj
                        .get_mut("calendar")
                        .and_then(|v| v.as_object_mut())
                    {
                        if let Some(source_cal) = source_val.as_object() {
                            for (cal_k, cal_v) in source_cal {
                                target_cal.insert(cal_k.clone(), cal_v.clone());
                            }
                        }
                    } else {
                        target_obj.insert(key.clone(), source_val.clone());
                    }
                }
                "calendar_dates" => {
                    if let Some(target_cd) = target_obj
                        .get_mut("calendar_dates")
                        .and_then(|v| v.as_object_mut())
                    {
                        if let Some(source_cd) = source_val.as_object() {
                            for (cd_k, cd_v) in source_cd {
                                match (
                                    target_cd.get_mut(cd_k).and_then(|v| v.as_array_mut()),
                                    cd_v.as_array(),
                                ) {
                                    (Some(t_arr), Some(s_arr)) => {
                                        t_arr.extend(s_arr.clone());
                                    }
                                    _ => {
                                        target_cd.insert(cd_k.clone(), cd_v.clone());
                                    }
                                }
                            }
                        }
                    } else {
                        target_obj.insert(key.clone(), source_val.clone());
                    }
                }
                _ => {
                    if !target_obj.contains_key(key) {
                        target_obj.insert(key.clone(), source_val.clone());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::ScheduleRow;
    use chrono::{NaiveDate, NaiveDateTime};
    use geo::LineString;
    use routee_compass_core::model::network::EdgeConfig;

    fn make_test_edge(
        src: usize,
        dst: usize,
        edge_id: usize,
        feed_id: &str,
        route_id: &str,
    ) -> GtfsEdge {
        let edge = EdgeConfig {
            edge_id: EdgeId(edge_id),
            src_vertex_id: VertexId(src),
            dst_vertex_id: VertexId(dst),
            distance: 100.0,
        };
        let geometry = LineString::from(vec![(0.0, 0.0), (1.0, 1.0)]);
        let mut gtfs_edge = GtfsEdge::new(edge, geometry);
        let dt = NaiveDateTime::parse_from_str("2025-01-01 08:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        gtfs_edge.add_schedule(ScheduleRow::new(
            edge_id,
            Some(feed_id.to_string()),
            route_id.to_string(),
            "service_1".to_string(),
            Some("agency_1".to_string()),
            dt,
            dt,
        ));
        gtfs_edge
    }

    #[test]
    fn test_merge_all_consolidates_and_renumbers_dense_ids() {
        // Bundle 1 has edge (1 -> 2) with edge_id 0 and edge (2 -> 3) with edge_id 1
        let bundle1 = GtfsBundle {
            edges: vec![
                make_test_edge(1, 2, 0, "feed_a", "route_1"),
                make_test_edge(2, 3, 1, "feed_a", "route_2"),
            ],
            metadata: serde_json::json!({
                "agencies": [{"id": "agency_a"}],
                "feed_info": [{"publisher": "pub_a"}],
                "read_duration": {"secs": 1, "nanos": 500_000_000},
                "calendar": {"serv_a": "cal_a"},
                "calendar_dates": {"serv_a": ["date_a1"]},
            }),
            date_mapping: HashSet::from([DateMapping {
                feed_id: Some("feed_a".to_string()),
                agency_id: Some("agency_a".to_string()),
                route_id: "route_1".to_string(),
                service_id: "serv_a".to_string(),
                target_date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
                picked_date: NaiveDate::from_ymd_opt(2025, 1, 2).unwrap(),
            }]),
        };

        // Bundle 2 shares edge (1 -> 2) but with edge_id 0, plus a new edge (3 -> 4) with edge_id 1
        let bundle2 = GtfsBundle {
            edges: vec![
                make_test_edge(1, 2, 0, "feed_b", "route_x"),
                make_test_edge(3, 4, 1, "feed_b", "route_y"),
            ],
            metadata: serde_json::json!({
                "agencies": [{"id": "agency_b"}],
                "feed_info": [{"publisher": "pub_b"}],
                "read_duration": {"secs": 2, "nanos": 700_000_000},
                "calendar": {"serv_b": "cal_b"},
                "calendar_dates": {"serv_b": ["date_b1"]},
            }),
            date_mapping: HashSet::from([DateMapping {
                feed_id: Some("feed_b".to_string()),
                agency_id: Some("agency_b".to_string()),
                route_id: "route_x".to_string(),
                service_id: "serv_b".to_string(),
                target_date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
                picked_date: NaiveDate::from_ymd_opt(2025, 1, 3).unwrap(),
            }]),
        };

        let merged = GtfsBundle::merge_all(vec![bundle1, bundle2]);

        // Total unique vertex pairs: (1, 2), (2, 3), (3, 4) => exactly 3 edges
        assert_eq!(merged.edges.len(), 3);

        // Edge IDs must be dense: 0, 1, 2
        for (expected_id, edge) in merged.edges.iter().enumerate() {
            assert_eq!(edge.edge.edge_id, EdgeId(expected_id));
            for schedule in edge.schedules.iter() {
                assert_eq!(schedule.edge_id, expected_id);
            }
        }

        // Edge (1 -> 2) was merged and should have 2 schedules from feed_a and feed_b
        let edge_1_2 = &merged.edges[0];
        assert_eq!(edge_1_2.edge.src_vertex_id, VertexId(1));
        assert_eq!(edge_1_2.edge.dst_vertex_id, VertexId(2));
        assert_eq!(edge_1_2.schedules.len(), 2);
        assert_eq!(edge_1_2.schedules[0].feed_id.as_deref(), Some("feed_a"));
        assert_eq!(edge_1_2.schedules[1].feed_id.as_deref(), Some("feed_b"));

        // Date mappings must contain both entries
        assert_eq!(merged.date_mapping.len(), 2);

        // Metadata check
        let agencies = merged.metadata["agencies"].as_array().unwrap();
        assert_eq!(agencies.len(), 2);
        let duration = &merged.metadata["read_duration"];
        assert_eq!(duration["secs"], 4); // 1 + 2 + 1 carry from 1.2s nanos
        assert_eq!(duration["nanos"], 200_000_000);
        assert!(merged.metadata["calendar"].get("serv_a").is_some());
        assert!(merged.metadata["calendar"].get("serv_b").is_some());
    }
}
