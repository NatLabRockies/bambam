use std::{collections::HashSet, fmt, sync::Arc};

use bambam_network::{CliBoundingBox, GeographicExtent};
use geo::{BoundingRect, Centroid, Geometry, Intersects, LineString};
use rayon::prelude::*;
use routee_compass_core::model::{map::SpatialIndex, network::Vertex};

use super::Bbox;
use crate::collection::{
    OvertureMapsCollectionError, TransportationCollection, TransportationSegmentRecord,
};

/// Default Haversine distance tolerance (in meters) when matching Overture geometries
/// to road network vertices.
pub const DEFAULT_VERTEX_MATCH_TOLERANCE_METERS: f64 = 200.0;

/// Approximate meters per degree of latitude (WGS84).
const METERS_PER_DEGREE_LAT: f64 = 111_320.0;

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

        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;

        for v in vertices {
            let coord = v.coordinate.0;
            min_x = min_x.min(coord.x);
            max_x = max_x.max(coord.x);
            min_y = min_y.min(coord.y);
            max_y = max_y.max(coord.y);
        }

        let lat_pad = (tol_m / METERS_PER_DEGREE_LAT) as f32;
        let mid_lat_rad = (((min_y + max_y) * 0.5) as f64).to_radians();
        let cos_lat = mid_lat_rad.cos().abs().max(0.01);
        let lon_pad = (tol_m / (METERS_PER_DEGREE_LAT * cos_lat)) as f32;

        let mut bbox = Bbox {
            xmin: (min_x - lon_pad).max(-180.0),
            xmax: (max_x + lon_pad).min(180.0),
            ymin: (min_y - lat_pad).max(-90.0),
            ymax: (max_y + lat_pad).min(90.0),
        };

        let parsed_extent = if let Some(ext_cfg) = extent {
            let (ext_geom, ext_bbox) = ext_cfg
                .to_geometry_and_bbox()
                .map_err(OvertureMapsCollectionError::InvalidUserInput)?;
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

    /// Returns the Stage 1 [`Bbox`] used for Parquet row-group and row predicate pushdown.
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
                Some(geom) => self.matches_geometry(&geom),
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
}
