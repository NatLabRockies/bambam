use routee_compass_core::{
    model::{constraint::ConstraintModelError, network::Vertex},
    util::geo::PolygonalRTree,
};

use crate::model::constraint::geofence::GeofenceConstraintConfig;

// whatever the type of GeofenceId should be
type GeofenceId = String;

pub struct GeofenceConstraintEngine {
    pub config: GeofenceConstraintConfig,
    pub rtree: PolygonalRTree<f32, GeofenceId>,
}

impl GeofenceConstraintEngine {
    pub fn new(
        config: GeofenceConstraintConfig,
        rtree: PolygonalRTree<f32, GeofenceId>,
    ) -> GeofenceConstraintEngine {
        GeofenceConstraintEngine { config, rtree }
    }

    pub fn in_geofence(
        &self,
        vertex: &Vertex,
        geofence_id: &str,
    ) -> Result<bool, ConstraintModelError> {
        let pt = geo::Geometry::Point(geo::Point::new(vertex.x(), vertex.y()));
        let mut iter = self.rtree.intersection(&pt).map_err(|e| {
            ConstraintModelError::ConstraintModelError(format!(
                "failure checking geofence for {:?}: {e}",
                vertex.coordinate.x_y()
            ))
        })?;
        match iter.next() {
            Some(boundary) => {
                let result = boundary.data == geofence_id;
                Ok(result)
            }
            _ => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use routee_compass_core::{model::network::Vertex, util::geo::PolygonalRTree};

    use crate::model::constraint::geofence::GeofenceConstraintConfig;

    use super::GeofenceConstraintEngine;

    #[test]
    fn test_in_geofence() {
        let config = GeofenceConstraintConfig {};
        let polygon = geo::Geometry::Polygon(geo::Polygon::new(
            geo::line_string![
                (0.0, 0.0).into(),
                (1.0, 0.0).into(),
                (1.0, 1.0).into(),
                (0.0, 1.0).into(),
                (0.0, 0.0).into()
            ],
            vec![],
        ));
        let rtree = PolygonalRTree::new(vec![(polygon, "zone 1".to_string())])
            .expect("test invariant failed: could not build Rtree");
        let engine = GeofenceConstraintEngine::new(config, rtree);
        let vertex = Vertex::new(0, 0.5, 0.5);
        let result = engine.in_geofence(&vertex, "zone 1");
        assert!(result.is_ok());
        assert!(result.unwrap());
    }
}
