use super::{graph::osm_element_filter::ElementFilter, OsmError};
use crate::{
    algorithm::truncation::ComponentFilter,
    model::osm::{graph::OsmGraphVectorized, import_ops},
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
        #[serde(default)]
        infrastructure_match_distance_meters: Option<f64>,
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
                infrastructure_match_distance_meters,
            } => {
                let net_ftr = network_filter.clone().unwrap_or_default();
                let extent_opt = extent_filter_filepath
                    .as_deref()
                    .map(read_extent_wkt)
                    .transpose()?;
                let cc_ftr = component_filter.clone().unwrap_or_default();

                log::info!("Reading PBF source and extracting topological Connectors & Segments");
                let (nodes_map, ways_map) =
                    import_ops::read_pbf(pbf_filepath, net_ftr, &extent_opt)?;

                let result = OsmGraphVectorized::from_osm_maps(
                    pbf_filepath.clone(),
                    nodes_map,
                    ways_map,
                    extent_opt.as_ref(),
                    &cc_ftr,
                    None,
                    *truncate_by_edge,
                    *simplify,
                    *ignore_errors,
                    *infrastructure_match_distance_meters,
                )?;

                log::info!(
                    "Loaded PBF-sourced Compass graph with {} vertices, {} edges",
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
