//! Unified road network import pipeline for RouteE Compass and BAMBAM.
//!
//! This module provides a single high-level entry point ([`import_network`] /
//! [`NetworkImportRequest`]) on top of both the Overture Maps Foundation (`bambam-omf`)
//! and OpenStreetMap (`bambam-osm`) ETL pipelines, backed by the shared split-based
//! graph construction and serialization core in `bambam-network`.
//!
//! ## Guarantees across both OMF and OSM imports
//! 1. **Split-Based Topological Graph Construction**: Both OMF and OSM represent networks as
//!    Connectors (intersection/control vertices) + Segments + Linear-Referenced Splits
//!    ([`bambam_network::SegmentSplit`]), avoiding destructive in-place graph simplification.
//! 2. **Spatial Infrastructure Attachment (`rstar`)**: Traffic control and pedestrian comfort
//!    features (`traffic_signals`, `stop`, `give_way`, `crossing`) are spatially matched to
//!    their nearest connector vertex within a configurable Haversine distance (default `15.0`m)
//!    and written to both `vertices-complete.csv.gz` and `vertices-*-enumerated.txt.gz`.
//! 3. **Always-On Raw Data Retention**: Raw source records are always persisted as Snappy-compressed
//!    Parquet files under `<output_directory>/raw/` (`segments.parquet`, `connectors.parquet`,
//!    `infrastructure.parquet` for OMF; `nodes.parquet`, `ways.parquet` for OSM).
//! 4. **Always-On Bidirectional ID Mappings**: Source-to-Compass vertex and edge mappings are
//!    always written to `vertices-mapping.csv.gz` (`vertex_id,source_id`) and
//!    `<mode>/edges-mapping.csv.gz` (`edge_id,source_id,start_lr,end_lr,heading`).

use std::path::Path;

use bambam_network::{ComponentsAlgorithmType, IslandDetectionAlgorithm};
use bambam_omf::app::{network::NetworkEdgeListConfiguration, CliBoundingBox};
use bambam_osm::{
    algorithm::truncation::ComponentFilter,
    model::osm::{
        graph::{osm_element_filter::ElementFilter, CompassWriter, OsmGraphVectorized},
        import_ops,
    },
};
use geo::{BoundingRect, Coord, Geometry, LineString, MapCoords, Polygon};
use geozero::{wkt::Wkt as WktReader, ToGeo};
use routee_compass_core::model::unit::DistanceUnit;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Geographic extent specification for a road network import.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GeographicExtent {
    /// Bounding box in WGS84 lon/lat degrees: `[xmin, ymin, xmax, ymax]`.
    Bbox {
        xmin: f32,
        ymin: f32,
        xmax: f32,
        ymax: f32,
    },
    /// Inline WKT string (`POLYGON` or `MULTIPOLYGON`).
    Wkt { wkt: String },
    /// Path to a file containing a WKT polygon/multipolygon (or JSON with an `"extent"` key).
    WktFile { path: String },
}

impl GeographicExtent {
    /// Converts the extent specification into a `geo::Geometry<f32>` polygon/multipolygon
    /// and its bounding `CliBoundingBox`.
    pub fn to_geometry_and_bbox(&self) -> Result<(Geometry<f32>, CliBoundingBox), String> {
        match self {
            GeographicExtent::Bbox {
                xmin,
                ymin,
                xmax,
                ymax,
            } => {
                if xmin >= xmax || ymin >= ymax {
                    return Err(format!(
                        "invalid bbox: expected xmin < xmax and ymin < ymax, got [{xmin}, {ymin}, {xmax}, {ymax}]"
                    ));
                }
                let poly = Polygon::new(
                    LineString::from(vec![
                        (*xmin, *ymin),
                        (*xmax, *ymin),
                        (*xmax, *ymax),
                        (*xmin, *ymax),
                        (*xmin, *ymin),
                    ]),
                    vec![],
                );
                let bbox = CliBoundingBox {
                    xmin: *xmin,
                    ymin: *ymin,
                    xmax: *xmax,
                    ymax: *ymax,
                };
                Ok((Geometry::Polygon(poly), bbox))
            }
            GeographicExtent::Wkt { wkt } => parse_wkt_str_to_geometry_and_bbox(wkt),
            GeographicExtent::WktFile { path } => {
                let raw = std::fs::read_to_string(path)
                    .map_err(|e| format!("failed to read extent file '{path}': {e}"))?;
                parse_wkt_str_to_geometry_and_bbox(&raw)
            }
        }
    }
}

fn parse_wkt_str_to_geometry_and_bbox(
    raw: &str,
) -> Result<(Geometry<f32>, CliBoundingBox), String> {
    let trimmed = raw.trim();
    // Support JSON with an `{"extent": "POLYGON(...)"}` wrapper as well as plain WKT
    let wkt_str: String = if trimmed.starts_with('{') {
        let json_val: serde_json::Value = serde_json::from_str(trimmed)
            .map_err(|e| format!("failed to parse JSON extent wrapper: {e}"))?;
        json_val
            .get("extent")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "JSON extent object missing string key 'extent'".to_string())?
            .trim()
            .to_string()
    } else {
        trimmed.to_string()
    };

    let geom_f64 = WktReader(wkt_str.as_str())
        .to_geo()
        .map_err(|e| format!("failed to parse WKT geometry: {e}"))?;
    let geom_f32: Geometry<f32> = geom_f64.map_coords(|Coord { x, y }| Coord {
        x: x as f32,
        y: y as f32,
    });

    match &geom_f32 {
        Geometry::Polygon(_) | Geometry::MultiPolygon(_) => {}
        other => {
            return Err(format!(
                "extent WKT must be a POLYGON or MULTIPOLYGON, got {other:?}"
            ))
        }
    }

    let rect = geom_f32
        .bounding_rect()
        .ok_or_else(|| "extent geometry has empty bounding box".to_string())?;
    let bbox = CliBoundingBox::from_coords(&rect.min(), &rect.max())?;
    Ok((geom_f32, bbox))
}

/// Source dataset for road network import.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NetworkDataSource {
    /// Overture Maps Foundation (downloads from S3 by default, or reads from a local raw directory/file).
    OvertureMaps {
        /// Optional path to a previously retained `raw/` directory or JSON export.
        #[serde(default)]
        local_source: Option<String>,
    },
    /// OpenStreetMap `.osm.pbf` file or retained `raw/` Parquet directory.
    OpenStreetMap {
        /// Path to the local `.osm.pbf` file (or retained `raw/` Parquet directory).
        pbf_path: String,
        /// Optional path to a previously retained `raw/` Parquet directory.
        #[serde(default)]
        local_source: Option<String>,
        /// Whether to collapse intermediate degree-2 shape nodes into segment LineStrings (defaults to `true`).
        #[serde(default)]
        simplify: Option<bool>,
        /// Whether to retain edges that intersect the polygon boundary (defaults to `true`).
        #[serde(default)]
        truncate_by_edge: Option<bool>,
    },
}

/// Unified high-level request for importing a RouteE Compass road network from
/// either Overture Maps or OpenStreetMap over a geographic extent.
///
/// Raw source data (`raw/*.parquet`) and bidirectional ID mappings
/// (`vertices-mapping.csv.gz` and `edges-mapping.csv.gz`) are always retained.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NetworkImportRequest {
    /// Study region / network name.
    pub name: String,
    /// Geographic extent (bounding box, WKT string, or WKT file).
    pub extent: GeographicExtent,
    /// Network data source (`overture_maps` or `open_street_map`).
    pub source: NetworkDataSource,
    /// Output directory where Compass network files, mappings, and `raw/` Parquet files are written.
    pub output_directory: String,
    /// Optional path to a network import configuration file (`bambam-network-config.json`).
    #[serde(default)]
    pub configuration_file: Option<String>,
    /// Optional travel modes (e.g., `["walk", "bike", "drive"]`). Defaults to `["walk", "bike", "drive"]` when `configuration_file` is not provided.
    #[serde(default)]
    pub modes: Option<Vec<String>>,
    /// Optional maximum Haversine distance in meters for snapping infrastructure (`traffic_signals`, `stop`, `give_way`, `crossing`) to vertices (defaults to `15.0`).
    #[serde(default)]
    pub infrastructure_match_distance_meters: Option<f64>,
}

impl NetworkImportRequest {
    /// Executes the road network import and writes the RouteE Compass dataset,
    /// ID mapping files, and `raw/` Parquet tables to `self.output_directory`.
    pub fn run(&self) -> Result<(), String> {
        import_network(self)
    }
}

/// High-level entry point to import a road network from a [`NetworkImportRequest`].
pub fn import_network(request: &NetworkImportRequest) -> Result<(), String> {
    let (extent_geom, bbox) = request.extent.to_geometry_and_bbox()?;
    let out_dir = Path::new(&request.output_directory);

    let (edge_lists, island_alg, cfg_match_dist) = if let Some(ref cfg_path) =
        request.configuration_file
    {
        let cfg = bambam_network::NetworkImportFilterConfig::from_file(Path::new(cfg_path))
            .map_err(|e| e.to_string())?;
        (
            cfg.edge_lists,
            cfg.island_algorithm_configuration,
            cfg.infrastructure_match_distance_meters,
        )
    } else {
        let edge_lists = match request.modes.as_ref().filter(|m| !m.is_empty()) {
            Some(modes) => modes
                .iter()
                .map(|m| NetworkEdgeListConfiguration::default_for_mode(m))
                .collect::<Result<Vec<_>, _>>()?,
            None => NetworkEdgeListConfiguration::default_multimodal(),
        };
        let island_alg = Some(IslandDetectionAlgorithm {
            min_distance: 2.0,
            distance_unit: DistanceUnit::Miles,
            algorithm_type: ComponentsAlgorithmType::Wcc,
        });
        (edge_lists, island_alg, None)
    };

    let match_distance = request
        .infrastructure_match_distance_meters
        .or(cfg_match_dist);

    match &request.source {
        NetworkDataSource::OvertureMaps { local_source } => {
            let local_path = local_source.as_deref().map(Path::new);
            bambam_omf::app::network::run(
                &request.name,
                Some(&bbox),
                &edge_lists,
                out_dir,
                local_path,
                island_alg,
                Some(extent_geom),
                match_distance,
            )
            .map_err(|e| e.to_string())
        }
        NetworkDataSource::OpenStreetMap {
            pbf_path,
            local_source,
            simplify,
            truncate_by_edge,
        } => {
            let extent_opt = Some(extent_geom);
            let (src_label, nodes_map, ways_map) = if let Some(ref local_dir) = local_source {
                let col = import_ops::OsmCollection::from_local_source(Path::new(local_dir))
                    .map_err(|e| e.to_string())?;
                col.into_maps()
            } else if Path::new(pbf_path).is_dir() {
                let col = import_ops::OsmCollection::from_local_source(Path::new(pbf_path))
                    .map_err(|e| e.to_string())?;
                col.into_maps()
            } else {
                let filter = ElementFilter::from_edge_list_configs(&edge_lists);
                let (n, w) = import_ops::read_pbf(pbf_path, filter, &extent_opt)
                    .map_err(|e| e.to_string())?;
                (pbf_path.clone(), n, w)
            };

            let graph = OsmGraphVectorized::from_osm_maps_with_config(
                src_label,
                Some(request.name.clone()),
                nodes_map,
                ways_map,
                extent_opt.as_ref(),
                &edge_lists,
                &ComponentFilter::KeepAll,
                island_alg.as_ref(),
                truncate_by_edge.unwrap_or(true),
                simplify.unwrap_or(true),
                true,
                match_distance,
            )
            .map_err(|e| e.to_string())?;

            graph
                .write_compass(out_dir, true)
                .map_err(|e| e.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bambam_omf::collection::{
        record::{
            segment::{ConnectorReference, SpeedLimitWithUnit},
            SegmentClass, SegmentSpeedLimit, SegmentSpeedUnit, SegmentSubtype,
        },
        InfrastructureRecord, TransportationCollection, TransportationConnectorRecord,
        TransportationSegmentRecord,
    };
    use geo::{LineString, Point};
    use std::path::PathBuf;

    #[test]
    fn test_unified_network_import_omf_and_osm() {
        let tmp_dir =
            std::env::temp_dir().join(format!("bambam_phase4_unified_test_{}", std::process::id()));
        if tmp_dir.exists() {
            let _ = std::fs::remove_dir_all(&tmp_dir);
        }
        std::fs::create_dir_all(&tmp_dir).unwrap();

        // 1. Test OMF via unified NetworkImportRequest with Bbox extent and built-in mode presets
        let omf_source_dir = tmp_dir.join("omf_source");
        let c0 = TransportationConnectorRecord {
            id: "conn-a".to_string(),
            geometry: Some(Geometry::Point(Point::new(-105.275, 40.015))),
            version: 1,
            ..Default::default()
        };
        let c1 = TransportationConnectorRecord {
            id: "conn-b".to_string(),
            geometry: Some(Geometry::Point(Point::new(-105.225, 40.015))),
            version: 1,
            ..Default::default()
        };
        let seg = TransportationSegmentRecord {
            id: "seg-ab".to_string(),
            geometry: Some(Geometry::LineString(LineString::from(vec![
                (-105.275, 40.015),
                (-105.225, 40.015),
            ]))),
            subtype: Some(SegmentSubtype::Road),
            class: Some(SegmentClass::Residential),
            version: 1,
            connectors: Some(vec![
                ConnectorReference {
                    connector_id: "conn-a".to_string(),
                    at: 0.0,
                },
                ConnectorReference {
                    connector_id: "conn-b".to_string(),
                    at: 1.0,
                },
            ]),
            speed_limits: Some(vec![SegmentSpeedLimit {
                min_speed: None,
                max_speed: Some(SpeedLimitWithUnit {
                    value: 25,
                    unit: SegmentSpeedUnit::Mph,
                }),
                is_max_speed_variable: Some(false),
                when: None,
                between: None,
            }]),
            ..Default::default()
        };
        let infra = InfrastructureRecord {
            id: "infra-sig".to_string(),
            geometry: Some(Geometry::Point(Point::new(-105.275, 40.015))),
            subtype: Some("transportation".to_string()),
            class: Some("traffic_signals".to_string()),
            ..Default::default()
        };
        let collection = TransportationCollection {
            release: "2026-03-18.0".to_string(),
            connectors: vec![c0, c1],
            segments: vec![seg],
            infrastructure: vec![infra],
        };
        collection.to_parquet(&omf_source_dir).unwrap();

        let omf_out_dir = tmp_dir.join("omf_out");
        let omf_req = NetworkImportRequest {
            name: "boulder_test".to_string(),
            extent: GeographicExtent::Bbox {
                xmin: -105.30,
                ymin: 40.00,
                xmax: -105.20,
                ymax: 40.03,
            },
            source: NetworkDataSource::OvertureMaps {
                local_source: Some(omf_source_dir.to_string_lossy().to_string()),
            },
            output_directory: omf_out_dir.to_string_lossy().to_string(),
            configuration_file: None,
            modes: None, // defaults to ["walk", "bike", "drive"]
            infrastructure_match_distance_meters: Some(15.0),
        };
        omf_req.run().unwrap();

        assert!(omf_out_dir.join("raw").join("segments.parquet").exists());
        assert!(omf_out_dir.join("raw").join("connectors.parquet").exists());
        assert!(omf_out_dir
            .join("raw")
            .join("infrastructure.parquet")
            .exists());
        assert!(omf_out_dir.join("vertices-mapping.csv.gz").exists());
        assert!(omf_out_dir.join("vertices-complete.csv.gz").exists());
        assert!(omf_out_dir
            .join("walk")
            .join("edges-mapping.csv.gz")
            .exists());
        assert!(omf_out_dir
            .join("bike")
            .join("edges-mapping.csv.gz")
            .exists());
        assert!(omf_out_dir
            .join("drive")
            .join("edges-mapping.csv.gz")
            .exists());

        // 2. Test OSM via unified NetworkImportRequest with WktFile extent
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let osm_test_dir = manifest_dir
            .parent()
            .unwrap()
            .join("bambam-osm")
            .join("src")
            .join("test");
        let pbf_path = osm_test_dir.join("liechtenstein-250101.osm.pbf");
        let wkt_path = osm_test_dir.join("schaan_liechtenstein.txt");

        let osm_out_dir = tmp_dir.join("osm_out");
        let osm_req = NetworkImportRequest {
            name: "schaan_osm".to_string(),
            extent: GeographicExtent::WktFile {
                path: wkt_path.to_string_lossy().to_string(),
            },
            source: NetworkDataSource::OpenStreetMap {
                pbf_path: pbf_path.to_string_lossy().to_string(),
                local_source: None,
                simplify: Some(true),
                truncate_by_edge: Some(true),
            },
            output_directory: osm_out_dir.to_string_lossy().to_string(),
            configuration_file: None,
            modes: Some(vec!["drive".to_string(), "walk".to_string()]),
            infrastructure_match_distance_meters: Some(15.0),
        };
        osm_req.run().unwrap();

        assert!(osm_out_dir.join("raw").join("nodes.parquet").exists());
        assert!(osm_out_dir.join("raw").join("ways.parquet").exists());
        assert!(osm_out_dir.join("summary.toml").exists());
        assert!(osm_out_dir.join("vertices-mapping.csv.gz").exists());
        assert!(osm_out_dir.join("vertices-complete.csv.gz").exists());
        assert!(osm_out_dir
            .join("drive")
            .join("edges-compass.csv.gz")
            .exists());
        assert!(osm_out_dir
            .join("drive")
            .join("edges-mapping.csv.gz")
            .exists());
        assert!(osm_out_dir
            .join("walk")
            .join("edges-compass.csv.gz")
            .exists());
        assert!(osm_out_dir
            .join("walk")
            .join("edges-mapping.csv.gz")
            .exists());

        // Verify both OMF and OSM outputs share the exact same core Compass + mapping + infrastructure files
        for out_dir in [&omf_out_dir, &osm_out_dir] {
            for root_file in [
                "summary.toml",
                "vertices-compass.csv.gz",
                "vertices-complete.csv.gz",
                "vertices-mapping.csv.gz",
                "vertices-traffic-signals-enumerated.txt.gz",
                "vertices-stop-enumerated.txt.gz",
                "vertices-give-way-enumerated.txt.gz",
                "vertices-crossing-enumerated.txt.gz",
            ] {
                assert!(
                    out_dir.join(root_file).exists(),
                    "missing {} in {}",
                    root_file,
                    out_dir.display()
                );
            }
            for mode in ["walk", "drive"] {
                let mode_dir = out_dir.join(mode);
                for mode_file in [
                    "edges-compass.csv.gz",
                    "edges-mapping.csv.gz",
                    "edges-geometries-enumerated.txt.gz",
                    "edges-speeds-mph-enumerated.txt.gz",
                    "edges-classes-enumerated.txt.gz",
                    "edges-classes-speed-mapping.csv.gz",
                    "edges-bearings-enumerated.txt.gz",
                ] {
                    assert!(
                        mode_dir.join(mode_file).exists(),
                        "missing {}/{} in {}",
                        mode,
                        mode_file,
                        out_dir.display()
                    );
                }
            }
        }

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
