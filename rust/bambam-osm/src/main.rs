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
        #[arg(long, help = "path to file containing WKT used to filter the PBF data")]
        extent_file: Option<String>,
        #[arg(long, help = "path to file with bambam-osm import parameters")]
        configuration_file: Option<String>,
        #[arg(long, help = "output path for network dataset")]
        output_directory: String,
    },
}

pub fn run(app: &App) -> Result<(), OsmCliError> {
    env_logger::init();
    match app {
        App::Pbf {
            pbf_file,
            extent_file,
            configuration_file, // network_filter,
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
            let pbf_config = OsmSource::Pbf {
                pbf_filepath: pbf_file.clone(),
                extent_filter_filepath: extent_file.clone(),
                network_filter: Some(conf.element_filter),
                component_filter: Some(conf.component_filter),
                truncate_by_edge: conf.truncate_by_edge,
                ignore_errors: conf.ignore_osm_parsing_errors,
                simplify: conf.simplify,
                infrastructure_match_distance_meters: conf.infrastructure_match_distance_meters,
            };
            let graph = pbf_config.import()?;
            match graph.write_compass(out_path, true) {
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

    use bambam_network::{EdgeMappingRecord, VertexMappingRecord};
    use bambam_osm::{
        algorithm::truncation::ComponentFilter,
        model::osm::{
            graph::{
                osm_element_filter::ElementFilter, CompassWriter, OsmNodeDataSerializable,
                OsmWayDataSerializable,
            },
            import_ops::OsmCollection,
            OsmSource,
        },
    };
    use routee_compass_core::util::fs::read_utils;

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

        // Verify standardized ID mappings
        let v_mappings: Box<[VertexMappingRecord]> =
            read_utils::from_csv(&"src/test/tmp/vertices-mapping.csv.gz", true, None, None)
                .expect("failed to read vertices-mapping.csv.gz");
        let e_mappings: Box<[EdgeMappingRecord]> =
            read_utils::from_csv(&"src/test/tmp/edges-mapping.csv.gz", true, None, None)
                .expect("failed to read edges-mapping.csv.gz");

        // test graph connectivity
        let mut disconnected: Vec<String> = vec![];
        let ways_result: Result<Box<[OsmWayDataSerializable]>, _> =
            read_utils::from_csv(&"src/test/tmp/edges-complete.csv.gz", true, None, None);
        let nodes_result: Result<Box<[OsmNodeDataSerializable]>, _> =
            read_utils::from_csv(&"src/test/tmp/vertices-complete.csv.gz", true, None, None);
        let invariant_error_msg = match (&ways_result, &nodes_result) {
            (Ok(_), Ok(_)) => None,
            (Ok(_), Err(e)) => Some(format!("failed to read nodes file: {e}")),
            (Err(e), Ok(_)) => Some(format!("failed to read ways file: {e}")),
            (Err(e1), Err(e2)) => Some(format!(
                "failed to read both nodes and ways files: {e1} {e2}"
            )),
        };
        if let (Ok(ways), Ok(nodes)) = (ways_result, nodes_result) {
            assert_eq!(v_mappings.len(), nodes.len());
            assert_eq!(e_mappings.len(), ways.len());
            for way in ways.iter() {
                let src_id = way.src_vertex_id.0;
                let way_id = way.osmid.0;
                let dst_id = way.dst_vertex_id.0;
                let way_img = format!("({src_id})-[{way_id}]->({dst_id})");
                let src = nodes.get(src_id);
                let dst = nodes.get(dst_id);
                match (src, dst) {
                    (None, Some(_)) => disconnected.push(format!("src missing from {way_img}")),
                    (Some(_), None) => disconnected.push(format!("dst missing from {way_img}")),
                    (None, None) => disconnected.push(format!("src/dst missing from {way_img}")),
                    (Some(_), Some(_)) => {}
                }
            }
        }

        // Also verify simplified + largest component import on the same Liechtenstein PBF
        let simplified_dir = Path::new("src/test/tmp_simplified");
        let simplified_source = OsmSource::Pbf {
            pbf_filepath: pbf_file.to_string(),
            network_filter: Some(ElementFilter::OsmnxAllPublic),
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
        assert!(!simplified_graph.nodes.is_empty());
        assert!(!simplified_graph.ways.is_empty());
        for way in &simplified_graph.ways {
            assert!(way.src_vertex_id.0 < simplified_graph.nodes.len());
            assert!(way.dst_vertex_id.0 < simplified_graph.nodes.len());
        }

        // cleanup before exits, order is important.
        if cleanup_tmp_dir {
            std::fs::remove_dir_all(temp_directory).expect("failed to remove tmp directory");
            std::fs::remove_dir_all(simplified_dir)
                .expect("failed to remove tmp_simplified directory");
        }
        // error if we couldn't read the output files or if the graph connectivity was invalid.
        match invariant_error_msg {
            None if !disconnected.is_empty() => {
                disconnected.iter().take(5).for_each(|d| println!("{d}"));
                panic!("files were written but some ways had invalid src/dst vertex ids (first 5 in log)")
            }
            Some(msg) => panic!("{msg}"),
            _ => {}
        }
    }
}
