use bambam_osm::{app::OsmApp, model::OsmCliError};
use clap::Parser;

fn main() -> Result<(), OsmCliError> {
    let _ = env_logger::try_init();
    let args = OsmApp::parse();
    args.op.run()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use bambam_network::{
        CommonNetworkImportArgs, EdgeMappingRecord, VertexCompleteRecord, VertexMappingRecord,
    };
    use bambam_osm::{
        algorithm::truncation::ComponentFilter,
        app::{OsmNetworkArgs, OsmOperation},
        model::osm::{
            graph::{osm_element_filter::ElementFilter, CompassWriter},
            import_ops::OsmCollection,
            OsmSource,
        },
    };
    use routee_compass_core::{model::network::EdgeConfig, util::fs::read_utils};

    #[test]
    fn test_e2e_liechtenstein() {
        // uses a small OSM dataset to test the end-to-end data processing
        let cleanup_tmp_dir = match std::env::var("CLEANUP_TMP_DIR") {
            Ok(value) => value
                .parse::<bool>()
                .expect("CLEANUP_TMP_DIR must be 'true' or 'false'"),
            Err(_) => true, // default
        };
        let temp_directory = "src/test/tmp";
        let pbf_file = "src/test/liechtenstein-250101.osm.pbf";
        let extent_file = "src/test/schaan_liechtenstein.txt";
        let config_file = "src/test/test_osm_import.toml";
        let op = OsmOperation::Network(OsmNetworkArgs {
            pbf_file: Some(pbf_file.to_string()),
            common: CommonNetworkImportArgs {
                configuration_file: config_file.to_string(),
                output_directory: temp_directory.to_string(),
                local_source: None,
                extent_file: Some(extent_file.to_string()),
                infrastructure_match_distance_meters: None,
            },
        });

        if let Err(e) = op.run() {
            panic!("bambam-osm run failure during import: {e}");
        }

        // Verify raw Parquet retention and round-trip
        let raw_loaded = OsmCollection::from_parquet_dir(Path::new(temp_directory))
            .expect("failed to load retained raw OSM Parquet files");
        assert!(!raw_loaded.nodes.is_empty());
        assert!(!raw_loaded.ways.is_empty());

        // Verify summary.toml exists
        assert!(Path::new(temp_directory).join("summary.toml").exists());

        // Verify standardized ID mappings and VertexCompleteRecord schema
        let v_mappings: Box<[VertexMappingRecord]> =
            read_utils::from_csv(&"src/test/tmp/vertices-mapping.csv.gz", true, None, None)
                .expect("failed to read vertices-mapping.csv.gz");
        let nodes: Box<[VertexCompleteRecord]> =
            read_utils::from_csv(&"src/test/tmp/vertices-complete.csv.gz", true, None, None)
                .expect("failed to read vertices-complete.csv.gz");
        assert_eq!(v_mappings.len(), nodes.len());

        // test graph connectivity across walk, bike, and drive mode subdirectories
        let mut disconnected: Vec<String> = vec![];
        for mode in ["walk", "bike", "drive"] {
            let e_mappings: Box<[EdgeMappingRecord]> = read_utils::from_csv(
                &format!("src/test/tmp/{mode}/edges-mapping.csv.gz"),
                true,
                None,
                None,
            )
            .expect("failed to read mode edges-mapping.csv.gz");
            let edges: Box<[EdgeConfig]> = read_utils::from_csv(
                &format!("src/test/tmp/{mode}/edges-compass.csv.gz"),
                true,
                None,
                None,
            )
            .expect("failed to read mode edges-compass.csv.gz");

            assert_eq!(e_mappings.len(), edges.len());
            for edge in edges.iter() {
                let src_id = edge.src_vertex_id.0;
                let edge_id = edge.edge_id.0;
                let dst_id = edge.dst_vertex_id.0;
                let edge_img = format!("{mode}:({src_id})-[{edge_id}]->({dst_id})");
                let src = nodes.get(src_id);
                let dst = nodes.get(dst_id);
                match (src, dst) {
                    (None, Some(_)) => disconnected.push(format!("src missing from {edge_img}")),
                    (Some(_), None) => disconnected.push(format!("dst missing from {edge_img}")),
                    (None, None) => disconnected.push(format!("src/dst missing from {edge_img}")),
                    (Some(_), Some(_)) => {}
                }
            }
        }

        // Also verify simplified + largest component import on the same Liechtenstein PBF
        let simplified_dir = Path::new("src/test/tmp_simplified");
        let simplified_source = OsmSource::Pbf {
            pbf_filepath: pbf_file.to_string(),
            study_region: Some("schaan_simplified".to_string()),
            network_filter: Some(ElementFilter::OsmnxAllPublic),
            edge_lists: None,
            island_algorithm_configuration: None,
            extent_filter_filepath: Some(extent_file.to_string()),
            component_filter: Some(ComponentFilter::Largest),
            truncate_by_edge: true,
            ignore_errors: true,
            simplify: true,
            infrastructure_match_distance_meters: Some(15.0),
        };
        let simplified_graph = simplified_source
            .import()
            .expect("simplified OSM import failed");
        simplified_graph
            .write_compass(simplified_dir, true)
            .expect("writing simplified OSM compass files failed");
        assert!(!simplified_graph.vertices.is_empty());
        assert!(!simplified_graph.edge_lists.is_empty());
        for el in &simplified_graph.edge_lists {
            for edge in el.edges.edges() {
                assert!(edge.src_vertex_id.0 < simplified_graph.vertices.len());
                assert!(edge.dst_vertex_id.0 < simplified_graph.vertices.len());
            }
        }

        // Also verify consuming configuration/bambam-network/bambam-network-config.json directly
        let net_cfg_dir = Path::new("src/test/tmp_network_config");
        let net_cfg_op = OsmOperation::Network(OsmNetworkArgs {
            pbf_file: Some(pbf_file.to_string()),
            common: CommonNetworkImportArgs {
                configuration_file:
                    "../../configuration/bambam-network/bambam-network-config.json".to_string(),
                output_directory: net_cfg_dir.to_string_lossy().to_string(),
                local_source: None,
                extent_file: Some(extent_file.to_string()),
                infrastructure_match_distance_meters: None,
            },
        });
        net_cfg_op
            .run()
            .expect("bambam-osm failed with bambam-network-config.json");
        assert!(net_cfg_dir.join("summary.toml").exists());
        for mode in ["walk", "bike", "drive"] {
            assert!(net_cfg_dir.join(mode).join("edges-compass.csv.gz").exists());
            assert!(net_cfg_dir.join(mode).join("edges-mapping.csv.gz").exists());
        }

        // Also verify re-importing from the retained local_source Parquet directory without pbf_file
        let local_reimport_dir = Path::new("src/test/tmp_local_reimport");
        let local_reimport_op = OsmOperation::Network(OsmNetworkArgs {
            pbf_file: None,
            common: CommonNetworkImportArgs {
                configuration_file:
                    "../../configuration/bambam-network/bambam-network-config.json".to_string(),
                output_directory: local_reimport_dir.to_string_lossy().to_string(),
                local_source: Some(net_cfg_dir.to_string_lossy().to_string()),
                extent_file: None,
                infrastructure_match_distance_meters: None,
            },
        });
        local_reimport_op
            .run()
            .expect("bambam-osm failed when re-importing from local_source Parquet");
        assert!(local_reimport_dir.join("summary.toml").exists());

        // cleanup before exits, order is important.
        if cleanup_tmp_dir {
            std::fs::remove_dir_all(temp_directory).expect("failed to remove tmp directory");
            std::fs::remove_dir_all(simplified_dir)
                .expect("failed to remove tmp_simplified directory");
            std::fs::remove_dir_all(net_cfg_dir)
                .expect("failed to remove tmp_network_config directory");
            std::fs::remove_dir_all(local_reimport_dir)
                .expect("failed to remove tmp_local_reimport directory");
        }
        if !disconnected.is_empty() {
            disconnected.iter().take(5).for_each(|d| println!("{d}"));
            panic!("files were written but some edges had invalid src/dst vertex ids (first 5 in log)");
        }
    }
}
