use std::{collections::HashSet, fmt, sync::Arc};

use bambam_network::{CliBoundingBox, GeographicExtent};
use geo::{BoundingRect, Centroid, Geometry, Intersects, LineString};
use rayon::prelude::*;
use routee_compass_core::{
    model::{map::SpatialIndex, network::Vertex},
    util::geo::haversine::APPROX_EARTH_RADIUS_M,
};

use super::Bbox;
use crate::collection::{
    OvertureMapsCollectionError, TransportationCollection, TransportationSegmentRecord,
};

/// Default Haversine distance tolerance (in meters) when matching Overture geometries
/// to road network vertices.
pub const DEFAULT_VERTEX_MATCH_TOLERANCE_METERS: f64 = 200.0;

/// Two-stage spatial filter for Overture Maps datasets (both Transportation networks
/// and Places/Buildings opportunities):
///
/// 1. **Stage 1 (Parquet pushdown)**: [`OvertureSpatialFilter::bbox`] provides a [`Bbox`]
///    used by [`super::RowFilterConfig::Bbox`] to prune GeoParquet row groups and rows before
///    decoding full geometries.
/// 2. **Stage 2 (In-memory geometry refinement)**: [`OvertureSpatialFilter::matches_geometry`]
///    evaluates exact polygon/multipolygon extent intersection and/or road network vertex
///    proximity via [`SpatialIndex`].
#[derive(Clone)]
pub enum OvertureSpatialFilter {
    /// Stage 1: Parquet bbox pushdown from the extent's bounding rect.
    /// Stage 2: Polygon/MultiPolygon intersection (`extent.intersects(geom)`).
    Extent {
        bbox: Bbox,
        extent: Arc<Geometry<f32>>,
    },
    /// Stage 1: Parquet bbox pushdown from the graph vertices' bounding rect (buffered by tolerance).
    /// Stage 2: Graph vertex proximity via `SpatialIndex::nearest_graph_id` (plus optional extent intersection).
    Graph {
        bbox: Bbox,
        spatial_index: Arc<SpatialIndex>,
        extent: Option<Arc<Geometry<f32>>>,
    },
}

impl fmt::Debug for OvertureSpatialFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Extent { bbox, extent } => f
                .debug_struct("OvertureSpatialFilter::Extent")
                .field("bbox", bbox)
                .field("extent", extent)
                .finish(),
            Self::Graph { bbox, extent, .. } => f
                .debug_struct("OvertureSpatialFilter::Graph")
                .field("bbox", bbox)
                .field("extent", extent)
                .finish_non_exhaustive(),
        }
    }
}

impl OvertureSpatialFilter {
    /// Builds an [`OvertureSpatialFilter::Extent`] from a [`GeographicExtent`]
    /// (`Bbox`, inline `Wkt`, or `WktFile`).
    pub fn from_extent(extent: &GeographicExtent) -> Result<Self, OvertureMapsCollectionError> {
        let (geom, cli_bbox) = extent
            .to_geometry_and_bbox()
            .map_err(OvertureMapsCollectionError::InvalidUserInput)?;
        Ok(Self::from_geometry_and_bbox(geom, &cli_bbox))
    }

    /// Builds an [`OvertureSpatialFilter::Extent`] from an already-parsed extent geometry
    /// and its bounding [`CliBoundingBox`].
    pub fn from_geometry_and_bbox(extent: Geometry<f32>, cli_bbox: &CliBoundingBox) -> Self {
        Self::Extent {
            bbox: Bbox::from(cli_bbox),
            extent: Arc::new(extent),
        }
    }

    /// Builds an [`OvertureSpatialFilter::Graph`] from a slice of RouteE Compass [`Vertex`]
    /// records, an optional Haversine distance tolerance in meters (defaults to
    /// [`DEFAULT_VERTEX_MATCH_TOLERANCE_METERS`]), and an optional [`GeographicExtent`].
    ///
    /// Since a Graph filter includes both a lat/lon bbox and an rtree of points, we ensure
    /// that, from the arguments, we have full agreement with the resulting rtree and bbox
    /// including some padding so that vertices are included within the extent.
    pub fn from_vertices(
        vertices: &[Vertex],
        tolerance_meters: Option<f64>,
        extent: Option<&GeographicExtent>,
    ) -> Result<Self, OvertureMapsCollectionError> {
        if vertices.is_empty() {
            return Err(OvertureMapsCollectionError::InvalidUserInput(
                "cannot construct OvertureSpatialFilter::Graph from an empty vertex slice"
                    .to_string(),
            ));
        }

        let tol_m = tolerance_meters.unwrap_or(DEFAULT_VERTEX_MATCH_TOLERANCE_METERS);
        if tol_m < 0.0 || !tol_m.is_finite() {
            return Err(OvertureMapsCollectionError::InvalidUserInput(format!(
                "vertex_match_tolerance_meters must be a non-negative finite number, got {tol_m}"
            )));
        }

        // Convert the distance to an angular radius using the same Earth radius as
        // the matcher. Add a margin for its f32 Haversine arithmetic; cap at half
        // the circumference, beyond which every point on Earth is in range.
        let angular_radius = (tol_m / f64::from(APPROX_EARTH_RADIUS_M)
            + 8.0 * f64::from(f32::EPSILON))
        .min(std::f64::consts::PI);
        let lat_pad = angular_radius.to_degrees();
        let mut bounds = Bbox::new(180.0, -180.0, 90.0, -90.0);

        for v in vertices {
            let coord = v.coordinate.0;
            if !(-180.0..=180.0).contains(&coord.x) || !(-90.0..=90.0).contains(&coord.y) {
                return Err(OvertureMapsCollectionError::InvalidUserInput(format!(
                    "vertex has invalid longitude or latitude: {coord:?}"
                )));
            }
            bounds.xmin = bounds.xmin.min(coord.x);
            bounds.xmax = bounds.xmax.max(coord.x);
            bounds.ymin = bounds.ymin.min(coord.y);
            bounds.ymax = bounds.ymax.max(coord.y);
        }

        // Longitude degrees get narrower toward the poles. Use the most poleward
        // vertex for the padding of every vertex, rather than the graph's midpoint.
        let poleward_lat = f64::from(bounds.ymin.abs().max(bounds.ymax.abs()));
        let (xmin, xmax) = if poleward_lat + lat_pad >= 90.0 {
            // A tolerance region reaching a pole can include any longitude.
            (-180.0, 180.0)
        } else {
            // Maximum longitude displacement of a spherical cap that misses the poles.
            let lon_pad = (angular_radius.sin() / poleward_lat.to_radians().cos())
                .clamp(-1.0, 1.0)
                .asin()
                .to_degrees();
            let xmin = f64::from(bounds.xmin) - lon_pad;
            let xmax = f64::from(bounds.xmax) + lon_pad;
            if xmin <= -180.0 || xmax >= 180.0 {
                // A single Bbox cannot represent a wrapped interval. Keep both
                // sides of the antimeridian by allowing all longitudes.
                (-180.0, 180.0)
            } else {
                // Round outward so conversion to f32 cannot shrink the envelope.
                (
                    (xmin as f32).next_down().max(-180.0),
                    (xmax as f32).next_up().min(180.0),
                )
            }
        };
        // Apply the same outward rounding to latitude, clamped to valid coordinates.
        let mut bbox = Bbox::new(
            xmin,
            xmax,
            ((f64::from(bounds.ymin) - lat_pad) as f32)
                .next_down()
                .max(-90.0),
            ((f64::from(bounds.ymax) + lat_pad) as f32)
                .next_up()
                .min(90.0),
        );

        let parsed_extent = if let Some(ext_cfg) = extent {
            let (ext_geom, ext_bbox) = ext_cfg
                .to_geometry_and_bbox()
                .map_err(OvertureMapsCollectionError::InvalidUserInput)?;
            // Both the extent and vertex proximity must match; their bbox
            // intersection is a coarse prefilter before the geometry checks.
            bbox.xmin = bbox.xmin.max(ext_bbox.xmin);
            bbox.xmax = bbox.xmax.min(ext_bbox.xmax);
            bbox.ymin = bbox.ymin.max(ext_bbox.ymin);
            bbox.ymax = bbox.ymax.min(ext_bbox.ymax);
            if bbox.xmin > bbox.xmax || bbox.ymin > bbox.ymax {
                return Err(OvertureMapsCollectionError::InvalidUserInput(format!(
                    "vertices bounding box does not overlap the provided extent: {bbox:?}"
                )));
            }
            Some(Arc::new(ext_geom))
        } else {
            None
        };

        let tolerance_len = uom::si::f64::Length::new::<uom::si::length::meter>(tol_m);
        let spatial_index = Arc::new(SpatialIndex::new_vertex_oriented(
            vertices,
            Some(tolerance_len),
        ));

        Ok(Self::Graph {
            bbox,
            spatial_index,
            extent: parsed_extent,
        })
    }

    /// Returns the Stage 1 [z`Bbox`] used for Parquet row-group and row predicate pushdown.
    pub fn bbox(&self) -> Bbox {
        match self {
            Self::Extent { bbox, .. } | Self::Graph { bbox, .. } => *bbox,
        }
    }

    /// Returns the Stage 1 bounding box as a [`CliBoundingBox`].
    pub fn cli_bbox(&self) -> CliBoundingBox {
        CliBoundingBox::from(self.bbox())
    }

    /// Evaluates the Stage 2 spatial filter on a candidate [`Geometry<f32>`].
    pub fn matches_geometry(&self, geom: &Geometry<f32>) -> bool {
        let Some(rect) = geom.bounding_rect() else {
            return false;
        };

        match self {
            Self::Extent { extent, .. } => extent.intersects(&rect) && extent.intersects(geom),
            Self::Graph {
                spatial_index,
                extent,
                ..
            } => {
                if let Some(ext) = extent {
                    if !ext.intersects(&rect) || !ext.intersects(geom) {
                        return false;
                    }
                }
                let Some(centroid) = geom.centroid() else {
                    return false;
                };
                spatial_index.nearest_graph_id(&centroid).is_ok()
            }
        }
    }

    /// Evaluates the Stage 2 spatial filter on a candidate [`LineString<f32>`] without cloning.
    pub fn matches_linestring(&self, ls: &LineString<f32>) -> bool {
        let Some(rect) = ls.bounding_rect() else {
            return false;
        };

        match self {
            Self::Extent { extent, .. } => extent.intersects(&rect) && extent.intersects(ls),
            Self::Graph {
                spatial_index,
                extent,
                ..
            } => {
                if let Some(ext) = extent {
                    if !ext.intersects(&rect) || !ext.intersects(ls) {
                        return false;
                    }
                }
                let Some(centroid) = ls.centroid() else {
                    return false;
                };
                spatial_index.nearest_graph_id(&centroid).is_ok()
            }
        }
    }

    /// Applies Stage 2 spatial filtering to a [`TransportationCollection`]:
    /// filters segments by geometry, retains only connectors referenced by surviving segments,
    /// and filters infrastructure by geometry.
    pub fn filter_transportation_collection(
        &self,
        collection: TransportationCollection,
    ) -> TransportationCollection {
        log::info!("Started applying spatial filter to OMF segments");
        let filtered_segments: Vec<TransportationSegmentRecord> = collection
            .segments
            .into_par_iter()
            .filter(|segment| match segment.get_linestring() {
                Ok(ls) => self.matches_linestring(ls),
                Err(_) => false,
            })
            .collect();

        log::info!("Collecting all connector IDs from filtered segments");
        let connector_ids = filtered_segments
            .par_iter()
            .flat_map(|segment| {
                segment
                    .connectors
                    .as_ref()
                    .unwrap_or(&vec![])
                    .iter()
                    .map(|con_ref| con_ref.connector_id.clone())
                    .collect::<Vec<String>>()
            })
            .collect::<HashSet<String>>();

        let arc_ids = Arc::new(connector_ids);

        log::info!("Started applying spatial filter to OMF connectors");
        let filtered_connectors = collection
            .connectors
            .into_par_iter()
            .filter(|connector| arc_ids.contains(&connector.id))
            .collect();

        log::info!("Started applying spatial filter to OMF infrastructure");
        let filtered_infrastructure = collection
            .infrastructure
            .into_par_iter()
            .filter(|infra| match infra.get_geometry() {
                Some(geom) => self.matches_geometry(geom),
                None => false,
            })
            .collect();

        TransportationCollection {
            release: collection.release,
            connectors: filtered_connectors,
            segments: filtered_segments,
            infrastructure: filtered_infrastructure,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::Point;

    #[test]
    fn test_overture_spatial_filter_from_extent() {
        // Triangle polygon from (0,0) -> (10,0) -> (0,10)
        // Bounding box is [0, 0, 10, 10], but point (9, 9) lies outside the triangle.
        let extent = GeographicExtent::Wkt {
            wkt: "POLYGON ((0 0, 10 0, 0 10, 0 0))".to_string(),
        };
        let filter = OvertureSpatialFilter::from_extent(&extent).unwrap();

        assert_eq!(filter.bbox(), Bbox::new(0.0, 10.0, 0.0, 10.0));
        // Inside polygon
        assert!(filter.matches_geometry(&Geometry::Point(Point::new(2.0, 2.0))));
        // Inside bbox [0, 10]x[0, 10] but outside triangle polygon (Stage 2 refinement)
        assert!(!filter.matches_geometry(&Geometry::Point(Point::new(9.0, 9.0))));
        // Outside bbox
        assert!(!filter.matches_geometry(&Geometry::Point(Point::new(15.0, 15.0))));
    }

    #[test]
    fn test_overture_spatial_filter_from_vertices() {
        let vertices = vec![
            Vertex::new(0, -105.2705, 40.0150),
            Vertex::new(1, -105.2600, 40.0200),
        ];

        let filter = OvertureSpatialFilter::from_vertices(&vertices, Some(200.0), None).unwrap();
        let bbox = filter.bbox();
        assert!(bbox.xmin < -105.2705 && bbox.xmax > -105.2600);
        assert!(bbox.ymin < 40.0150 && bbox.ymax > 40.0200);

        // Point right on vertex 0 matches
        assert!(filter.matches_geometry(&Geometry::Point(Point::new(-105.2705, 40.0150))));
        // Point far from both vertices (even if inside their axis-aligned bounding box corner if spaced apart) fails Stage 2
        assert!(!filter.matches_geometry(&Geometry::Point(Point::new(-105.2600, 40.0150))));
    }

    #[test]
    fn graph_bbox_contains_poleward_vertex_matches() {
        for latitude in [49.0_f32, -49.0] {
            let vertices = vec![
                Vertex::new(0, -100.0, latitude.signum() * 25.0),
                Vertex::new(1, -100.0, latitude),
            ];
            let filter =
                OvertureSpatialFilter::from_vertices(&vertices, Some(200.0), None).unwrap();
            let point = Point::new(-99.9974, latitude);
            assert!(filter.matches_geometry(&Geometry::Point(point)));
            let bbox = filter.bbox();
            assert!(bbox.xmin <= point.x() && point.x() <= bbox.xmax);
            assert!(bbox.ymin <= point.y() && point.y() <= bbox.ymax);
        }
    }

    #[test]
    fn graph_bbox_wraps_antimeridian_and_covers_poles() {
        for (lon, lat, point) in [
            (179.999, 0.0, Point::new(-179.9995, 0.0)),
            (-179.999, 0.0, Point::new(179.9995, 0.0)),
            (10.0, 89.999, Point::new(-170.0, 89.999)),
            (10.0, -89.999, Point::new(-170.0, -89.999)),
        ] {
            let filter = OvertureSpatialFilter::from_vertices(
                &[Vertex::new(0, lon, lat)],
                Some(300.0),
                None,
            )
            .unwrap();
            let bbox = filter.bbox();
            assert_eq!((bbox.xmin, bbox.xmax), (-180.0, 180.0));
            assert!(bbox.ymin <= point.y() && point.y() <= bbox.ymax);
            assert!(filter.matches_geometry(&Geometry::Point(point)));
        }
    }

    #[test]
    fn graph_bbox_rounds_spherical_limits_outward() {
        let tolerance = 200.0;
        let lat = 40.0_f64;
        let lon = -105.0_f64;
        let radius = tolerance / f64::from(APPROX_EARTH_RADIUS_M);
        let lat_pad = radius.to_degrees();
        let lon_pad = (radius.sin() / lat.to_radians().cos()).asin().to_degrees();
        let filter = OvertureSpatialFilter::from_vertices(
            &[Vertex::new(0, lon as f32, lat as f32)],
            Some(tolerance),
            None,
        )
        .unwrap();
        let bbox = filter.bbox();
        assert!(f64::from(bbox.xmin) <= lon - lon_pad);
        assert!(f64::from(bbox.xmax) >= lon + lon_pad);
        assert!(f64::from(bbox.ymin) <= lat - lat_pad);
        assert!(f64::from(bbox.ymax) >= lat + lat_pad);
    }

    #[test]
    fn graph_bbox_contains_spherical_tolerance_boundaries() {
        for lat in [-89.999_f32, -49.0, 0.0, 49.0, 89.999] {
            for lon in [-179.999_f32, -100.0, 179.999] {
                for tolerance in [0.0, 200.0, 100_000.0] {
                    let filter = OvertureSpatialFilter::from_vertices(
                        &[Vertex::new(0, lon, lat)],
                        Some(tolerance),
                        None,
                    )
                    .unwrap();
                    let bbox = filter.bbox();
                    let radius = tolerance / f64::from(APPROX_EARTH_RADIUS_M);
                    let lat1 = f64::from(lat).to_radians();
                    let lon1 = f64::from(lon).to_radians();
                    for bearing in (0..360).step_by(15) {
                        let bearing = f64::from(bearing).to_radians();
                        let lat2 = (lat1.sin() * radius.cos()
                            + lat1.cos() * radius.sin() * bearing.cos())
                        .asin();
                        let lon2 = lon1
                            + (bearing.sin() * radius.sin() * lat1.cos())
                                .atan2(radius.cos() - lat1.sin() * lat2.sin());
                        let x = (lon2.to_degrees() + 180.0).rem_euclid(360.0) - 180.0;
                        let y = lat2.to_degrees();
                        assert!(
                            f64::from(bbox.xmin) <= x
                                && x <= f64::from(bbox.xmax)
                                && f64::from(bbox.ymin) <= y
                                && y <= f64::from(bbox.ymax),
                            "vertex=({lon}, {lat}), tolerance={tolerance}, point=({x}, {y}), bbox={bbox:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn graph_bbox_rejects_invalid_vertex_coordinates() {
        for (lon, lat) in [(181.0, 0.0), (0.0, -91.0), (f32::NAN, 0.0)] {
            assert!(
                OvertureSpatialFilter::from_vertices(&[Vertex::new(0, lon, lat)], None, None)
                    .is_err()
            );
        }
    }
}
