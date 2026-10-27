use std::{collections::HashSet, path::Path, sync::Arc};

pub use bambam_network::NetworkEdgeListConfiguration;
use geo::{BoundingRect, Geometry, Intersects};
use rayon::prelude::*;

use crate::{
    app::CliBoundingBox,
    collection::{
        ObjectStoreSource, OvertureMapsCollectionError, OvertureMapsCollectorConfig,
        ReleaseVersion, TransportationCollection,
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
    name: &str,
    bbox: Option<&CliBoundingBox>,
    modes: &[NetworkEdgeListConfiguration],
    output_directory: &Path,
    local_source: Option<&Path>,
    island_detection_configuration: Option<IslandDetectionAlgorithm>,
    extent: Option<Geometry<f32>>,
    infrastructure_match_distance_meters: Option<f64>,
) -> Result<(), OvertureMapsCollectionError> {
    let collection: TransportationCollection = match local_source {
        Some(src_path) => read_local(src_path),
        None => run_collector(bbox),
    }?;

    util::fs::create_dirs(output_directory)?;

    let collection = if let Some(ext_geom) = extent {
        apply_extent_to_collection(collection, ext_geom)
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
    let stats = NetworkGraphStats::try_from(&vectorized_graph)?;
    let source = NetworkGraphSource::new(&release, name, bbox);
    let summary = NetworkGraphSummary { source, stats };

    vectorized_graph.write_compass(&summary, output_directory, true)?;

    Ok(())
}

fn read_local(path: &Path) -> Result<TransportationCollection, OvertureMapsCollectionError> {
    TransportationCollection::from_local_source(path)
}

/// retrieve a TransportationCollection from a URL.
fn run_collector(
    bbox_arg: Option<&CliBoundingBox>,
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
        let msg = String::from("must provide bbox argument for download");
        OvertureMapsCollectionError::InvalidUserInput(msg)
    })?;
    log::info!(
        "running OMF import with
        object store {object_store:?}
        rg_chunk_size {rg_chunk_size}
        file_concurrency_limit {file_concurrency_limit}
        release {release}
        (xmin,xmax,ymin,ymax): {bbox}"
    );

    TransportationCollection::try_from_collector(collector, release, Some(bbox.into()))
}

/// filters segments and connectors in a transportation collection
/// using an arbitrary extent with the `intersect` predicate over the segments collection.
/// a second pass is performed over the connectors collection to reomve all connectors not referenced
/// by the remaining segments.
/// empty geometries are ignored (filtered out).
fn apply_extent_to_collection(
    collection: TransportationCollection,
    extent: Geometry<f32>,
) -> TransportationCollection {
    log::info!("Started applying extent to segments");
    let extent_arc = Arc::new(extent);
    let filtered_segments: Vec<crate::collection::TransportationSegmentRecord> = collection
        .segments
        .into_par_iter()
        .filter(|segment| match segment.get_linestring() {
            Ok(ls) => {
                let Some(bbox) = ls.bounding_rect() else {
                    return false;
                };

                // Short-circuit condition for bbox
                extent_arc.intersects(&bbox) && extent_arc.intersects(ls)
            }
            Err(_) => false,
        })
        .collect();

    log::info!("Collecting all connector IDs");
    let connector_ids = filtered_segments
        .par_iter()
        .flat_map(|segment| {
            segment
                .connectors
                .as_ref()
                .unwrap_or(&vec![])
                .iter()
                .map(|con_ref| con_ref.connector_id.clone())
                .collect::<Vec<String>>()
        })
        .collect::<HashSet<String>>();

    let arc_ids = Arc::new(connector_ids);

    log::info!("Started applying extent to connectors");
    let filtered_connectors = collection
        .connectors
        .into_par_iter()
        .filter(|connector| arc_ids.contains(&connector.id))
        .collect();

    log::info!("Started applying extent to infrastructure");
    let filtered_infrastructure = collection
        .infrastructure
        .into_par_iter()
        .filter(|infra| match infra.get_geometry() {
            Some(geom) => {
                let Some(bbox) = geom.bounding_rect() else {
                    return false;
                };
                extent_arc.intersects(&bbox) && extent_arc.intersects(geom)
            }
            None => false,
        })
        .collect();

    TransportationCollection {
        release: collection.release.clone(),
        connectors: filtered_connectors,
        segments: filtered_segments,
        infrastructure: filtered_infrastructure,
    }
}
