use std::path::Path;

pub use bambam_network::NetworkEdgeListConfiguration;

use crate::{
    collection::{
        Bbox, ObjectStoreSource, OvertureMapsCollectionError, OvertureMapsCollectorConfig,
        OvertureSpatialFilter, ReleaseVersion, RowFilterConfig, TransportationCollection,
    },
    graph::{
        island_detection::IslandDetectionAlgorithm, NetworkGraphSource, NetworkGraphStats,
        NetworkGraphSummary, OmfGraphVectorized,
    },
    util,
};

/// runs an OMF network import using the provided configuration.
#[allow(clippy::too_many_arguments)]
pub fn run(
    spatial_filter: Option<&OvertureSpatialFilter>,
    modes: &[NetworkEdgeListConfiguration],
    output_directory: &Path,
    local_source: Option<&Path>,
    island_detection_configuration: Option<IslandDetectionAlgorithm>,
    infrastructure_match_distance_meters: Option<f64>,
) -> Result<(), OvertureMapsCollectionError> {
    let collection: TransportationCollection = match local_source {
        Some(src_path) => read_local(src_path),
        None => run_collector(spatial_filter.map(|f| f.bbox())),
    }?;

    util::fs::create_dirs(output_directory)?;

    let collection = if let Some(filter) = spatial_filter {
        filter.filter_transportation_collection(collection)
    } else {
        collection
    };

    // Always retain the raw OMF dataset in `<output_directory>/raw/*.parquet`
    // unless `local_source` is already reading from that exact directory.
    let target_raw_dir = output_directory.join("raw");
    let same_raw_source = local_source.is_some_and(|src| {
        src == output_directory || src == target_raw_dir || src.parent() == Some(&target_raw_dir)
    });
    if !same_raw_source {
        collection.to_parquet(output_directory)?;
    }

    let vectorized_graph = OmfGraphVectorized::new_with_match_distance(
        &collection,
        modes,
        island_detection_configuration,
        infrastructure_match_distance_meters,
    )?;

    // summarize imported graph
    let release = match local_source {
        Some(local) => format!("file://{}", local.to_string_lossy()),
        None => collection.release.clone(),
    };
    let study_region = output_directory.to_string_lossy();
    let stats = NetworkGraphStats::try_from(&vectorized_graph)?;
    let cli_bbox = spatial_filter.map(|f| f.cli_bbox());
    let source = NetworkGraphSource::new(&release, &study_region, cli_bbox.as_ref());
    let summary = NetworkGraphSummary { source, stats };

    vectorized_graph.write_compass(&summary, output_directory, true)?;

    Ok(())
}

fn read_local(path: &Path) -> Result<TransportationCollection, OvertureMapsCollectionError> {
    TransportationCollection::from_local_source(path)
}

/// retrieve a TransportationCollection from a URL.
fn run_collector(
    bbox_arg: Option<Bbox>,
) -> Result<TransportationCollection, OvertureMapsCollectionError> {
    let object_store = ObjectStoreSource::AmazonS3;
    let rg_chunk_size = 4;
    let file_concurrency_limit = 64;
    let collector = OvertureMapsCollectorConfig::new(
        object_store,
        Some(rg_chunk_size),
        Some(file_concurrency_limit),
    )
    .build()?;
    let release = ReleaseVersion::Latest;
    let bbox = bbox_arg.ok_or_else(|| {
        let msg = String::from("must provide spatial filter / extent for download");
        OvertureMapsCollectionError::InvalidUserInput(msg)
    })?;
    log::info!(
        "running OMF import with
        object store {object_store:?}
        rg_chunk_size {rg_chunk_size}
        file_concurrency_limit {file_concurrency_limit}
        release {release}
        bbox: {bbox:?}"
    );

    TransportationCollection::try_from_collector(
        collector,
        release,
        Some(RowFilterConfig::from(bbox)),
    )
}
