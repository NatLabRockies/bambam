use bambam::app::gtfs_flex_config::CliGtfsFlexConfigApp;
use bambam::app::network_import::{GeographicExtent, NetworkDataSource, NetworkImportRequest};
use bambam::app::oppvec::{self, oppvec_ops};
use bambam::app::overlay::{
    self, GeometryColumnType, GeometryFormat, OverlayOperation, OverlaySource,
};
use bambam_congestion::{CalibrateCliArgs, ImputeCliArgs};
use bambam_modal_metrics::common::bulk_compute_modal_metric::bulk_compute_modal_metric_with_context;
use bambam_modal_metrics::common::modal_metrics::ModalMetric;
use bambam_network::{
    EdgeCompleteRecord, VertexCompleteRecord, COMPLETE_EDGES_FILENAME, COMPLETE_VERTEX_FILENAME,
};
use bambam_osm::model::osm::graph::{OsmNodeDataSerializable, OsmWayDataSerializable};
use clap::{Parser, Subcommand};
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct CliArgs {
    #[command(subcommand)]
    app: App,
}

use bambam::model::input_plugin::grid::extent_format::ExtentFormat;
use bambam::model::input_plugin::grid::grid_input_plugin;
use bambam::model::input_plugin::grid::grid_input_plugin_builder;
use bambam::model::input_plugin::grid::grid_type::GridType;
use bambam::model::input_plugin::population::population_source_config::PopulationSourceConfig;
use bamcensus_acs::model::AcsType;
use bamcensus_core::model::identifier::GeoidType;
use h3o::Resolution;
use serde_json::json;
use std::fs::File;
use std::io::BufWriter;
use std::io::Write;

#[derive(Subcommand)]
pub enum App {
    #[command(
        name = "modal_metric",
        about = "calculate the modal metric (WCI or LTS) of each edge in an OMF or OSM network, write to file"
    )]
    ModalMetricSet {
        /// modal metric type to compute, either "WCI" or "LTS"
        #[arg(long)]
        modal_metric: ModalMetric,
        /// optional network directory produced by bambam-omf or bambam-osm (resolves vertices-complete.csv.gz and <mode>/edges-complete.csv.gz)
        #[arg(long)]
        network_dir: Option<String>,
        /// optional target mode subdirectory when --network-dir is used (defaults to "walk" for WCI, "bike" for LTS)
        #[arg(long)]
        mode: Option<String>,
        /// input csv / csv.gz file with edges data (required when --network-dir is not set)
        #[arg(long)]
        edges_file: Option<String>,
        /// input csv / csv.gz file with vertices data (required when --network-dir is not set)
        #[arg(long)]
        vertices_file: Option<String>,
        /// optional comma-separated list of additional edges-complete.csv[.gz] files to load into the spatial R-tree for neighbor queries
        #[arg(long)]
        context_edges_files: Option<String>,
        /// optional schema source override: "omf" (EdgeCompleteRecord/VertexCompleteRecord) or "osm" (legacy OsmWayDataSerializable/OsmNodeDataSerializable). Auto-detected from CSV header if omitted.
        #[arg(long)]
        source: Option<String>,
        /// file to write modal metric values to (.csv or .csv.gz)
        #[arg(long)]
        output_file: String,
    },
    #[command(
        name = "preprocess_grid",
        about = "processs the grid before running bambam to avoid time-out errors"
    )]
    PreProcessGrid {
        /// 1 or 5 for one- or five-year ACS (American Community Survey) population estimate
        #[arg(long)]
        acs_type: AcsType,
        /// year for estimate
        #[arg(long)]
        acs_year: u64,
        /// (Optional) GeoidType for resolution
        #[arg(long)]
        acs_resolution: Option<GeoidType>,
        /// (Optional) String for comma-separated categories
        #[arg(long)]
        acs_categories: Option<String>,
        /// census.gov token for ACS data access (required as of 06/2026). must be passed
        /// via the CENSUS_API_TOKEN environment variable when calling this application.
        #[arg(env = "CENSUS_API_TOKEN")]
        api_token: String,
        /// format of the extent in ExtentFormat, Wkt
        #[arg(long)]
        extent_format: ExtentFormat,
        /// Resolution of grid, value 0-15
        #[arg(long)]
        grid_resolution: Resolution,
        /// String of desired output file location
        #[arg(long)]
        output_file: String,
        /// String of extent to examine stored in a text file
        #[arg(long)]
        extent_file: String,
    },
    #[command(
        name = "opps-long",
        about = "vectorize an opportunity dataset CSV in long format for bambam integration"
    )]
    OpportunitiesLongFormat {
        /// a vertices-compass.csv.gz file for a RouteE Compass dataset
        vertices_compass_filename: String,
        /// a CSV file containing opportunities and geometries in long format
        opportunities_filename: String,
        /// file to write resulting opportunities dataset, designed to be a tabular
        /// opportunity input to bambam.
        output_filename: String,
        /// column name containing x coordinates. cannot be used when "geometry_column" is specified.
        #[arg(long)]
        x_column: Option<String>,
        /// column name containing y coordinates. cannot be used when "geometry_column" is specified.
        #[arg(long)]
        y_column: Option<String>,
        /// column name containing WKT geometry. cannot be used when x|y columns are specified.
        #[arg(long)]
        geometry_column: Option<String>,

        /// column name containing activity category name
        #[arg(long)]
        category_column: String,

        // /// optional column name containing activity counts. if omitted, counts each row as 1 opportunity.
        #[arg(long)]
        count_column: Option<String>,
        /// mapping from category name to activity type as comma-delimited string of "cat->acts" statements, where
        /// "cat" is the source category name, and "acts" is a hyphen-delimited non-empty list of target activity categories.
        /// wrap the entire argument in double-quotes. example: "CNS07->retail-jobs,CNS16->healthcare-jobs,CNS05->jobs"
        #[arg(long)]
        category_mapping: String,
        // // / comma-delimited list of categories to keep
        // #[arg(long)]
        // activity_categories: String,
    },
    #[command(
        name = "opps-wide",
        about = "vectorize an opportunity dataset CSV for bambam integration"
    )]
    OpportunitiesWideFormat {
        // source_format: SourceFormat,
        /// a vertices-compass.csv.gz file for a RouteE Compass dataset
        vertices_compass_filename: String,
        /// a CSV file containing opportunities and geometries in long format
        opportunities_filename: String,
        /// file to write resulting opportunities dataset, designed to be a tabular
        /// opportunity input to bambam.
        output_filename: String,
        /// column name containing x coordinates. cannot be used when "geometry_column" is specified.
        #[arg(long)]
        x_column: Option<String>,
        /// column name containing y coordinates. cannot be used when "geometry_column" is specified.
        #[arg(long)]
        y_column: Option<String>,
        /// column name containing WKT geometry. cannot be used when x|y columns are specified.
        #[arg(long)]
        geometry_column: Option<String>,
        /// mapping from column name to activity type as comma-delimited string of "col->acts" statements, where
        /// "col" is the source column name, and "acts" is a hyphen-delminited non-empty list of target activity categories.
        /// example: "CNS07->retail-jobs,CNS16->healthcare-jobs,CNS05->jobs"
        #[arg(long)]
        column_mapping: String,
        // /// comma-delimited list of categories to keep
        // #[arg(long)]
        // activity_categories: String,
    },
    #[command(
        name = "overlay-shapefile",
        about = "partition the BAMBAM output into another dataset of geospatial boundaries"
    )]
    OverlayShapefile {
        /// a CSV file containing a bambam output
        bambam_output_filepath: String,
        /// a path to the overlay boundary dataset, such as a shapefile
        overlay_filepath: String,
        /// file path to write the result dataset
        output_directory: String,
        /// used for specifying column name for the geometry x value. do not combine with
        /// geomcol argument.
        #[arg(long)]
        xcol: Option<String>,
        /// used for specifying column name for the geometry y value. do not combine with
        /// geomcol argument.
        #[arg(long)]
        ycol: Option<String>,
        /// used for specifying column name for the geometry. do not combine with xcol or
        /// ycol arguments.
        #[arg(long)]
        geomcol: Option<String>,
        /// used for specifying the geometry type of the value stored at geomcol do not combine
        /// with xcol or ycol arguments.
        #[arg(long)]
        geomfmt: Option<GeometryFormat>,
        /// overlay method to apply
        #[arg(long, default_value_t = OverlayOperation::Intersection)]
        how: OverlayOperation,
        /// name of the id field in the shapefile, used to create the output filepath
        /// for each partitioned dataset. values will be re-encoded for the filesystem,
        /// removing values such as forward slashes that could have unintended effects.
        #[arg(long, default_value_t = String::from("GEOID"))]
        id_field: String,
        /// if true, log if any rows fail to match the provided overlay dataset
        #[arg(long)]
        verbose: bool,
    },
    #[command(
        name = "gtfs-config",
        about = "modifies a BAMBAM configuration file to incorporate a directory of GTFS data assets generated by bambam-transit"
    )]
    GtfsConfigApp {
        /// OS directory containing processed BAMBAM-GTFS files
        #[arg(long)]
        directory: String,
        /// path to the BAMBAM configuration file to augment with this set of GTFS data
        #[arg(long)]
        base_config: String,
        /// append a relative path as a prefix for any "*input_file" entries referenced within the BAMBAM configuration file
        /// for the transit-mode. can be None if there is no relative path present.
        #[arg(long)]
        inject_filepath: Option<String>,
    },
    GtfsFlexConfigApp(CliGtfsFlexConfigApp),
    #[command(
        name = "import-network",
        about = "Import a RouteE Compass road network from Overture Maps or OpenStreetMap over a geographic extent"
    )]
    ImportNetwork {
        /// Data source: "omf" ("overture") or "osm"
        #[arg(short, long)]
        source: String,
        /// Output directory for Compass network, ID mappings, and raw Parquet tables
        #[arg(short, long)]
        output_directory: String,
        /// Optional configuration file (e.g., configuration/bambam-network/bambam-network-config.json)
        #[arg(short, long)]
        configuration_file: Option<String>,
        /// Optional WKT file path defining the study extent
        #[arg(short, long)]
        extent_file: Option<String>,
        /// Optional inline WKT string defining the study extent
        #[arg(long)]
        extent_wkt: Option<String>,
        /// Optional bounding box "xmin,ymin,xmax,ymax"
        #[arg(long)]
        bbox: Option<String>,
        /// For OSM: path to .osm.pbf file (or retained raw/ Parquet directory); for OMF: optional local raw directory/file
        #[arg(short, long)]
        input_path: Option<String>,
        /// Optional comma-separated travel modes (e.g., "walk,bike,drive") when --configuration-file is not provided
        #[arg(short, long)]
        modes: Option<String>,
        /// Optional maximum distance in meters for snapping infrastructure to vertices (default 15.0)
        #[arg(long)]
        infrastructure_match_distance_meters: Option<f64>,
    },
    #[command(
        name = "congestion-calibrate",
        about = "Conflate an Open Network (OSM/OMF) with TomTom MultiNet-R speeds and export calibration training tables"
    )]
    CongestionCalibrate(CalibrateCliArgs),
    #[command(
        name = "congestion-impute",
        about = "Impute operating speeds across all 6 time periods for a BAMBAM / RouteE Compass road network"
    )]
    CongestionImpute(ImputeCliArgs),
}

impl App {
    pub fn run(&self) -> Result<(), String> {
        let _ = env_logger::try_init();
        match self {
            Self::ModalMetricSet {
                modal_metric,
                network_dir,
                mode,
                edges_file,
                vertices_file,
                context_edges_files,
                source,
                output_file,
            } => {
                let default_mode = match modal_metric {
                    ModalMetric::WalkingComfortIndex => "walk",
                    ModalMetric::LevelOfTrafficStress => "bike",
                };
                let target_mode = mode.as_deref().unwrap_or(default_mode);

                let (resolved_edges_file, resolved_vertices_file, mut resolved_context_files) =
                    if let Some(net_dir) = network_dir {
                        let root = std::path::Path::new(net_dir);
                        let e_file = edges_file.clone().unwrap_or_else(|| {
                            root.join(target_mode)
                                .join(COMPLETE_EDGES_FILENAME)
                                .to_string_lossy()
                                .into_owned()
                        });
                        let v_file = vertices_file.clone().unwrap_or_else(|| {
                            root.join(COMPLETE_VERTEX_FILENAME)
                                .to_string_lossy()
                                .into_owned()
                        });
                        let mut ctx = Vec::new();
                        for sibling_mode in ["walk", "bike", "drive"] {
                            let candidate = root.join(sibling_mode).join(COMPLETE_EDGES_FILENAME);
                            let candidate_str = candidate.to_string_lossy().into_owned();
                            if candidate.exists() && candidate_str != e_file {
                                ctx.push(candidate_str);
                            }
                        }
                        (e_file, v_file, ctx)
                    } else {
                        let e_file = edges_file.clone().ok_or_else(|| {
                            "must provide either --network-dir or both --edges-file and --vertices-file"
                                .to_string()
                        })?;
                        let v_file = vertices_file.clone().ok_or_else(|| {
                            "must provide either --network-dir or both --edges-file and --vertices-file"
                                .to_string()
                        })?;
                        let mut ctx = Vec::new();
                        // If edges_file is inside a mode subdirectory (<root>/<mode>/edges-complete.csv.gz),
                        // automatically include any sibling mode edges-complete.csv.gz files as R-tree context.
                        if let Some(root) = std::path::Path::new(&e_file)
                            .parent()
                            .and_then(|p| p.parent())
                        {
                            for sibling_mode in ["walk", "bike", "drive"] {
                                let candidate =
                                    root.join(sibling_mode).join(COMPLETE_EDGES_FILENAME);
                                let candidate_str = candidate.to_string_lossy().into_owned();
                                if candidate.exists() && candidate_str != e_file {
                                    ctx.push(candidate_str);
                                }
                            }
                        }
                        (e_file, v_file, ctx)
                    };

                if let Some(explicit_ctx) = context_edges_files {
                    resolved_context_files = explicit_ctx
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }

                let use_complete_records = match source
                    .as_deref()
                    .map(|s| s.trim().to_ascii_lowercase())
                    .as_deref()
                {
                    Some("omf" | "overture" | "overture_maps" | "compass" | "network") => true,
                    Some("osm" | "openstreetmap" | "legacy_osm") => false,
                    Some(other) => {
                        return Err(format!(
                            "unsupported --source '{other}'; expected 'omf' or 'osm'"
                        ))
                    }
                    None => is_edge_complete_record_csv(&resolved_edges_file)?,
                };

                if use_complete_records {
                    bulk_compute_modal_metric_with_context::<
                        EdgeCompleteRecord,
                        VertexCompleteRecord,
                    >(
                        *modal_metric,
                        &resolved_edges_file,
                        &resolved_vertices_file,
                        &resolved_context_files,
                        output_file,
                    )
                    .map_err(|e| format!("failed to run bulk compute modal metric: {e:?}"))
                } else {
                    bulk_compute_modal_metric_with_context::<
                        OsmWayDataSerializable,
                        OsmNodeDataSerializable,
                    >(
                        *modal_metric,
                        &resolved_edges_file,
                        &resolved_vertices_file,
                        &resolved_context_files,
                        output_file,
                    )
                    .map_err(|e| format!("failed to run bulk compute modal metric: {e:?}"))
                }
            }
            Self::PreProcessGrid {
                acs_type,
                acs_year,
                acs_resolution,
                acs_categories,
                api_token,
                extent_format,
                grid_resolution,
                output_file,
                extent_file,
            } => {
                // build acs categories from &Option<String> to Option<Vec<String>>
                let acs_categories: Option<Vec<String>> = acs_categories
                    .as_ref()
                    .map(|str| str.split(',').map(|elem| elem.trim().to_string()).collect());

                // create popconfig
                let pop_config = PopulationSourceConfig::UsCensusAcs {
                    acs_type: *acs_type,
                    acs_year: *acs_year,
                    acs_resolution: *acs_resolution,
                    acs_categories,
                    api_token: Ok(api_token.clone()),
                };

                // Using grid_resolution, build grid_type:Gridtype
                let grid_res_add = *grid_resolution;
                let grid_type = GridType::H3 {
                    resolution: grid_res_add,
                };

                let extent = std::fs::read_to_string(extent_file)
                    .map_err(|e| format!("failed to read extent file: {e}"))?;

                // unpack the command line arguments into serde_json::Values
                let mut data: serde_json::Value = json!({
                    "extent": extent,
                    "population_source": pop_config,
                    "extent_format": extent_format,
                    "grid": grid_type,
                    "output_file": output_file
                });

                // BUILD THE PLUGIN
                let plugin = grid_input_plugin_builder::plugin_builder(&data)
                    .map_err(|e| format!("failed to read grid model arguments: {e}"))?;

                // RUN GRID BUILDING PROCESS
                grid_input_plugin::process_grid_input(
                    &mut data,
                    plugin.extent_format,
                    plugin.grid_type,
                    &plugin.population_source,
                )
                .map_err(|e| format!("failure running grid processing: {e}"))?;

                // mutable data as input to process_grid_input becomes a json array
                // these 3 lines make sure the resulting data array is json, if it is, we have an array to loop through
                let array = match data.as_array() {
                    Some(a) => a,
                    None => {
                        eprintln!(
                            "{}",
                            serde_json::to_string_pretty(&data).unwrap_or_default()
                        );
                        return Err(
                            "resulting data is not an array of JSON as expected".to_string()
                        );
                    }
                };

                // write the resulting array to the output file location as newline-delimited JSON
                let file = File::create(output_file).map_err(|e| e.to_string())?;
                let mut writeto = BufWriter::new(file);
                for value in array {
                    let json_line = serde_json::to_string(value).map_err(|e| e.to_string())?;
                    writeln!(writeto, "{json_line}").map_err(|e| e.to_string())?;
                }
                println!("Wrote newline-delimited JSON to {output_file}");
                Ok(())
            }
            Self::OpportunitiesLongFormat {
                vertices_compass_filename,
                opportunities_filename,
                output_filename,
                geometry_column,
                x_column,
                y_column,
                category_column,
                count_column,
                category_mapping,
            } => {
                let geometry_format = oppvec::GeometryFormat::new(
                    geometry_column.as_ref(),
                    x_column.as_ref(),
                    y_column.as_ref(),
                )?;
                let category_mapping = oppvec_ops::create_mapping(category_mapping)?;
                log::debug!(
                    "category mapping:\n{}",
                    serde_json::to_string_pretty(&category_mapping).unwrap_or_default()
                );
                let source_format = oppvec::SourceFormat::LongFormat {
                    geometry_format,
                    category_column: category_column.clone(),
                    count_column: count_column.clone(),
                    category_mapping,
                };
                oppvec::run(
                    vertices_compass_filename,
                    opportunities_filename,
                    output_filename,
                    &source_format,
                    // &cats,
                )
            }
            Self::OpportunitiesWideFormat {
                vertices_compass_filename,
                opportunities_filename,
                output_filename,
                geometry_column,
                x_column,
                y_column,
                column_mapping,
                // activity_categories,
            } => {
                let geometry_format = oppvec::GeometryFormat::new(
                    geometry_column.as_ref(),
                    x_column.as_ref(),
                    y_column.as_ref(),
                )?;
                if column_mapping.is_empty() {
                    return Err(String::from(
                        "cannot build wide-format source with empty column mapping",
                    ));
                }
                let column_mapping = oppvec_ops::create_mapping(column_mapping)?;
                log::debug!(
                    "column mapping:\n{}",
                    serde_json::to_string_pretty(&column_mapping).unwrap_or_default()
                );
                let source_format = oppvec::SourceFormat::WideFormat {
                    geometry_format,
                    column_mapping,
                };
                oppvec::run(
                    vertices_compass_filename,
                    opportunities_filename,
                    output_filename,
                    &source_format,
                )
            }
            Self::GtfsConfigApp {
                directory,
                base_config: base_config_filepath,
                inject_filepath: base_config_relative_path,
            } => bambam::app::gtfs_config::run(
                directory,
                base_config_filepath,
                base_config_relative_path.as_deref(),
            )
            .map_err(|e| e.to_string()),

            Self::OverlayShapefile {
                bambam_output_filepath,
                overlay_filepath,
                output_directory,
                xcol: x_column,
                ycol: y_column,
                geomcol,
                geomfmt: geomformat,
                how,
                id_field,
                verbose,
            } => {
                let col_type = GeometryColumnType::new(
                    x_column.as_ref(),
                    y_column.as_ref(),
                    geomcol.as_ref(),
                    geomformat.as_ref(),
                )?;
                let overlay_source = OverlaySource::Shapefile {
                    file: overlay_filepath.clone(),
                    id_field: id_field.clone(),
                };
                overlay::run(
                    bambam_output_filepath,
                    output_directory,
                    &overlay_source,
                    &col_type,
                    how,
                    *verbose,
                )
            }
            App::GtfsFlexConfigApp(app) => app
                .clone() // shouldn't happen, App::run should pass owned self.
                .run()
                .map_err(|e| format!("failure running gtfs-flex config app due to {e}")),
            App::ImportNetwork {
                source,
                output_directory,
                configuration_file,
                extent_file,
                extent_wkt,
                bbox,
                input_path,
                modes,
                infrastructure_match_distance_meters,
            } => {
                let extent = if let Some(path) = extent_file {
                    GeographicExtent::WktFile { path: path.clone() }
                } else if let Some(wkt) = extent_wkt {
                    GeographicExtent::Wkt { wkt: wkt.clone() }
                } else if let Some(bbox_str) = bbox {
                    let parts: Vec<f32> = bbox_str
                        .split(',')
                        .map(|s| {
                            s.trim()
                                .parse::<f32>()
                                .map_err(|e| format!("invalid bbox float '{s}': {e}"))
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let [xmin, ymin, xmax, ymax] = parts.as_slice() else {
                        return Err("expected --bbox in 'xmin,ymin,xmax,ymax' format".to_string());
                    };
                    GeographicExtent::Bbox {
                        xmin: *xmin,
                        ymin: *ymin,
                        xmax: *xmax,
                        ymax: *ymax,
                    }
                } else {
                    return Err(
                        "must provide one of --extent-file, --extent-wkt, or --bbox".to_string()
                    );
                };

                let data_source = match source.trim().to_ascii_lowercase().as_str() {
                    "omf" | "overture" | "overture_maps" => NetworkDataSource::OvertureMaps {
                        local_source: input_path.clone(),
                    },
                    "osm" | "openstreetmap" | "open_street_map" => {
                        let pbf_path = input_path.clone().ok_or_else(|| {
                            "must provide --input-path pointing to .osm.pbf file (or raw/ Parquet directory) when --source is osm"
                                .to_string()
                        })?;
                        NetworkDataSource::OpenStreetMap {
                            pbf_path,
                            local_source: None,
                            simplify: Some(true),
                            truncate_by_edge: Some(true),
                        }
                    }
                    other => {
                        return Err(format!(
                            "unsupported --source '{other}'; expected 'omf' or 'osm'"
                        ))
                    }
                };

                let parsed_modes = modes.as_ref().map(|m| {
                    m.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                });

                let req = NetworkImportRequest {
                    extent,
                    source: data_source,
                    output_directory: output_directory.clone(),
                    configuration_file: configuration_file.clone(),
                    modes: parsed_modes,
                    infrastructure_match_distance_meters: *infrastructure_match_distance_meters,
                };
                req.run()
            }
            App::CongestionCalibrate(args) => {
                let cfg = args.resolve_config().map_err(|e| e.to_string())?;
                bambam_congestion::run_calibration(&cfg)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            App::CongestionImpute(args) => {
                let cfg = args.resolve_config().map_err(|e| e.to_string())?;
                bambam_congestion::run_imputation(&cfg)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
        }
    }
}

/// Inspects the header row of `edges_file` (`.csv` or `.csv.gz`) to determine whether it
/// follows the `EdgeCompleteRecord` schema (`edge_id,source_id,...`) or the legacy
/// `OsmWayDataSerializable` schema (`osmid,...`).
fn is_edge_complete_record_csv(edges_file: &str) -> Result<bool, String> {
    use flate2::read::GzDecoder;
    use std::io::{BufRead, BufReader};

    let file = File::open(edges_file)
        .map_err(|e| format!("failed to open edges file '{edges_file}': {e}"))?;
    let mut reader: Box<dyn BufRead> = if edges_file.ends_with(".gz") {
        Box::new(BufReader::new(GzDecoder::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };

    let mut header_line = String::new();
    reader
        .read_line(&mut header_line)
        .map_err(|e| format!("failed reading header from '{edges_file}': {e}"))?;

    let cols: Vec<&str> = header_line
        .trim()
        .split(',')
        .map(|c| c.trim().trim_matches('"'))
        .collect();
    Ok(cols.contains(&"source_id") || cols.first().copied() == Some("edge_id"))
}

fn main() {
    let args = CliArgs::parse();
    match args.app.run() {
        Ok(_) => log::info!("bambam_util finished."),
        Err(e) => log::error!("{e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bambam_network::{
        fs::serialize_into_csv, SegmentFullType, SegmentHeading, COMPLETE_EDGES_FILENAME,
        COMPLETE_VERTEX_FILENAME,
    };
    use geo::LineString;
    use std::{fs, path::Path, str::FromStr};

    #[test]
    fn test_modal_metric_cli_with_omf_network_dir() {
        let tmp_dir = Path::new("target/test_tmp_bambam_util_modal_metric");
        let _ = fs::remove_dir_all(tmp_dir);
        let walk_dir = tmp_dir.join("walk");
        let bike_dir = tmp_dir.join("bike");
        let drive_dir = tmp_dir.join("drive");
        fs::create_dir_all(&walk_dir).unwrap();
        fs::create_dir_all(&bike_dir).unwrap();
        fs::create_dir_all(&drive_dir).unwrap();

        let vertices = vec![
            VertexCompleteRecord {
                vertex_id: 0,
                connector_id: "conn-0".to_string(),
                x: -105.170735,
                y: 39.773087,
                traffic_signals: true,
                stop: false,
                give_way: false,
                crossing: true,
                infrastructure_ids: None,
            },
            VertexCompleteRecord {
                vertex_id: 1,
                connector_id: "conn-1".to_string(),
                x: -105.170445,
                y: 39.773137,
                traffic_signals: false,
                stop: false,
                give_way: false,
                crossing: false,
                infrastructure_ids: None,
            },
        ];
        serialize_into_csv(
            vertices.iter(),
            COMPLETE_VERTEX_FILENAME,
            tmp_dir,
            true,
            "write vertices",
        )
        .unwrap();

        let walk_edge = EdgeCompleteRecord {
            edge_id: 0,
            source_id: "seg-walk-0".to_string(),
            start_lr: 0.0,
            end_lr: 1.0,
            heading: SegmentHeading::Forward,
            src_vertex_id: 0,
            dst_vertex_id: 1,
            distance: 100.0,
            class: SegmentFullType::from_str("road-residential").unwrap(),
            posted_speed_mph: None,
            speed_mph: 25.0,
            bearing: 90.0,
            oneway: false,
            linestring: LineString::from(vec![(-105.170735, 39.773087), (-105.170445, 39.773137)]),
        };
        let bike_edge = EdgeCompleteRecord {
            edge_id: 0,
            source_id: "seg-bike-0".to_string(),
            start_lr: 0.0,
            end_lr: 1.0,
            heading: SegmentHeading::Forward,
            src_vertex_id: 0,
            dst_vertex_id: 1,
            distance: 100.0,
            class: SegmentFullType::from_str("road-cycleway").unwrap(),
            posted_speed_mph: None,
            speed_mph: 15.0,
            bearing: 90.0,
            oneway: false,
            linestring: LineString::from(vec![(-105.170612, 39.773116), (-105.170499, 39.773017)]),
        };
        let drive_edge = EdgeCompleteRecord {
            edge_id: 0,
            source_id: "seg-drive-0".to_string(),
            start_lr: 0.0,
            end_lr: 1.0,
            heading: SegmentHeading::Forward,
            src_vertex_id: 0,
            dst_vertex_id: 1,
            distance: 100.0,
            class: SegmentFullType::from_str("road-secondary").unwrap(),
            posted_speed_mph: Some(25.0),
            speed_mph: 25.0,
            bearing: 90.0,
            oneway: false,
            linestring: LineString::from(vec![(-105.170735, 39.773090), (-105.170445, 39.773140)]),
        };

        serialize_into_csv(
            [walk_edge].iter(),
            COMPLETE_EDGES_FILENAME,
            &walk_dir,
            true,
            "write walk edges",
        )
        .unwrap();
        serialize_into_csv(
            [bike_edge].iter(),
            COMPLETE_EDGES_FILENAME,
            &bike_dir,
            true,
            "write bike edges",
        )
        .unwrap();
        serialize_into_csv(
            [drive_edge].iter(),
            COMPLETE_EDGES_FILENAME,
            &drive_dir,
            true,
            "write drive edges",
        )
        .unwrap();

        let wci_out = walk_dir.join("wci.csv").to_string_lossy().into_owned();
        let wci_app = App::ModalMetricSet {
            modal_metric: ModalMetric::WalkingComfortIndex,
            network_dir: Some(tmp_dir.to_string_lossy().into_owned()),
            mode: None,
            edges_file: None,
            vertices_file: None,
            context_edges_files: None,
            source: None,
            output_file: wci_out.clone(),
        };
        wci_app.run().unwrap();
        let wci_lines: Vec<String> = fs::read_to_string(&wci_out)
            .unwrap()
            .lines()
            .map(String::from)
            .collect();
        assert_eq!(wci_lines.len(), 2);
        assert_eq!(wci_lines[1], "3,-2,2,1,2");

        let lts_out = bike_dir.join("lts.csv").to_string_lossy().into_owned();
        let lts_app = App::ModalMetricSet {
            modal_metric: ModalMetric::LevelOfTrafficStress,
            network_dir: Some(tmp_dir.to_string_lossy().into_owned()),
            mode: None,
            edges_file: None,
            vertices_file: None,
            context_edges_files: None,
            source: None,
            output_file: lts_out.clone(),
        };
        lts_app.run().unwrap();
        let lts_lines: Vec<String> = fs::read_to_string(&lts_out)
            .unwrap()
            .lines()
            .map(String::from)
            .collect();
        assert_eq!(lts_lines.len(), 2);
        assert_eq!(lts_lines[1], "1");

        let _ = fs::remove_dir_all(tmp_dir);
    }
}
