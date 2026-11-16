use crate::{
    app::opp_totals::{
        is_json_path, serialize_config_fragment, write_or_print_config_fragment, TotalsNamespace,
    },
    model::output_plugin::opportunity::{
        geometry_to_wkt, proportion_dataset_to_vertices, OpportunityDataset,
        OpportunityModelConfig, OpportunitySource,
    },
};
use bambam_core::model::output_plugin::opportunity::OpportunityOrientation;
use flate2::{write::GzEncoder, Compression};
use itertools::Itertools;
use kdam::tqdm;
use serde_json::json;
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

pub const DEFAULT_GEOMETRIES_OPPORTUNITIES_FILENAME: &str = "opportunities-geometries.csv.gz";
pub const DEFAULT_VECTORIZED_OPPORTUNITIES_FILENAME: &str = "opportunities.csv.gz";
pub const DEFAULT_OPPORTUNITY_CONFIG_FILENAME: &str = "opportunity.toml";

/// Resolved output paths for an opportunity import run.
#[derive(Clone, Debug)]
pub struct OpportunityImportOutputs {
    /// Path to write the opportunity counts by geometry CSV (`.csv` or `.csv.gz`).
    pub geometries_output_file: String,
    /// Path to write the vertex-vectorized opportunity counts CSV (`.csv` or `.csv.gz`).
    pub opportunities_output_file: String,
    /// Optional path to write the `OpportunityModelConfig::FileSource` TOML/JSON fragment
    /// (prints to stdout when `None`).
    pub config_output_file: Option<String>,
}

impl OpportunityImportOutputs {
    /// Resolves output file paths from an optional `output_directory` and optional explicit paths.
    pub fn resolve(
        output_directory: Option<&str>,
        geometries_output_file: Option<&str>,
        opportunities_output_file: Option<&str>,
        config_output_file: Option<&str>,
    ) -> Result<Self, String> {
        let base_dir = output_directory.map(Path::new);

        let geometries_output_file = match (geometries_output_file, base_dir) {
            (Some(f), _) => f.to_string(),
            (None, Some(dir)) => dir
                .join(DEFAULT_GEOMETRIES_OPPORTUNITIES_FILENAME)
                .to_string_lossy()
                .into_owned(),
            (None, None) => {
                return Err(
                    "must provide either --output-directory or --geometries-output-file"
                        .to_string(),
                )
            }
        };

        let opportunities_output_file = match (opportunities_output_file, base_dir) {
            (Some(f), _) => f.to_string(),
            (None, Some(dir)) => dir
                .join(DEFAULT_VECTORIZED_OPPORTUNITIES_FILENAME)
                .to_string_lossy()
                .into_owned(),
            (None, None) => {
                return Err(
                    "must provide either --output-directory or --opportunities-output-file"
                        .to_string(),
                )
            }
        };

        let config_output_file = match (config_output_file, base_dir) {
            (Some(f), _) => Some(f.to_string()),
            (None, Some(dir)) => Some(
                dir.join(DEFAULT_OPPORTUNITY_CONFIG_FILENAME)
                    .to_string_lossy()
                    .into_owned(),
            ),
            (None, None) => None,
        };

        Ok(Self {
            geometries_output_file,
            opportunities_output_file,
            config_output_file,
        })
    }
}

/// Runs an [`OpportunitySource`] import from `source_config_file` and `vertices_file`,
/// producing:
/// 1. Opportunity counts by geometry CSV (`outputs.geometries_output_file`)
/// 2. Vertex-vectorized opportunity counts CSV (`outputs.opportunities_output_file`)
/// 3. TOML/JSON configuration fragment for [`OpportunityModelConfig::FileSource`]
///    (`outputs.config_output_file` or stdout)
pub fn run(
    source_config_file: &str,
    vertices_file: &str,
    activity_column_names: Option<Vec<String>>,
    table_orientation: Option<OpportunityOrientation>,
    namespace: TotalsNamespace,
    outputs: &OpportunityImportOutputs,
) -> Result<OpportunityModelConfig, String> {
    let (source, cfg_activities, cfg_orientation) =
        OpportunitySource::from_config_path(Path::new(source_config_file))?;

    let resolved_activities = match activity_column_names.or(cfg_activities) {
        Some(cols) if !cols.is_empty() => cols,
        _ => source.default_activity_column_names()?,
    };
    if resolved_activities.is_empty() {
        return Err(
            "no activity column names were provided or found in OpportunitySource configuration"
                .to_string(),
        );
    }

    let resolved_orientation = table_orientation
        .or(cfg_orientation)
        .unwrap_or(OpportunityOrientation::DestinationVertexOriented);

    let raw_dataset = source.generate_dataset(&resolved_activities)?;

    run_with_dataset(
        &raw_dataset,
        vertices_file,
        &resolved_activities,
        resolved_orientation,
        namespace,
        outputs,
    )
}

/// Executes the three-stage output pipeline given an already-collected [`OpportunityDataset`].
pub fn run_with_dataset(
    raw_dataset: &OpportunityDataset,
    vertices_file: &str,
    activity_column_names: &[String],
    table_orientation: OpportunityOrientation,
    namespace: TotalsNamespace,
    outputs: &OpportunityImportOutputs,
) -> Result<OpportunityModelConfig, String> {
    // 1. Write opportunity counts by geometry CSV
    write_geometry_opportunities_csv(
        raw_dataset,
        activity_column_names,
        &outputs.geometries_output_file,
    )?;

    // 2. Proportion opportunities across graph vertices and write vectorized CSV
    let activity_counts = proportion_dataset_to_vertices(
        raw_dataset,
        vertices_file,
        activity_column_names.len(),
    )
    .map_err(|e| e.to_string())?;

    write_vectorized_opportunities_csv(
        &activity_counts,
        activity_column_names,
        &outputs.opportunities_output_file,
    )?;

    // 3. Build and write the OpportunityModelConfig::FileSource configuration fragment
    let file_source_config = OpportunityModelConfig::FileSource {
        opportunity_input_file: outputs.opportunities_output_file.clone(),
        activity_column_names: activity_column_names.to_vec(),
        table_orientation,
    };

    let as_json = is_json_path(outputs.config_output_file.as_deref());
    let serialized = format_file_source_config(&file_source_config, namespace, as_json)?;
    write_or_print_config_fragment(
        &serialized,
        outputs.config_output_file.as_deref(),
        "opportunity file source config",
    )?;

    Ok(file_source_config)
}

/// Writes `raw_dataset` (geometries + activity counts) to a `.csv` or `.csv.gz` file
/// with header `geometry,<activity_column_names...>`.
pub fn write_geometry_opportunities_csv(
    raw_dataset: &OpportunityDataset,
    activity_column_names: &[String],
    output_file: &str,
) -> Result<(), String> {
    let mut header = Vec::with_capacity(activity_column_names.len() + 1);
    header.push("geometry".to_string());
    header.extend(activity_column_names.iter().cloned());

    let mut rows = Vec::with_capacity(raw_dataset.len());
    for (idx, (geom, counts)) in raw_dataset.iter().enumerate() {
        if counts.len() != activity_column_names.len() {
            return Err(format!(
                "geometry opportunity row {idx} has {} counts, expected {}",
                counts.len(),
                activity_column_names.len()
            ));
        }
        let wkt = geometry_to_wkt(geom)?;
        let mut record = Vec::with_capacity(counts.len() + 1);
        record.push(wkt);
        record.extend(counts.iter().map(|v| v.to_string()));
        rows.push(record);
    }

    write_csv_records(
        output_file,
        &header,
        rows,
        "writing geometry opportunities CSV",
    )
}

/// Writes vertex-vectorized `activity_counts` (indexed by `VertexId`) to a `.csv` or `.csv.gz` file
/// with header `<activity_column_names...>`.
pub fn write_vectorized_opportunities_csv(
    activity_counts: &[Vec<f64>],
    activity_column_names: &[String],
    output_file: &str,
) -> Result<(), String> {
    let mut rows = Vec::with_capacity(activity_counts.len());
    for (idx, counts) in activity_counts.iter().enumerate() {
        if counts.len() != activity_column_names.len() {
            return Err(format!(
                "vertex opportunity row {idx} has {} counts, expected {}",
                counts.len(),
                activity_column_names.len()
            ));
        }
        rows.push(counts.iter().map(|v| v.to_string()).collect_vec());
    }

    write_csv_records(
        output_file,
        activity_column_names,
        rows,
        "writing vectorized opportunities CSV",
    )
}

/// Shared CSV/CSV.GZ writer for opportunity tables.
fn write_csv_records(
    output_file: &str,
    header: &[String],
    rows: Vec<Vec<String>>,
    progress_desc: &str,
) -> Result<(), String> {
    let path = Path::new(output_file);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "failed to create parent directory '{}': {e}",
                    parent.display()
                )
            })?;
        }
    }

    let file = File::create(path)
        .map_err(|e| format!("failed to create output file '{output_file}': {e}"))?;
    let is_gz = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("gz"))
        .unwrap_or(false);
    let writer_stream: Box<dyn Write> = if is_gz {
        Box::new(GzEncoder::new(file, Compression::default()))
    } else {
        Box::new(BufWriter::new(file))
    };

    let mut csv_writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(writer_stream);

    csv_writer
        .write_record(header)
        .map_err(|e| format!("failed writing CSV header to '{output_file}': {e}"))?;

    let total = rows.len();
    let row_iter = tqdm!(
        rows.into_iter().enumerate(),
        total = total,
        desc = format!("{progress_desc} ({output_file})")
    );
    for (idx, row) in row_iter {
        csv_writer
            .write_record(&row)
            .map_err(|e| format!("failed writing row {idx} to '{output_file}': {e}"))?;
    }
    csv_writer
        .flush()
        .map_err(|e| format!("failed flushing CSV writer for '{output_file}': {e}"))?;

    Ok(())
}

/// Formats an [`OpportunityModelConfig::FileSource`] as a TOML or JSON configuration fragment.
pub fn format_file_source_config(
    config: &OpportunityModelConfig,
    namespace: TotalsNamespace,
    as_json: bool,
) -> Result<String, String> {
    let value = match namespace {
        TotalsNamespace::Standalone => serde_json::to_value(config)
            .map_err(|e| format!("failed to convert OpportunityModelConfig to JSON value: {e}"))?,
        TotalsNamespace::OutputPlugins => json!({
            "plugin": {
                "output_plugins": {
                    "model": config,
                }
            }
        }),
    };

    serialize_config_fragment(&value, as_json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::{polygon, Geometry, MultiPolygon, Point};
    use std::{io::Write, path::PathBuf};
    use tempfile::tempdir;

    fn configuration_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("test invariant failed: repo 'rust/bambam' dir has no parent")
            .parent()
            .expect("test invariant failed: repo 'rust' dir has no parent")
            .join("configuration")
    }

    #[test]
    fn test_opportunity_source_deserialization_and_default_activities() {
        let conf_dir = configuration_dir();

        // 1. Standalone OpportunitySource TOML
        let source_toml_path = conf_dir.join("lodes_opportunity_source.toml");
        let (src1, cfg_acts1, cfg_orient1) =
            OpportunitySource::from_config_path(&source_toml_path).unwrap();
        assert!(cfg_acts1.is_none());
        assert!(cfg_orient1.is_none());
        assert_eq!(
            src1.default_activity_column_names().unwrap(),
            vec![
                "entertainment".to_string(),
                "food".to_string(),
                "healthcare".to_string(),
                "jobs".to_string(),
                "retail".to_string(),
                "services".to_string(),
            ]
        );

        // 2. Wrapped inside OpportunityModelConfig::ApiSource under [[plugin.output_plugins]]
        let api_toml_path = conf_dir.join("boulder_test.toml");
        let (_src2, cfg_acts2, cfg_orient2) =
            OpportunitySource::from_config_path(&api_toml_path).unwrap();
        assert_eq!(
            cfg_acts2,
            Some(vec![
                "entertainment".to_string(),
                "food".to_string(),
                "retail".to_string(),
                "healthcare".to_string(),
                "services".to_string(),
                "jobs".to_string(),
            ])
        );
        assert_eq!(
            cfg_orient2,
            Some(OpportunityOrientation::DestinationVertexOriented)
        );
    }

    #[test]
    fn test_run_with_dataset_and_file_source_round_trip() {
        let dir = tempdir().unwrap();

        // Create a 3-vertex compass CSV:
        // vertices 0 and 1 are near (0.0, 0.0); vertex 2 is near (10.0, 10.0)
        let vertices_path = dir.path().join("vertices-compass.csv");
        {
            let mut f = File::create(&vertices_path).unwrap();
            writeln!(f, "vertex_id,x,y").unwrap();
            writeln!(f, "0,0.0,0.0").unwrap();
            writeln!(f, "1,0.1,0.1").unwrap();
            writeln!(f, "2,10.0,10.0").unwrap();
        }

        // Create an OpportunityDataset with 2 geometries:
        // - Geometry 0: Polygon centered near (0.0, 0.0) with [20.0 retail, 100.0 jobs]
        //   (matched by vertices 0 and 1 -> 10.0 retail, 50.0 jobs each)
        // - Geometry 1: MultiPolygon/Point at (10.0, 10.0) with [5.0 retail, 30.0 jobs]
        //   (matched by vertex 2 -> 5.0 retail, 30.0 jobs)
        let poly = polygon![
            (x: -1.0_f32, y: -1.0_f32),
            (x: 1.0_f32, y: -1.0_f32),
            (x: 1.0_f32, y: 1.0_f32),
            (x: -1.0_f32, y: 1.0_f32),
            (x: -1.0_f32, y: -1.0_f32),
        ];
        let mp = MultiPolygon::new(vec![polygon![
            (x: 9.0_f32, y: 9.0_f32),
            (x: 11.0_f32, y: 9.0_f32),
            (x: 11.0_f32, y: 11.0_f32),
            (x: 9.0_f32, y: 11.0_f32),
            (x: 9.0_f32, y: 9.0_f32),
        ]]);
        let raw_dataset: OpportunityDataset = vec![
            (Geometry::Polygon(poly), vec![20.0, 100.0]),
            (Geometry::MultiPolygon(mp), vec![5.0, 30.0]),
            (
                Geometry::Point(Point::new(100.0_f32, 100.0_f32)),
                vec![0.0, 0.0],
            ),
        ];

        let activities = vec!["retail".to_string(), "jobs".to_string()];
        let out_dir = dir.path().join("opp_out");
        let outputs = OpportunityImportOutputs::resolve(
            Some(out_dir.to_str().unwrap()),
            None,
            None,
            None,
        )
        .unwrap();

        let file_cfg = run_with_dataset(
            &raw_dataset,
            vertices_path.to_str().unwrap(),
            &activities,
            OpportunityOrientation::DestinationVertexOriented,
            TotalsNamespace::OutputPlugins,
            &outputs,
        )
        .unwrap();

        assert!(Path::new(&outputs.geometries_output_file).exists());
        assert!(Path::new(&outputs.opportunities_output_file).exists());
        let config_path = outputs.config_output_file.as_ref().unwrap();
        assert!(Path::new(config_path).exists());

        // Verify Output 1 (geometry counts CSV) headers and rows
        {
            let f = File::open(&outputs.geometries_output_file).unwrap();
            let mut rdr = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_reader(flate2::read::GzDecoder::new(f));
            let headers: Vec<String> = rdr.headers().unwrap().iter().map(String::from).collect();
            assert_eq!(headers, vec!["geometry", "retail", "jobs"]);
            let records: Vec<csv::StringRecord> = rdr.records().map(|r| r.unwrap()).collect();
            assert_eq!(records.len(), 3);
            assert!(records[0][0].starts_with("POLYGON"));
            assert_eq!(&records[0][1], "20");
            assert_eq!(&records[0][2], "100");
            assert!(records[1][0].starts_with("MULTIPOLYGON"));
            assert_eq!(&records[1][1], "5");
            assert_eq!(&records[1][2], "30");
            assert!(records[2][0].starts_with("POINT"));
            assert_eq!(&records[2][1], "0");
            assert_eq!(&records[2][2], "0");
        }

        // Verify Output 3 TOML fragment can be parsed back via OpportunityModelConfig::try_from
        // and built via OpportunityModelConfig::FileSource::build to read Output 2 (.csv.gz)
        let loaded_cfg = OpportunityModelConfig::try_from(Path::new(config_path)).unwrap();
        assert_eq!(
            serde_json::to_value(&loaded_cfg).unwrap(),
            serde_json::to_value(&file_cfg).unwrap()
        );

        let model = loaded_cfg.build().unwrap();
        match &model {
            crate::model::output_plugin::opportunity::OpportunityModel::Tabular {
                activity_types,
                activity_counts,
                ..
            } => {
                assert_eq!(activity_types, &activities);
                assert_eq!(
                    activity_counts,
                    &vec![vec![10.0, 50.0], vec![10.0, 50.0], vec![5.0, 30.0]]
                );
            }
            _ => panic!("expected Tabular OpportunityModel"),
        }
        let totals = model.opportunity_totals().unwrap();
        assert_eq!(totals.get("retail"), Some(&25.0));
        assert_eq!(totals.get("jobs"), Some(&130.0));
    }
}

