use super::{graph::osm_element_filter::ElementFilter, OsmError};
use crate::{
    algorithm::{
        consolidation, simplification,
        truncation::{self, ComponentFilter},
    },
    model::osm::{
        graph::{OsmGraph, OsmGraphVectorized},
        import_ops,
    },
};
use geo::Geometry;
use geo::MapCoords;
use geozero::{wkt::Wkt as WktReader, ToGeo};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum OsmSource {
    Pbf {
        pbf_filepath: String,
        network_filter: Option<ElementFilter>,
        extent_filter_filepath: Option<String>,
        component_filter: Option<ComponentFilter>,
        truncate_by_edge: bool,
        ignore_errors: bool,
        simplify: bool,
        consolidate: bool,
        consolidation_threshold: uom::si::f64::Length,
        parallelize: bool,
    },
}

impl OsmSource {
    pub fn import(&self) -> Result<OsmGraphVectorized, OsmError> {
        match self {
            OsmSource::Pbf {
                pbf_filepath,
                network_filter,
                extent_filter_filepath,
                component_filter,
                truncate_by_edge,
                ignore_errors,
                simplify,
                consolidate,
                consolidation_threshold,
                parallelize,
            } => {
                let net_ftr = network_filter.clone().unwrap_or_default();
                let extent_opt = extent_filter_filepath
                    .as_deref()
                    .map(read_extent_wkt)
                    .transpose()?;
                let cc_ftr = component_filter.clone().unwrap_or_default();

                // # download the network data from OSM within buffered polygon
                // # create buffered graph from the downloaded data
                eprintln!();
                log::info!("  (((1))) reading PBF source");
                let (nodes, ways) = import_ops::read_pbf(pbf_filepath, net_ftr, &extent_opt)?;
                let mut graph = OsmGraph::new(nodes, ways)?;

                // rjf: this is handled above in import_ops::read_pbf for performance reasons
                // # truncate buffered graph to the buffered polygon and retain_all for
                // # now. needed because overpass returns entire ways that also include
                // # nodes outside the poly if the way (that is, a way with a single OSM
                // # ID) has a node inside the poly at some point.
                // G_buff = truncate.truncate_graph_polygon(G_buff, poly_buff, truncate_by_edge=truncate_by_edge)
                // if let Some(extent) = &extent_opt {
                //     let extent_buffered = buffer_extent(extent, Self::BUFFER_500M_IN_DEGREES)?;
                //     truncation::truncate_graph_polygon(&mut graph, extent, *truncate_by_edge)?;
                // }

                // # keep only the largest weakly connected component if retain_all is False
                // if not retain_all:
                // G_buff = truncate.largest_component(G_buff, strongly=False)
                eprintln!();
                log::info!("  (((2))) truncating graph via connected components filtering");
                truncation::filter_components(&mut graph, &cc_ftr)?;

                let mut apply_second_component_filter = false;
                if *simplify {
                    eprintln!();
                    log::info!("  (((3))) simplifying graph");
                    simplification::simplify_graph(&mut graph, *parallelize)?;
                    apply_second_component_filter = true;
                } else {
                    eprintln!();
                    log::info!("  (((3))) simplifying graph (skipped)");
                }

                // # truncate graph by original polygon to return graph within polygon
                // # caller wants. don't *simplify again: this allows us to retain
                // # intersections along the street that may now only connect 2 street
                // # segments in the network, but in reality also connect to an
                // # intersection just outside the polygon
                // G = truncate.truncate_graph_polygon(G_buff, polygon, truncate_by_edge=truncate_by_edge)
                if let Some(extent) = &extent_opt {
                    eprintln!();
                    log::info!("  (((4))) truncating graph via extent filtering");
                    truncation::truncate_graph_polygon(
                        &mut graph,
                        extent,
                        *truncate_by_edge,
                        *ignore_errors,
                    )?;
                    apply_second_component_filter = true;
                } else {
                    eprintln!();
                    log::info!("  (((4))) truncating graph via extent filtering (skipped)");
                }

                // # keep only the largest weakly connected component if retain_all is False
                // # we're doing this again in case the last truncate disconnected anything
                // # on the periphery
                // if not retain_all:
                // G = truncate.largest_component(G, strongly=False)
                if apply_second_component_filter {
                    eprintln!();
                    log::info!("  (((5))) truncating graph via connected components filtering");
                    truncation::filter_components(&mut graph, &cc_ftr)?;
                } else {
                    eprintln!();
                    log::info!(
                        "  (((5))) truncating graph via connected components filtering (skipped)"
                    );
                }

                // if requested, consolidate nodes in the graph
                if *consolidate {
                    eprintln!();
                    log::info!("  (((6))) consolidating graph nodes");
                    consolidation::consolidate_graph(&mut graph, *consolidation_threshold)?;
                } else {
                    eprintln!();
                    log::info!("  (((6))) consolidating graph nodes (skipped)");
                }

                // finalize the graph via vectorization
                let result = OsmGraphVectorized::new(graph, true)?;

                log::info!(
                    "loaded PBF-sourced Compass graph with {} nodes, {} ways",
                    result.nodes.len(),
                    result.ways.len()
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
    let geometry_f64 = WktReader(wkt_str).to_geo().map_err(|e| {
        OsmError::InvalidWKT(format!("unable to deserialize WKT in {wkt_str}: {e}"))
    })?;
    let geometry: Geometry<f32> = geometry_f64.map_coords(|geo::Coord { x, y }| geo::Coord {
        x: x as f32,
        y: y as f32,
    });

    match geometry {
        Geometry::Polygon(..) | Geometry::MultiPolygon(..) => Ok(geometry),
        _ => Err(OsmError::InvalidExtentWKT(format!(
            "Invalid extent provided. Only Polygon or Multipolygon can be a valid extent: {geometry:?}"
        ))),
    }
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
