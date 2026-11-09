pub mod census;
pub mod poi_traffic;

use rayon::prelude::*;
use rstar::RTree;

use crate::{
    config::{ProjectionMode, SpatialFeaturesConfig},
    error::CongestionError,
    features::{
        census::{build_census_rtree, compute_edge_census_densities, CensusPolygonEntry},
        poi_traffic::{
            build_poi_and_traffic_rtree, compute_edge_poi_and_traffic_densities,
            SpatialFeaturePoint,
        },
    },
    network::{LoadedOpenNetwork, OpenNetworkEdge},
    taxonomy::EdgeStaticFeatures,
};

/// Indexed spatial feature engine holding the R*-trees for MEP land-use opportunities,
/// traffic controls, and Census population/employment density polygons.
pub struct SpatialFeatureEngine {
    pub poi_traffic_rtree: RTree<SpatialFeaturePoint>,
    pub census_rtree: RTree<CensusPolygonEntry>,
    pub density_radius_miles: f64,
    pub projection_mode: ProjectionMode,
}

impl SpatialFeatureEngine {
    /// Builds the spatial R*-trees for `open_net` from `cfg`.
    pub fn build(
        open_net: &LoadedOpenNetwork,
        cfg: &SpatialFeaturesConfig,
    ) -> Result<Self, CongestionError> {
        let poi_traffic_rtree = build_poi_and_traffic_rtree(open_net, cfg)?;
        let census_rtree = build_census_rtree(
            cfg.census_source.as_ref(),
            open_net.bbox,
            cfg.density_radius_miles,
            cfg.projection_mode,
        )?;
        Ok(Self {
            poi_traffic_rtree,
            census_rtree,
            density_radius_miles: cfg.density_radius_miles,
            projection_mode: cfg.projection_mode,
        })
    }

    /// Computes `EdgeStaticFeatures` for a single edge.
    ///
    /// If `compute_spatial_densities` is `false` (e.g. for low-functional class roads during
    /// imputation that only use `sl-med-imp`), spatial R*-tree queries are skipped cleanly.
    pub fn compute_edge_features(
        &self,
        edge: &OpenNetworkEdge,
        compute_spatial_densities: bool,
    ) -> EdgeStaticFeatures {
        let (pop_den, emp_den, opp_den, tc_den) = if compute_spatial_densities {
            let (opp, tc) = compute_edge_poi_and_traffic_densities(
                edge,
                &self.poi_traffic_rtree,
                self.density_radius_miles,
                self.projection_mode,
            );
            let (pop, emp) = compute_edge_census_densities(
                edge,
                &self.census_rtree,
                self.density_radius_miles,
            );
            (pop, emp, opp, tc)
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };

        EdgeStaticFeatures {
            model_speed_limit_mph: edge.model_speed_limit_mph(),
            imputed_speed_limit_mph: edge.imputed_speed_limit_mph as f32,
            pop_density_1000_per_sq_mi: pop_den,
            emp_density_1000_per_sq_mi: emp_den,
            road_class: edge.road_class,
            opp_density_per_sq_mi: opp_den,
            traffic_control_density_per_sq_mi: tc_den,
        }
    }

    /// Computes `EdgeStaticFeatures` in parallel (`rayon`) across all edges in `open_net`.
    ///
    /// When `only_high_functional` is `true`, spatial R*-tree queries are only executed
    /// for high-functional class edges (`Motorway`, `Trunk`, `Link`, `Primary`, `Secondary`, `Tertiary`).
    pub fn compute_all_edge_features(
        &self,
        open_net: &LoadedOpenNetwork,
        only_high_functional: bool,
    ) -> Vec<EdgeStaticFeatures> {
        open_net
            .edges
            .par_iter()
            .map(|edge| {
                let run_spatial = !only_high_functional || edge.road_class.is_high_functional();
                self.compute_edge_features(edge, run_spatial)
            })
            .collect()
    }
}
