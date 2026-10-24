pub use bambam_network::vertex_serializable::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::InfrastructureRecord;
    use bambam_modal_metrics::network_traits::vertex_for_modal_metric::VertexForModalMetric;
    use geo::{Geometry, Point};
    use routee_compass_core::model::network::Vertex;

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
