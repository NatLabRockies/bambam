use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

use geo::{Bearing, Haversine, Length, Line, LineString};
use itertools::Itertools;
use rayon::prelude::*;
use routee_compass_core::model::network::{Edge, EdgeId, EdgeList, EdgeListId, Vertex, VertexId};

use crate::{
    consts,
    error::NetworkImportError,
    island_detection::IslandDetectionAlgorithm,
    model::{ImportConnector, ImportInfrastructure, ImportSegment, SegmentHeading},
    segment_split::SegmentSplit,
    taxonomy::{FilterableSegment, NetworkEdgeListConfiguration, SegmentAccessRestrictionWhen},
    vertex_serializable::{
        attach_infrastructure_to_vertices, EdgeMappingRecord, VertexCompleteRecord,
        VertexMappingRecord, DEFAULT_INFRASTRUCTURE_MATCH_DISTANCE_METERS,
        INFRASTRUCTURE_ID_DELIMITER,
    },
};

/// Generic vectorized edge list for a single travel mode.
#[derive(Debug, Clone)]
pub struct VectorizedEdgeList<C> {
    pub edge_list_id: EdgeListId,
    pub edges: EdgeList,
    pub geometries: Vec<LineString<f32>>,
    pub classes: Vec<C>,
    pub speeds: Vec<f64>,
    pub speed_lookup: HashMap<String, f64>,
    pub bearings: Vec<f64>,
    pub edge_mappings: Vec<EdgeMappingRecord>,
}

/// serializes the Connector records into Vertices and creates a source_id -> index mapping.
/// optionally filter to a 'keep list' of Connector ids. the vertex creation is parallelized.
pub fn create_vertices_and_lookup<K: ImportConnector>(
    connectors: &[K],
    keep_list: Option<&HashSet<&String>>,
) -> Result<(Vec<Vertex>, HashMap<String, usize>), NetworkImportError> {
    let keep_connectors = match keep_list {
        Some(keep) => connectors
            .iter()
            .filter(|c| {
                let id_str = c.connector_id().to_string();
                keep.contains(&id_str)
            })
            .collect_vec(),
        None => connectors.iter().collect_vec(),
    };

    let vertices = keep_connectors
        .par_iter()
        .enumerate()
        .map(|(idx, c)| c.try_to_vertex(idx))
        .collect::<Result<Vec<Vertex>, NetworkImportError>>()?;

    let mapping: HashMap<String, usize> = keep_connectors
        .iter()
        .enumerate()
        .map(|(idx, c)| (c.connector_id().to_string(), idx))
        .collect();

    Ok((vertices, mapping))
}

/// builds a lookup function from segment id to segment index
pub fn create_segment_lookup<S, C>(segments: &[&S]) -> HashMap<String, usize>
where
    C: Clone + Eq + Hash + Send + Sync,
    S: ImportSegment<C>,
{
    segments
        .iter()
        .enumerate()
        .map(|(idx, c)| (c.segment_id().to_string(), idx))
        .collect()
}

/// collects all splits from all segment records, used to create edges.
pub fn find_splits<S, W, E>(
    segments: &[&S],
    when: Option<&W>,
    split_op: fn(&S, Option<&W>) -> Result<Vec<SegmentSplit>, E>,
) -> Result<Vec<SegmentSplit>, E>
where
    S: Sync,
    W: Sync,
    E: Send,
{
    let result = segments
        .par_iter()
        .map(|s| split_op(s, when))
        .collect::<Result<Vec<Vec<SegmentSplit>>, E>>()?
        .into_iter()
        .flatten()
        .collect();
    Ok(result)
}

/// creates all edges along the provided set of splits.
pub fn create_edges<S, C>(
    segments: &[&S],
    segment_lookup: &HashMap<String, usize>,
    splits: &[SegmentSplit],
    vertices: &[Vertex],
    vertex_lookup: &HashMap<String, usize>,
    edge_list_id: EdgeListId,
) -> Result<Vec<Edge>, NetworkImportError>
where
    C: Clone + Eq + Hash + Send + Sync,
    S: ImportSegment<C>,
{
    splits
        .iter()
        .enumerate()
        .collect_vec()
        .par_iter()
        .map(|(idx, split)| {
            split.create_edge_from_split(
                EdgeId(*idx),
                edge_list_id,
                segments,
                segment_lookup,
                vertices,
                vertex_lookup,
            )
        })
        .collect::<Result<Vec<Edge>, NetworkImportError>>()
}

pub fn create_geometries<S, C>(
    segments: &[&S],
    segment_lookup: &HashMap<String, usize>,
    splits: &[SegmentSplit],
) -> Result<Vec<LineString<f32>>, NetworkImportError>
where
    C: Clone + Eq + Hash + Send + Sync,
    S: ImportSegment<C>,
{
    splits
        .par_iter()
        .map(|split| split.create_geometry_from_split(segments, segment_lookup))
        .collect::<Result<Vec<LineString<f32>>, NetworkImportError>>()
}

pub fn create_speeds<S, C>(
    segments: &[&S],
    segment_lookup: &HashMap<String, usize>,
    splits: &[SegmentSplit],
) -> Result<Vec<Option<f64>>, NetworkImportError>
where
    C: Clone + Eq + Hash + Send + Sync,
    S: ImportSegment<C>,
{
    splits
        .par_iter()
        .map(|split| split.get_split_speed(segments, segment_lookup))
        .collect::<Result<Vec<Option<f64>>, NetworkImportError>>()
}

pub fn create_classes<S, C>(
    segments: &[&S],
    segment_lookup: &HashMap<String, usize>,
    splits: &[SegmentSplit],
) -> Result<Vec<C>, NetworkImportError>
where
    C: Clone + Eq + Hash + Send + Sync,
    S: ImportSegment<C>,
{
    splits
        .par_iter()
        .map(|split| split.get_split_class(segments, segment_lookup))
        .collect::<Result<Vec<C>, NetworkImportError>>()
}

pub fn create_speed_by_class_lookup<'a, S, C>(
    initial_speeds: &[Option<f64>],
    segments: &[&S],
    segment_lookup: &HashMap<String, usize>,
    splits: &[SegmentSplit],
    classes: &'a [C],
) -> Result<HashMap<&'a C, f64>, NetworkImportError>
where
    C: Clone + Eq + Hash + Send + Sync,
    S: ImportSegment<C>,
{
    let split_lengths = splits
        .par_iter()
        .map(|split| {
            split
                .get_split_length_meters(segments, segment_lookup)
                .map(|v_f32| v_f32 as f64)
        })
        .collect::<Result<Vec<f64>, NetworkImportError>>()?;

    let mut speed_sum_lookup: HashMap<&C, (f64, f64)> = HashMap::new();

    for ((class, w), speed) in classes.iter().zip(split_lengths).zip(initial_speeds) {
        let Some(x) = speed else { continue };
        let element = speed_sum_lookup.entry(class).or_insert((0.0, 0.0));
        element.0 += w * x;
        element.1 += w;
    }

    Ok(speed_sum_lookup
        .into_iter()
        .filter(|&(_k, (_wx, w))| w != 0.0)
        .map(|(k, (wx, w))| (k, wx / w))
        .collect::<HashMap<&C, f64>>())
}

pub fn get_edge_mappings<S, C>(
    segments: &[&S],
    segment_lookup: &HashMap<String, usize>,
    splits: &[SegmentSplit],
) -> Result<Vec<EdgeMappingRecord>, NetworkImportError>
where
    C: Clone + Eq + Hash + Send + Sync,
    S: ImportSegment<C>,
{
    splits
        .iter()
        .enumerate()
        .collect_vec()
        .par_iter()
        .map(|(idx, split)| split.get_edge_mapping(*idx, segments, segment_lookup))
        .collect()
}

pub fn get_global_average_speed<S, C>(
    initial_speeds: &[Option<f64>],
    segments: &[&S],
    segment_lookup: &HashMap<String, usize>,
    splits: &[SegmentSplit],
) -> Result<Option<f64>, NetworkImportError>
where
    C: Clone + Eq + Hash + Send + Sync,
    S: ImportSegment<C>,
{
    let split_lengths = splits
        .par_iter()
        .map(|split| {
            split
                .get_split_length_meters(segments, segment_lookup)
                .map(|v_f32| v_f32 as f64)
        })
        .collect::<Result<Vec<f64>, NetworkImportError>>()?;

    let mut total_length = 0.;
    let mut weighted_sum = 0.;
    for (opt_speed, length) in initial_speeds.iter().zip(split_lengths) {
        let Some(speed) = opt_speed else { continue };
        total_length += length;
        weighted_sum += length * speed;
    }

    if total_length < consts::F64_DISTANCE_TOLERANCE {
        return Ok(None);
    }

    Ok(Some(weighted_sum / total_length))
}

/// Computes the outward bearings of the geometries representing
/// segment splits using the last two points in the LineStrings.
pub fn bearing_deg_from_geometries(
    geometries: &[LineString<f32>],
) -> Result<Vec<f64>, NetworkImportError> {
    geometries
        .par_iter()
        .map(|linestring| {
            let n = linestring.0.len();
            if n < 2 {
                return Err(NetworkImportError::InternalError(format!(
                    "cannot compute bearing on linestring with less than two points: {linestring:?}"
                )));
            }

            let p1 = linestring.0[n - 1];
            let mut p0 = linestring.0[n - 2];

            for i in (0..n - 1).rev() {
                let candidate = linestring.0[i];
                let line = Line::new(candidate, p1);

                if Haversine.length(&line) > consts::F32_DISTANCE_TOLERANCE {
                    p0 = candidate;
                    break;
                }
            }

            Ok(Haversine.bearing(p0.into(), p1.into()) as f64)
        })
        .collect()
}

pub fn compute_vertex_remapping<C>(
    vertices: &[Vertex],
    edge_lists: &[VectorizedEdgeList<C>],
    island_edges: &[(EdgeListId, EdgeId)],
) -> Result<Vec<Option<VertexId>>, NetworkImportError> {
    let removed_edges: HashSet<(EdgeListId, EdgeId)> = island_edges.iter().cloned().collect();

    let mut valid_vertices = HashSet::new();

    for edge_list in edge_lists {
        for edge in edge_list.edges.0.iter() {
            if !removed_edges.contains(&(edge.edge_list_id, edge.edge_id)) {
                valid_vertices.insert(edge.src_vertex_id);
                valid_vertices.insert(edge.dst_vertex_id);
            }
        }
    }

    let mut new_id: usize = 0;
    Ok((0..vertices.len())
        .map(|old_id| {
            if valid_vertices.contains(&VertexId(old_id)) {
                new_id += 1;
                Some(VertexId(new_id - 1))
            } else {
                None
            }
        })
        .collect())
}

pub fn clean_edge_list<C: Send>(
    edge_list: VectorizedEdgeList<C>,
    mask: Vec<bool>,
    vertex_remapping: &[Option<VertexId>],
) -> Result<VectorizedEdgeList<C>, NetworkImportError> {
    let edges = EdgeList(
        edge_list
            .edges
            .0
            .iter()
            .filter(|edge| mask[edge.edge_id.0])
            .enumerate()
            .collect::<Vec<_>>()
            .into_par_iter()
            .map(|(new_idx, edge)| {
                let new_src_id = vertex_remapping
                    .get(edge.src_vertex_id.0)
                    .copied()
                    .flatten()
                    .ok_or_else(|| {
                        NetworkImportError::InternalError(format!(
                            "src vertex_id {} for edge ({},{}) was removed",
                            edge.src_vertex_id, edge.edge_list_id, edge.edge_id
                        ))
                    })?;
                let new_dst_id = vertex_remapping
                    .get(edge.dst_vertex_id.0)
                    .copied()
                    .flatten()
                    .ok_or_else(|| {
                        NetworkImportError::InternalError(format!(
                            "dst vertex_id {} for edge ({},{}) was removed",
                            edge.dst_vertex_id, edge.edge_list_id, edge.edge_id
                        ))
                    })?;

                Ok(Edge {
                    edge_id: EdgeId(new_idx),
                    src_vertex_id: new_src_id,
                    dst_vertex_id: new_dst_id,
                    ..*edge
                })
            })
            .collect::<Result<Vec<Edge>, NetworkImportError>>()?
            .into_boxed_slice(),
    );

    let geometries = edge_list
        .geometries
        .into_iter()
        .enumerate()
        .filter(|(idx, _)| mask[*idx])
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(_, ls)| ls)
        .collect();

    let classes = edge_list
        .classes
        .into_iter()
        .enumerate()
        .filter(|(idx, _)| mask[*idx])
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(_, cls)| cls)
        .collect();

    let speeds = edge_list
        .speeds
        .into_iter()
        .enumerate()
        .filter(|(idx, _)| mask[*idx])
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(_, s)| s)
        .collect();

    let bearings = edge_list
        .bearings
        .into_iter()
        .enumerate()
        .filter(|(idx, _)| mask[*idx])
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(_, b)| b)
        .collect();

    let edge_mappings = edge_list
        .edge_mappings
        .into_iter()
        .enumerate()
        .filter(|(idx, _)| mask[*idx])
        .enumerate()
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(new_idx, (_, mut m))| {
            m.edge_id = new_idx;
            m
        })
        .collect();

    Ok(VectorizedEdgeList {
        edge_list_id: edge_list.edge_list_id,
        edges,
        geometries,
        classes,
        speeds,
        speed_lookup: edge_list.speed_lookup,
        bearings,
        edge_mappings,
    })
}

/// Default fallback speed (mph) used in RouteE Compass speed tables when no speed limits
/// are observed in a mode's edge list.
pub const DEFAULT_FALLBACK_SPEED_MPH: f64 = 25.0;

/// Output of the shared multi-mode vectorized graph construction pipeline.
#[derive(Debug, Clone)]
pub struct BuiltNetworkGraph<C> {
    pub vertices: Vec<Vertex>,
    pub vertices_complete: Vec<VertexCompleteRecord>,
    pub vertex_mappings: Vec<VertexMappingRecord>,
    pub edge_lists: Vec<VectorizedEdgeList<C>>,
    pub edge_list_config: Vec<NetworkEdgeListConfiguration>,
    pub vertex_lookup: HashMap<String, usize>,
}

/// Shared multi-mode vectorized graph builder used by both `bambam-omf` and `bambam-osm`:
/// 1. Converts all `connectors` into initial `Vertex` records and builds `vertex_lookup`.
/// 2. For each mode in `configuration`, filters `segments`, finds directed `SegmentSplit`s,
///    and constructs a `VectorizedEdgeList<C>`.
/// 3. Runs `island_detection_configuration` (and any per-mode `island_algorithm_config`)
///    across all mode edge lists, remaps surviving vertices, and cleans each edge list.
/// 4. Attaches `infrastructure` within `infrastructure_match_distance_meters` via `rstar`
///    and merges any intrinsic connector control flags.
#[allow(clippy::too_many_arguments)]
pub fn build_vectorized_network_graph<K, S, I, C, E>(
    connectors: &[K],
    segments: &[S],
    infrastructure: &[I],
    configuration: &[NetworkEdgeListConfiguration],
    island_detection_configuration: Option<&IslandDetectionAlgorithm>,
    infrastructure_match_distance_meters: Option<f64>,
    split_op: fn(&S, Option<&SegmentAccessRestrictionWhen>) -> Result<Vec<SegmentSplit>, E>,
    map_edge_mapping: Option<fn(&S, usize, &SegmentSplit) -> EdgeMappingRecord>,
) -> Result<BuiltNetworkGraph<C>, E>
where
    K: ImportConnector,
    S: ImportSegment<C> + FilterableSegment,
    I: ImportInfrastructure,
    C: Clone + Eq + Hash + Send + Sync,
    E: From<NetworkImportError> + Send,
{
    log::info!("Creating vertex lookup");
    let (mut vertices, mut vertex_lookup) = create_vertices_and_lookup(connectors, None)?;

    log::info!("Processing edge lists");
    let mut edge_lists: Vec<VectorizedEdgeList<C>> = Vec::with_capacity(configuration.len());
    for (index, edge_list_config) in configuration.iter().enumerate() {
        let edge_list_id = EdgeListId(index);

        let mut filter = edge_list_config.filter.clone();
        filter.sort();

        log::info!(
            "Filtering edge list {edge_list_id} (mode '{}')",
            edge_list_config.mode
        );
        let mode_segments: Vec<&S> = segments
            .par_iter()
            .filter(|r| filter.iter().all(|f| f.matches_filter(*r)))
            .collect();

        if mode_segments.is_empty() {
            edge_lists.push(VectorizedEdgeList {
                edge_list_id,
                edges: EdgeList(Box::new([])),
                geometries: Vec::new(),
                classes: Vec::new(),
                speeds: Vec::new(),
                speed_lookup: HashMap::new(),
                bearings: Vec::new(),
                edge_mappings: Vec::new(),
            });
            continue;
        }

        let segment_lookup = create_segment_lookup::<S, C>(&mode_segments);

        log::info!("Creating splits");
        let mut splits = Vec::new();
        for heading in [SegmentHeading::Forward, SegmentHeading::Backward] {
            let mut when: SegmentAccessRestrictionWhen = edge_list_config.into();
            when.heading = Some(heading);

            let directed_splits = find_splits(&mode_segments, Some(&when), split_op)?;
            splits.extend(directed_splits);
        }

        log::info!("Creating edges");
        let edges = create_edges::<S, C>(
            &mode_segments,
            &segment_lookup,
            &splits,
            &vertices,
            &vertex_lookup,
            edge_list_id,
        )?;
        log::info!("Creating geometries");
        let geometries = create_geometries::<S, C>(&mode_segments, &segment_lookup, &splits)?;
        log::info!("Creating bearings");
        let bearings = bearing_deg_from_geometries(&geometries)?;
        log::info!("Creating classes");
        let classes = create_classes::<S, C>(&mode_segments, &segment_lookup, &splits)?;

        log::info!("Creating speeds");
        let initial_speeds = create_speeds::<S, C>(&mode_segments, &segment_lookup, &splits)?;
        log::info!("Creating speed lookup");
        let speed_lookup_by_cls = create_speed_by_class_lookup::<S, C>(
            &initial_speeds,
            &mode_segments,
            &segment_lookup,
            &splits,
            &classes,
        )?;

        log::info!("Computing global speed");
        let global_speed_opt =
            get_global_average_speed::<S, C>(&initial_speeds, &mode_segments, &segment_lookup, &splits)?;
        let fallback_speed = global_speed_opt.unwrap_or(DEFAULT_FALLBACK_SPEED_MPH);

        log::info!("Computing edge mappings");
        let edge_mappings = match map_edge_mapping {
            Some(mapper) => splits
                .iter()
                .enumerate()
                .map(|(edge_idx, split)| {
                    let seg_idx = segment_lookup[&split.src.segment_id];
                    let seg = mode_segments[seg_idx];
                    mapper(seg, edge_idx, split)
                })
                .collect(),
            None => get_edge_mappings::<S, C>(&mode_segments, &segment_lookup, &splits)?,
        };

        log::info!("Completing speeds vector with default global");
        let speeds = initial_speeds
            .into_par_iter()
            .zip(&classes)
            .map(|(opt_speed, class)| {
                opt_speed
                    .or_else(|| speed_lookup_by_cls.get(class).copied())
                    .unwrap_or(fallback_speed)
            })
            .collect::<Vec<f64>>();

        let mut speed_lookup: HashMap<String, f64> = speed_lookup_by_cls
            .into_iter()
            .map(|(k, v)| (S::class_to_string(k), v))
            .collect();
        if let Some(global_speed) = global_speed_opt {
            speed_lookup.insert(String::from(consts::GLOBAL_AVG_SPEED_KEY), global_speed);
        }

        let edge_list = VectorizedEdgeList {
            edge_list_id,
            edges: EdgeList(edges.into_boxed_slice()),
            geometries,
            classes,
            speeds,
            speed_lookup,
            bearings,
            edge_mappings,
        };
        edge_lists.push(edge_list);
    }

    // Compute islands in resulting edge lists and remove island edges
    log::info!("Compute islands");
    let mut island_edges = Vec::new();
    if let Some(island_detection) = island_detection_configuration {
        let ref_edge_lists = edge_lists
            .iter()
            .map(|e| &e.edges)
            .collect::<Vec<&EdgeList>>();
        island_edges.extend(island_detection.run(&ref_edge_lists, &vertices)?);
    } else {
        for (el, cfg) in edge_lists.iter().zip(configuration.iter()) {
            if let Some(ref mode_island_alg) = cfg.island_algorithm_config {
                island_edges.extend(mode_island_alg.run(&[&el.edges], &vertices)?);
            }
        }
    }

    if !island_edges.is_empty() {
        let mut edges_lookup: HashMap<EdgeListId, HashSet<EdgeId>> = HashMap::new();
        for (a, b) in &island_edges {
            edges_lookup.entry(*a).or_default().insert(*b);
        }

        let vertex_remapping = compute_vertex_remapping(&vertices, &edge_lists, &island_edges)?;
        vertices = vertices
            .into_iter()
            .filter_map(|vertex| {
                vertex_remapping[vertex.vertex_id.0].map(|vertex_id| Vertex {
                    vertex_id,
                    ..vertex
                })
            })
            .collect();

        vertex_lookup.retain(|_, v| vertex_remapping[*v].is_some());
        for v in vertex_lookup.values_mut() {
            *v = vertex_remapping[*v]
                .ok_or_else(|| {
                    NetworkImportError::InternalError(format!(
                        "vertex index {v} expected after island computation but was flagged for deletion"
                    ))
                })?
                .0;
        }

        log::info!("Apply islands algorithm result");
        edge_lists = edge_lists
            .into_iter()
            .map(|el| {
                let to_remove = edges_lookup.get(&el.edge_list_id);
                let mask = el
                    .edges
                    .0
                    .iter()
                    .map(|edge| !to_remove.is_some_and(|set| set.contains(&edge.edge_id)))
                    .collect::<Vec<bool>>();

                clean_edge_list(el, mask, &vertex_remapping).map_err(E::from)
            })
            .collect::<Result<Vec<VectorizedEdgeList<C>>, E>>()?;
    }

    // Order connector IDs by final vertex_id
    let connector_ids: Vec<String> = vertex_lookup
        .iter()
        .sorted_by_key(|(_, v)| **v)
        .map(|(k, _)| k.clone())
        .collect();

    let vertex_mappings: Vec<VertexMappingRecord> = connector_ids
        .iter()
        .enumerate()
        .map(|(vertex_id, source_id)| VertexMappingRecord {
            vertex_id,
            source_id: source_id.clone(),
        })
        .collect();

    let match_distance = infrastructure_match_distance_meters
        .unwrap_or(DEFAULT_INFRASTRUCTURE_MATCH_DISTANCE_METERS);
    log::info!(
        "Attaching {} infrastructure records to {} vertices (tolerance: {match_distance}m)",
        infrastructure.len(),
        vertices.len()
    );
    let mut vertices_complete = attach_infrastructure_to_vertices(
        &vertices,
        &connector_ids,
        infrastructure,
        match_distance,
    );

    // Also reflect any intrinsic connector control flags (e.g., direct or propagated OSM node tags)
    let connector_by_id: HashMap<&str, &K> =
        connectors.iter().map(|c| (c.connector_id(), c)).collect();
    for vc in &mut vertices_complete {
        if let Some(conn) = connector_by_id.get(vc.connector_id.as_str()) {
            if conn.has_traffic_signals() {
                vc.traffic_signals = true;
            }
            if conn.has_stop() {
                vc.stop = true;
            }
            if conn.has_give_way() {
                vc.give_way = true;
            }
            if conn.has_crossing() {
                vc.crossing = true;
            }
            if let Some(extra_ids) = conn.consolidated_infrastructure_ids() {
                let mut ids: Vec<String> = vc
                    .infrastructure_ids
                    .as_deref()
                    .map(|s| {
                        s.split(INFRASTRUCTURE_ID_DELIMITER)
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default();
                for eid in extra_ids {
                    if !ids.contains(&eid) {
                        ids.push(eid);
                    }
                }
                if !ids.is_empty() {
                    vc.infrastructure_ids = Some(ids.join(INFRASTRUCTURE_ID_DELIMITER));
                }
            }
        }
    }

    Ok(BuiltNetworkGraph {
        vertices,
        vertices_complete,
        vertex_mappings,
        edge_lists,
        edge_list_config: configuration.to_vec(),
        vertex_lookup,
    })
}
