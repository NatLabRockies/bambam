use bambam_osm::{
    config::OsmImportConfiguration,
    model::{
        osm::{graph::CompassWriter, OsmSource},
        OsmCliError,
    },
};
use clap::{Parser, Subcommand};
use std::path::Path;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct OsmAppArguments {
    #[command(subcommand)]
    app: App,
}

#[derive(Subcommand)]
pub enum App {
    Pbf {
        #[arg(long, help = "path to .pbf file for import")]
        pbf_file: String,
        #[arg(long, help = "optional study region name for summary.toml")]
        name: Option<String>,
        #[arg(long, help = "path to file containing WKT used to filter the PBF data")]
        extent_file: Option<String>,
        #[arg(
            long,
            help = "path to file with bambam-osm / travel-mode-filter configuration (.json or .toml)"
        )]
        configuration_file: Option<String>,
        #[arg(long, help = "output path for network dataset")]
        output_directory: String,
    },
}

pub fn run(app: &App) -> Result<(), OsmCliError> {
    let _ = env_logger::try_init();
    match app {
        App::Pbf {
            pbf_file,
            name,
            extent_file,
            configuration_file,
            output_directory,
        } => {
            let conf = match configuration_file {
                None => Ok(OsmImportConfiguration::default()),
                Some(f) => {
                    log::info!("reading bambam configuration from {f}");
                    OsmImportConfiguration::try_from(f)
                }
            }?;
            let out_path = Path::new(output_directory);
            let edge_lists = conf.resolved_edge_lists();
            let island_alg = conf.resolved_island_detection();
            let element_filter = conf.resolved_element_filter();
            let pbf_config = OsmSource::Pbf {
                pbf_filepath: pbf_file.clone(),
                study_region: name.clone(),
                extent_filter_filepath: extent_file.clone(),
                network_filter: Some(element_filter),
                edge_lists: Some(edge_lists),
                island_algorithm_configuration: island_alg,
                component_filter: Some(conf.component_filter),
                truncate_by_edge: conf.truncate_by_edge,
                ignore_errors: conf.ignore_osm_parsing_errors,
                simplify: conf.simplify,
                infrastructure_match_distance_meters: conf.infrastructure_match_distance_meters,
            };
            let graph = pbf_config.import()?;
            match graph.write_compass(out_path, conf.overwrite) {
                Ok(_) => {
                    eprintln!("finished.");
                    Ok(())
                }
                Err(e) => {
                    log::error!("bambam-osm failed: {e}");
                    Err(e)?
                }
            }
        }
    }
}

fn main() {
    let args = OsmAppArguments::parse();
    match run(&args.app) {
        Ok(_) => {
            // if !s.is_empty() {
            //     println!("{}", s);
            // }
        }
        Err(e) => {
            println!("{e}");
            // log::error!("app failed: {}", e);
            panic!("{}", e.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use bambam_network::{EdgeMappingRecord, VertexCompleteRecord, VertexMappingRecord};
    use bambam_osm::{
        algorithm::truncation::ComponentFilter,
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
        let conf = crate::App::Pbf {
            pbf_file: pbf_file.to_string(),
            name: Some("schaan_liechtenstein".to_string()),
            extent_file: Some(extent_file.to_string()),
            configuration_file: Some(config_file.to_string()),
            output_directory: temp_directory.to_string(),
        };

        if let Err(e) = crate::run(&conf) {
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

        // Also verify consuming configuration/bambam-omf/travel-mode-filter.json directly
        let omf_filter_dir = Path::new("src/test/tmp_travel_mode_filter");
        let omf_filter_conf = crate::App::Pbf {
            pbf_file: pbf_file.to_string(),
            name: Some("schaan_travel_mode_filter".to_string()),
            extent_file: Some(extent_file.to_string()),
            configuration_file: Some(
                "../../configuration/bambam-omf/travel-mode-filter.json".to_string(),
            ),
            output_directory: omf_filter_dir.to_string_lossy().to_string(),
        };
        crate::run(&omf_filter_conf)
            .expect("bambam-osm failed with travel-mode-filter.json");
        assert!(omf_filter_dir.join("summary.toml").exists());
        for mode in ["walk", "bike", "drive"] {
            assert!(omf_filter_dir.join(mode).join("edges-compass.csv.gz").exists());
            assert!(omf_filter_dir.join(mode).join("edges-mapping.csv.gz").exists());
        }

        // cleanup before exits, order is important.
        if cleanup_tmp_dir {
            std::fs::remove_dir_all(temp_directory).expect("failed to remove tmp directory");
            std::fs::remove_dir_all(simplified_dir)
                .expect("failed to remove tmp_simplified directory");
            std::fs::remove_dir_all(omf_filter_dir)
                .expect("failed to remove tmp_travel_mode_filter directory");
        }
        if !disconnected.is_empty() {
            disconnected.iter().take(5).for_each(|d| println!("{d}"));
            panic!("files were written but some edges had invalid src/dst vertex ids (first 5 in log)");
        }
    }
}
