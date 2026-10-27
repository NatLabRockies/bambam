use super::{graph::osm_element_filter::ElementFilter, OsmError};
use crate::{
    algorithm::truncation::ComponentFilter,
    model::osm::{
        graph::OsmGraphVectorized,
        import_ops::{self, OsmCollection},
    },
};
use bambam_network::{
    parse_wkt_str_to_geometry_and_bbox, IslandDetectionAlgorithm, NetworkEdgeListConfiguration,
};
use geo::Geometry;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum OsmSource {
    Pbf {
        pbf_filepath: String,
        #[serde(default)]
        study_region: Option<String>,
        network_filter: Option<ElementFilter>,
        #[serde(default)]
        edge_lists: Option<Vec<NetworkEdgeListConfiguration>>,
        #[serde(default)]
        island_algorithm_configuration: Option<IslandDetectionAlgorithm>,
        extent_filter_filepath: Option<String>,
        component_filter: Option<ComponentFilter>,
        truncate_by_edge: bool,
        ignore_errors: bool,
        simplify: bool,
        #[serde(default)]
        infrastructure_match_distance_meters: Option<f64>,
    },
    LocalParquet {
        local_source: String,
        #[serde(default)]
        study_region: Option<String>,
        #[serde(default)]
        edge_lists: Option<Vec<NetworkEdgeListConfiguration>>,
        #[serde(default)]
        island_algorithm_configuration: Option<IslandDetectionAlgorithm>,
        extent_filter_filepath: Option<String>,
        component_filter: Option<ComponentFilter>,
        truncate_by_edge: bool,
        ignore_errors: bool,
        simplify: bool,
        #[serde(default)]
        infrastructure_match_distance_meters: Option<f64>,
    },
}

impl OsmSource {
    pub fn import(&self) -> Result<OsmGraphVectorized, OsmError> {
        match self {
            OsmSource::Pbf {
                pbf_filepath,
                study_region,
                network_filter,
                edge_lists,
                island_algorithm_configuration,
                extent_filter_filepath,
                component_filter,
                truncate_by_edge,
                ignore_errors,
                simplify,
                infrastructure_match_distance_meters,
            } => {
                let effective_edge_lists = match edge_lists.as_ref().filter(|e| !e.is_empty()) {
                    Some(lists) => lists.clone(),
                    None => NetworkEdgeListConfiguration::default_multimodal(),
                };
                let net_ftr = match network_filter {
                    Some(ElementFilter::NoFilter) | None => {
                        ElementFilter::from_edge_list_configs(&effective_edge_lists)
                    }
                    Some(other) => other.clone(),
                };
                let extent_opt = extent_filter_filepath
                    .as_deref()
                    .map(read_extent_wkt)
                    .transpose()?;
                let cc_ftr = component_filter.clone().unwrap_or_default();

                log::info!("Reading PBF source and extracting topological Connectors & Segments");
                let (nodes_map, ways_map) =
                    import_ops::read_pbf(pbf_filepath, net_ftr, &extent_opt)?;

                let result = OsmGraphVectorized::from_osm_maps_with_config(
                    pbf_filepath.clone(),
                    study_region.clone(),
                    nodes_map,
                    ways_map,
                    extent_opt.as_ref(),
                    &effective_edge_lists,
                    &cc_ftr,
                    island_algorithm_configuration.as_ref(),
                    *truncate_by_edge,
                    *simplify,
                    *ignore_errors,
                    *infrastructure_match_distance_meters,
                )?;

                log::info!(
                    "Loaded PBF-sourced Compass graph with {} vertices across {} mode edge lists",
                    result.vertices.len(),
                    result.edge_lists.len()
                );
                Ok(result)
            }
            OsmSource::LocalParquet {
                local_source,
                study_region,
                edge_lists,
                island_algorithm_configuration,
                extent_filter_filepath,
                component_filter,
                truncate_by_edge,
                ignore_errors,
                simplify,
                infrastructure_match_distance_meters,
            } => {
                let effective_edge_lists = match edge_lists.as_ref().filter(|e| !e.is_empty()) {
                    Some(lists) => lists.clone(),
                    None => NetworkEdgeListConfiguration::default_multimodal(),
                };
                let extent_opt = extent_filter_filepath
                    .as_deref()
                    .map(read_extent_wkt)
                    .transpose()?;
                let cc_ftr = component_filter.clone().unwrap_or_default();

                log::info!("Reading retained OSM Parquet source from '{local_source}'");
                let collection = OsmCollection::from_local_source(Path::new(local_source))?;
                let (src_label, nodes_map, ways_map) = collection.into_maps();

                let result = OsmGraphVectorized::from_osm_maps_with_config(
                    src_label,
                    study_region.clone(),
                    nodes_map,
                    ways_map,
                    extent_opt.as_ref(),
                    &effective_edge_lists,
                    &cc_ftr,
                    island_algorithm_configuration.as_ref(),
                    *truncate_by_edge,
                    *simplify,
                    *ignore_errors,
                    *infrastructure_match_distance_meters,
                )?;

                log::info!(
                    "Loaded local Parquet-sourced OSM Compass graph with {} vertices across {} mode edge lists",
                    result.vertices.len(),
                    result.edge_lists.len()
                );
                Ok(result)
            }
        }
    }
}

/// helper function that attempts to read an optional WKT from a file if provided.
fn read_extent_wkt(extent_filter_filepath: &str) -> Result<Geometry<f32>, OsmError> {
    let wkt_str = std::fs::read_to_string(extent_filter_filepath).map_err(|e| {
        OsmError::ConfigurationError(format!("unable to read file {extent_filter_filepath}: {e}"))
    })?;

    deserialize_validate_extent_str(&wkt_str)
}

/// Try to deserialize a string into geometry and validate if said geometry is useful as a extent (Polygon or Multipolygon)
fn deserialize_validate_extent_str(wkt_str: &str) -> Result<Geometry<f32>, OsmError> {
    parse_wkt_str_to_geometry_and_bbox(wkt_str)
        .map(|(geom, _)| geom)
        .map_err(|e| match e {
            bambam_network::NetworkImportError::InvalidGeometry(msg) => {
                OsmError::InvalidExtentWKT(msg)
            }
            other => OsmError::InvalidWKT(other.to_string()),
        })
}

#[cfg(test)]
mod test {
    use crate::model::osm::{osm_source::deserialize_validate_extent_str, OsmError};
    use geo::{LineString, Polygon};

    #[test]
    fn test_deserialize_wkt_polygon() {
        // Should pass and produce expected geometry
        let poly_str = "POLYGON ((0 0, 1 0, 1 1, 0 1, 0 0))";
        let exterior_coords: Vec<(f32, f32)> = vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        let polygon = Polygon::new(LineString::from(exterior_coords), vec![]);
        match deserialize_validate_extent_str(poly_str) {
            Ok(p) => assert_eq!(p, geo::Geometry::Polygon(polygon)),
            Err(e) => panic!("failed due to: {e}"),
        }
    }

    #[test]
    fn test_deserialize_wkt_notpolygon_or_multipolygon() {
        // Should fail: Extent cannot be Points/Lines
        let point_str = "POINT(0 0)";
        assert!(matches!(
            deserialize_validate_extent_str(point_str),
            Err(OsmError::InvalidExtentWKT(_))
        ));

        let line_str = "LINESTRING (30 10, 10 30, 40 40)";
        assert!(matches!(
            deserialize_validate_extent_str(line_str),
            Err(OsmError::InvalidExtentWKT(_))
        ));

        let poly_str = "POLYGON ((30 10, 40 40, 20 40, 10 20, 30 10))";
        assert!(deserialize_validate_extent_str(poly_str).is_ok());

        let mpoly_str =
            "MULTIPOLYGON (((30 20, 45 40, 10 40, 30 20)), ((15  5, 40 10, 10 20,  5 10, 15  5)))";
        assert!(deserialize_validate_extent_str(mpoly_str).is_ok());
    }
}
