use bambam_modal_metrics::network_traits::vertex_for_modal_metric::VertexForModalMetric;
use geo::{Distance, Haversine, Point};
use routee_compass_core::model::network::Vertex;
use rstar::{RTree, RTreeObject, AABB};
use serde::{Deserialize, Serialize};

use crate::collection::{record::SegmentHeading, InfrastructureRecord};

/// Default maximum Haversine distance (in meters) to snap an Overture infrastructure
/// feature to its nearest connector vertex.
pub const DEFAULT_INFRASTRUCTURE_MATCH_DISTANCE_METERS: f64 = 15.0;

/// Delimiter used when joining multiple matched infrastructure GERS IDs on a single vertex.
pub const INFRASTRUCTURE_ID_DELIMITER: &str = ";";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VertexSerializable {
    pub vertex_id: usize,
    pub x: f32,
    pub y: f32,
}

impl From<Vertex> for VertexSerializable {
    fn from(value: Vertex) -> Self {
        Self {
            vertex_id: value.vertex_id.0,
            x: value.x(),
            y: value.y(),
        }
    }
}

/// Complete vertex record including OMF connector ID and spatially attached
/// infrastructure attributes (`vertices-complete.csv.gz`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OmfVertexComplete {
    pub vertex_id: usize,
    pub connector_id: String,
    pub x: f32,
    pub y: f32,
    pub traffic_signals: bool,
    pub stop: bool,
    pub give_way: bool,
    pub crossing: bool,
    pub infrastructure_ids: Option<String>,
}

impl VertexForModalMetric for OmfVertexComplete {
    fn has_traffic_signals(&self) -> bool {
        self.traffic_signals
    }

    fn has_stop_sign(&self) -> bool {
        self.stop
    }
}

/// Standardized mapping between a RouteE Compass `vertex_id` and its source OMF Connector GERS ID
/// (`vertices-mapping.csv.gz`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VertexMappingRecord {
    pub vertex_id: usize,
    pub source_id: String,
}

/// Standardized mapping between a RouteE Compass `edge_id` and its source OMF Segment GERS ID,
/// linear reference bounds, and travel heading (`<mode>/edges-mapping.csv.gz`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeMappingRecord {
    pub edge_id: usize,
    pub source_id: String,
    pub start_lr: f64,
    pub end_lr: f64,
    pub heading: SegmentHeading,
}

#[derive(Clone, Debug)]
struct VertexRTreeNode {
    vertex_id: usize,
    point: Point<f32>,
}

impl RTreeObject for VertexRTreeNode {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_point([self.point.x() as f64, self.point.y() as f64])
    }
}

/// Spatially matches Overture `InfrastructureRecord` items to their nearest connector vertex
/// within `max_distance_meters` (using Haversine distance) and builds a complete vertex table.
pub fn attach_infrastructure_to_vertices(
    vertices: &[Vertex],
    connector_ids: &[String],
    infrastructure: &[InfrastructureRecord],
    max_distance_meters: f64,
) -> Vec<OmfVertexComplete> {
    let mut complete_vertices: Vec<OmfVertexComplete> = vertices
        .iter()
        .zip(connector_ids.iter())
        .map(|(v, conn_id)| OmfVertexComplete {
            vertex_id: v.vertex_id.0,
            connector_id: conn_id.clone(),
            x: v.x(),
            y: v.y(),
            traffic_signals: false,
            stop: false,
            give_way: false,
            crossing: false,
            infrastructure_ids: None,
        })
        .collect();

    if complete_vertices.is_empty() || infrastructure.is_empty() {
        return complete_vertices;
    }

    let rtree_nodes: Vec<VertexRTreeNode> = vertices
        .iter()
        .map(|v| VertexRTreeNode {
            vertex_id: v.vertex_id.0,
            point: Point::new(v.x(), v.y()),
        })
        .collect();
    let rtree = RTree::bulk_load(rtree_nodes);

    let mut matched_ids_per_vertex: Vec<Vec<String>> = vec![Vec::new(); complete_vertices.len()];

    // Minimum meters per degree of latitude (~110.574 km at equator)
    const METERS_PER_DEGREE_LAT: f64 = 110_574.0;

    for infra in infrastructure {
        if !infra.is_traffic_signals()
            && !infra.is_stop()
            && !infra.is_give_way()
            && !infra.is_crossing()
        {
            continue;
        }
        let Some(pt) = infra.get_reference_point() else {
            continue;
        };

        let lat_rad = (pt.y() as f64).to_radians();
        let cos_lat = lat_rad.cos().abs().max(0.01);
        let d_lat = (max_distance_meters / METERS_PER_DEGREE_LAT) * 1.1;
        let d_lon = (max_distance_meters / (METERS_PER_DEGREE_LAT * cos_lat)) * 1.1;

        let envelope = AABB::from_corners(
            [pt.x() as f64 - d_lon, pt.y() as f64 - d_lat],
            [pt.x() as f64 + d_lon, pt.y() as f64 + d_lat],
        );

        let nearest = rtree
            .locate_in_envelope(&envelope)
            .filter_map(|node| {
                let dist_m = Haversine.distance(pt, node.point) as f64;
                if dist_m <= max_distance_meters {
                    Some((node.vertex_id, dist_m))
                } else {
                    None
                }
            })
            .min_by(|(id_a, dist_a), (id_b, dist_b)| {
                dist_a
                    .partial_cmp(dist_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| id_a.cmp(id_b))
            });

        if let Some((vertex_id, _)) = nearest {
            if let Some(vertex_complete) = complete_vertices.get_mut(vertex_id) {
                if infra.is_traffic_signals() {
                    vertex_complete.traffic_signals = true;
                }
                if infra.is_stop() {
                    vertex_complete.stop = true;
                }
                if infra.is_give_way() {
                    vertex_complete.give_way = true;
                }
                if infra.is_crossing() {
                    vertex_complete.crossing = true;
                }
                if !infra.id.is_empty() {
                    matched_ids_per_vertex[vertex_id].push(infra.id.clone());
                }
            }
        }
    }

    for (vertex_complete, ids) in complete_vertices.iter_mut().zip(matched_ids_per_vertex) {
        if !ids.is_empty() {
            vertex_complete.infrastructure_ids = Some(ids.join(INFRASTRUCTURE_ID_DELIMITER));
        }
    }

    complete_vertices
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::Geometry;

    #[test]
    fn test_attach_infrastructure_within_15m_tolerance() {
        // Two vertices in Boulder, CO (~40.015 N, -105.2705 W)
        let v0 = Vertex::new(0, -105.2705, 40.015);
        let v1 = Vertex::new(1, -105.272, 40.015);
        let vertices = vec![v0, v1];
        let connector_ids = vec!["conn-0".to_string(), "conn-1".to_string()];

        // 1. Traffic signal ~5m east of v0 (0.00005 deg lon at 40N is ~4.2m)
        let sig = InfrastructureRecord {
            id: "infra-sig-1".to_string(),
            geometry: Some(Geometry::Point(Point::new(-105.27045, 40.015))),
            subtype: Some("transportation".to_string()),
            class: Some("traffic_signals".to_string()),
            ..Default::default()
        };

        // 2. Pedestrian crossing ~8m north of v0 (0.00007 deg lat is ~7.8m)
        let crossing = InfrastructureRecord {
            id: "infra-cross-1".to_string(),
            geometry: Some(Geometry::Point(Point::new(-105.2705, 40.01507))),
            subtype: Some("pedestrian".to_string()),
            class: Some("pedestrian_crossing".to_string()),
            ..Default::default()
        };

        // 3. Stop sign ~6m south of v1
        let stop = InfrastructureRecord {
            id: "infra-stop-1".to_string(),
            geometry: Some(Geometry::Point(Point::new(-105.272, 40.01495))),
            subtype: Some("transportation".to_string()),
            class: Some("stop".to_string()),
            ..Default::default()
        };

        // 4. Give way sign ~35m away from v0 (0.00035 deg lat is ~38.9m > 15m tolerance)
        let far_give_way = InfrastructureRecord {
            id: "infra-far-1".to_string(),
            geometry: Some(Geometry::Point(Point::new(-105.2705, 40.01535))),
            subtype: Some("transportation".to_string()),
            class: Some("give_way".to_string()),
            ..Default::default()
        };

        let attached = attach_infrastructure_to_vertices(
            &vertices,
            &connector_ids,
            &[sig, crossing, stop, far_give_way],
            DEFAULT_INFRASTRUCTURE_MATCH_DISTANCE_METERS,
        );

        assert_eq!(attached.len(), 2);

        // v0 should have traffic_signals and crossing, but not stop or give_way
        assert!(attached[0].traffic_signals);
        assert!(attached[0].has_traffic_signals());
        assert!(!attached[0].stop);
        assert!(!attached[0].has_stop_sign());
        assert!(!attached[0].give_way);
        assert!(attached[0].crossing);
        assert_eq!(
            attached[0].infrastructure_ids.as_deref(),
            Some("infra-sig-1;infra-cross-1")
        );

        // v1 should have stop only
        assert!(!attached[1].traffic_signals);
        assert!(attached[1].stop);
        assert!(attached[1].has_stop_sign());
        assert!(!attached[1].give_way);
        assert!(!attached[1].crossing);
        assert_eq!(
            attached[1].infrastructure_ids.as_deref(),
            Some("infra-stop-1")
        );
    }
}
