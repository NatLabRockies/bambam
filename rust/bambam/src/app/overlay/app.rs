use super::OverlayOperation;
use crate::app::overlay::{GeometryColumnType, Grouping, OverlaySource};
use csv::{Reader, StringRecord};
use geo::Geometry;
use itertools::Itertools;
use kdam::{tqdm, BarBuilder, BarExt};
use rayon::prelude::*;
use routee_compass_core::util::{fs::read_utils, geo::PolygonalRTree};
use std::{
    collections::HashMap,
    fs::File,
    path::Path,
    sync::{Arc, Mutex},
};

/// function to aggregate mep output rows to some overlay geometry dataset.
/// the number of output rows is not dependent on the size of the source geometry dataset,
/// instead based on the number of geometry rows with matches in the mep dataset.
/// only mep score and population data are aggregated at this time, via summation.
pub fn run(
    bambam_filepath: &str,
    output_directory: &str,
    overlay_source: &OverlaySource,
    col_type: &GeometryColumnType,
    _: &OverlayOperation,
    verbose: bool,
) -> Result<(), String> {
    // fail early if IO error from read/write destinations
    let output_directory = Path::new(output_directory);
    std::fs::create_dir_all(output_directory).map_err(|e| {
        format!(
            "failure creating output directory '{}': {e}",
            output_directory.as_os_str().to_string_lossy()
        )
    })?;

    // read overlay dataset
    let overlay_data = overlay_source.build()?;
    log::info!("found {} rows in overlay dataset", overlay_data.len());
    let overlay_lookup = overlay_data
        .iter()
        .map(|(geom, geoid)| (geoid.clone(), geom.clone()))
        .collect::<HashMap<_, _>>();
    let overlay: Arc<PolygonalRTree<f64, String>> = Arc::new(PolygonalRTree::new(overlay_data)?);

    let mut file = std::fs::File::open(bambam_filepath)
        .map_err(|e| format!("failure reading {bambam_filepath}: {e}"))?;
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .trim(csv::Trim::Fields)
        .from_reader(file);

    let headers = build_header_lookup(&mut reader)?;

    let grouped_rows: Vec<(String, (StringRecord, Geometry))> =
        spatial_lookup(reader, overlay.clone(), &headers, col_type, verbose)?;

    let mut grouped_lookup: HashMap<String, (Geometry, Vec<StringRecord>)> = HashMap::new();
    for (grouping, (row, geom)) in grouped_rows.into_iter() {
        match grouped_lookup.get_mut(&grouping) {
            Some((_, v)) => v.push(row),
            None => {
                let geometry = overlay_lookup.get(&grouping).ok_or_else(|| {
                    format!(
                        "internal error, lookup missing geometry entry for id '{}'",
                        grouping
                    )
                })?;
                let _ = grouped_lookup.insert(grouping.clone(), (geometry.clone(), vec![row]));
            }
        }
    }

    let len = grouped_lookup.len();
    let write_iter = tqdm!(
        grouped_lookup
            .into_iter()
            .sorted_by_cached_key(|(k, _)| k.clone()),
        desc = "writing partitioned datasets",
        total = len
    );
    for (id, (overlay, raw_rows)) in write_iter {
        let id_sani = sanitize_filename::sanitize(&id);
        let out_filename = format!("{id_sani}.csv");
        let out_filepath = output_directory.join(out_filename);
        let mut output_writer = csv::Writer::from_path(&out_filepath).map_err(|e| {
            format!(
                "failure opening output file '{}': {e}",
                out_filepath.as_os_str().to_string_lossy()
            )
        })?;

        for row in raw_rows.into_iter() {
            output_writer
                .write_record(&row)
                .map_err(|e| format!("failure writing row to output: {e}"))?;
        }
    }

    Ok(())
}

/// performs batch geospatial intersection operations to assign each [`MepRow`] its
/// grouping identifier (GEOID). run in parallel over the rows argument, a chunk of
/// the source MEP dataset.
fn spatial_lookup(
    reader: csv::Reader<File>,
    overlay: Arc<PolygonalRTree<f64, String>>,
    headers: &HashMap<String, usize>,
    col_type: &GeometryColumnType,
    verbose: bool,
) -> Result<Vec<(String, (csv::StringRecord, Geometry))>, String> {
    let iter = tqdm!(reader.into_records(), desc = "spatial lookup");

    let mut result = vec![];
    for (idx, row_result) in iter.enumerate() {
        let row = row_result.map_err(|e| format!("cannot read row {idx}: {e}"))?;
        let point = col_type
            .get_point(&row, headers)
            .map_err(|e| format!("failure reading geometry from row {idx}: {e}"))?;
        let found = overlay
            .intersection(&point)
            .map_err(|e| format!("failure running spatial intersection for row {idx}: {e}"))?
            .collect_vec();
        match found[..] {
            [single] => result.push((single.data.clone(), (row, point))),
            [] if verbose => {
                log::warn!("no spatial match found for row {idx}")
            }
            _ if verbose => {
                log::warn!(
                    "more than one spatial match ({} > 1) found for row {idx}",
                    found.len()
                )
            }
            _ => {}
        }
    }

    eprintln!();
    Ok(result)
}

pub fn build_header_lookup(reader: &mut Reader<File>) -> Result<HashMap<String, usize>, String> {
    // We nest this call in its own scope because of lifetimes.
    let headers = reader
        .headers()
        .map_err(|e| format!("failure retrieving headers: {e}"))?;
    let lookup: HashMap<String, usize> = headers
        .iter()
        .enumerate()
        .map(|(idx, col)| (String::from(col), idx))
        .collect::<HashMap<_, _>>();

    Ok(lookup)
}
