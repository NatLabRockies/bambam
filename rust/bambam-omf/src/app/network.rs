use std::{collections::HashSet, path::Path, sync::Arc};

use geo::{BoundingRect, Geometry, Intersects};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    app::CliBoundingBox,
    collection::{
        filter::{MatchBehavior, TravelModeFilter},
        record::SegmentMode,
        ObjectStoreSource, OvertureMapsCollectionError, OvertureMapsCollectorConfig,
        ReleaseVersion, SegmentAccessRestrictionWhen, SegmentClass, SegmentSubtype,
        TransportationCollection,
    },
    graph::{
        island_detection::IslandDetectionAlgorithm, OmfGraphSource, OmfGraphStats, OmfGraphSummary,
        OmfGraphVectorized,
    },
    util,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NetworkEdgeListConfiguration {
    pub mode: String,
    pub filter: Vec<TravelModeFilter>,
    pub island_algorithm_config: Option<IslandDetectionAlgorithm>,
}

impl NetworkEdgeListConfiguration {
    /// Default Overture Maps walk-mode edge list configuration.
    pub fn default_walk() -> Self {
        Self {
            mode: "walk".to_string(),
            filter: vec![
                TravelModeFilter::MatchesSubtype {
                    subtype: SegmentSubtype::Road,
                },
                TravelModeFilter::MatchesClasses {
                    classes: HashSet::from([
                        SegmentClass::Secondary,
                        SegmentClass::Tertiary,
                        SegmentClass::Residential,
                        SegmentClass::LivingStreet,
                        SegmentClass::Unclassified,
                        SegmentClass::Service,
                        SegmentClass::Pedestrian,
                        SegmentClass::Footway,
                        SegmentClass::Steps,
                        SegmentClass::Path,
                        SegmentClass::Track,
                        SegmentClass::Bridleway,
                        SegmentClass::Unknown,
                    ]),
                    behavior: MatchBehavior::Include,
                    allow_unset: true,
                },
                TravelModeFilter::MatchesModeAccess {
                    modes: vec![SegmentMode::Foot],
                },
            ],
            island_algorithm_config: None,
        }
    }

    /// Default Overture Maps bike-mode edge list configuration.
    pub fn default_bike() -> Self {
        Self {
            mode: "bike".to_string(),
            filter: vec![
                TravelModeFilter::MatchesSubtype {
                    subtype: SegmentSubtype::Road,
                },
                TravelModeFilter::MatchesClasses {
                    classes: HashSet::from([
                        SegmentClass::Primary,
                        SegmentClass::Secondary,
                        SegmentClass::Tertiary,
                        SegmentClass::Residential,
                        SegmentClass::LivingStreet,
                        SegmentClass::Unclassified,
                        SegmentClass::Service,
                        SegmentClass::Pedestrian,
                        SegmentClass::Path,
                        SegmentClass::Track,
                        SegmentClass::Cycleway,
                        SegmentClass::Bridleway,
                        SegmentClass::Unknown,
                    ]),
                    behavior: MatchBehavior::Include,
                    allow_unset: true,
                },
                TravelModeFilter::MatchesModeAccess {
                    modes: vec![SegmentMode::Bicycle],
                },
            ],
            island_algorithm_config: None,
        }
    }

    /// Default Overture Maps drive-mode edge list configuration.
    pub fn default_drive() -> Self {
        Self {
            mode: "drive".to_string(),
            filter: vec![
                TravelModeFilter::MatchesSubtype {
                    subtype: SegmentSubtype::Road,
                },
                TravelModeFilter::MatchesClasses {
                    classes: HashSet::from([
                        SegmentClass::Motorway,
                        SegmentClass::Trunk,
                        SegmentClass::Primary,
                        SegmentClass::Secondary,
                        SegmentClass::Tertiary,
                        SegmentClass::Residential,
                        SegmentClass::LivingStreet,
                        SegmentClass::Unclassified,
                        SegmentClass::Service,
                        SegmentClass::Unknown,
                    ]),
                    behavior: MatchBehavior::Include,
                    allow_unset: true,
                },
                TravelModeFilter::MatchesModeAccess {
                    modes: vec![
                        SegmentMode::MotorVehicle,
                        SegmentMode::Car,
                        SegmentMode::Truck,
                        SegmentMode::Motorcycle,
                    ],
                },
            ],
            island_algorithm_config: None,
        }
    }

    /// Built-in preset configuration for `"walk"`, `"bike"`, or `"drive"`.
    pub fn default_for_mode(mode: &str) -> Result<Self, OvertureMapsCollectionError> {
        match mode.trim().to_ascii_lowercase().as_str() {
            "walk" | "foot" | "pedestrian" => Ok(Self::default_walk()),
            "bike" | "bicycle" | "cycling" => Ok(Self::default_bike()),
            "drive" | "car" | "motor_vehicle" | "auto" => Ok(Self::default_drive()),
            other => Err(OvertureMapsCollectionError::InvalidUserInput(format!(
                "unsupported travel mode preset '{other}'; expected one of: walk, bike, drive"
            ))),
        }
    }

    /// Default multimodal (`["walk", "bike", "drive"]`) edge list configurations.
    pub fn default_multimodal() -> Vec<Self> {
        vec![
            Self::default_walk(),
            Self::default_bike(),
            Self::default_drive(),
        ]
    }
}

impl From<&NetworkEdgeListConfiguration> for SegmentAccessRestrictionWhen {
    fn from(value: &NetworkEdgeListConfiguration) -> Self {
        let user_modes_opt = value.filter.iter().find_map(|f| match f {
            TravelModeFilter::MatchesModeAccess { modes } => Some(modes.clone()),
            _ => None,
        });
        let mut result = SegmentAccessRestrictionWhen::default();
        if let Some(modes) = user_modes_opt {
            result.mode = Some(modes);
        }
        result
    }
}

/// runs an OMF network import using the provided configuration.
#[allow(clippy::too_many_arguments)]
pub fn run(
    name: &str,
    bbox: Option<&CliBoundingBox>,
    modes: &[NetworkEdgeListConfiguration],
    output_directory: &Path,
    local_source: Option<&Path>,
    write_json: bool,
    island_detection_configuration: Option<IslandDetectionAlgorithm>,
    export_omf_ids: bool,
    extent: Option<Geometry<f32>>,
    infrastructure_match_distance_meters: Option<f64>,
) -> Result<(), OvertureMapsCollectionError> {
    let collection: TransportationCollection = match local_source {
        Some(src_path) => read_local(src_path),
        None => run_collector(bbox),
    }?;

    util::fs::create_dirs(output_directory)?;
    if write_json {
        collection.to_json(output_directory)?;
    }

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
    let stats = OmfGraphStats::try_from(&vectorized_graph)?;
    let source = OmfGraphSource::new(&release, name, bbox);
    let summary = OmfGraphSummary { source, stats };

    vectorized_graph.write_compass(&summary, output_directory, true, export_omf_ids)?;

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
