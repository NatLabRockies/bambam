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
    model::{ImportConnector, ImportSegment},
    segment_split::SegmentSplit,
    vertex_serializable::EdgeMappingRecord,
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
    pub omf_segment_ids: Vec<(String, f64)>,
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

pub fn create_segment_full_types<S, C>(
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
        .map(|split| split.get_split_segment_full_type(segments, segment_lookup))
        .collect::<Result<Vec<C>, NetworkImportError>>()
}

pub fn create_speed_by_segment_type_lookup<'a, S, C>(
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

pub fn get_segment_omf_ids<S, C>(
    segments: &[&S],
    segment_lookup: &HashMap<String, usize>,
    splits: &[SegmentSplit],
) -> Result<Vec<(String, f64)>, NetworkImportError>
where
    C: Clone + Eq + Hash + Send + Sync,
    S: ImportSegment<C>,
{
    splits
        .par_iter()
        .map(|split| split.get_omf_segment_id_and_linear_ref(segments, segment_lookup))
        .collect()
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
) -> Result<f64, NetworkImportError>
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
        return Err(NetworkImportError::InternalError(format!(
            "internal division by zero when computing average speed: {initial_speeds:?}"
        )));
    }

    Ok(weighted_sum / total_length)
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

pub fn clean_omf_edge_list<C: Send>(
    omf_list: VectorizedEdgeList<C>,
    mask: Vec<bool>,
    vertex_remapping: &[Option<VertexId>],
) -> Result<VectorizedEdgeList<C>, NetworkImportError> {
    let edges = EdgeList(
        omf_list
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

    let geometries = omf_list
        .geometries
        .into_iter()
        .enumerate()
        .filter(|(idx, _)| mask[*idx])
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(_, ls)| ls)
        .collect();

    let classes = omf_list
        .classes
        .into_iter()
        .enumerate()
        .filter(|(idx, _)| mask[*idx])
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(_, cls)| cls)
        .collect();

    let speeds = omf_list
        .speeds
        .into_iter()
        .enumerate()
        .filter(|(idx, _)| mask[*idx])
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(_, s)| s)
        .collect();

    let bearings = omf_list
        .bearings
        .into_iter()
        .enumerate()
        .filter(|(idx, _)| mask[*idx])
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(_, b)| b)
        .collect();

    let omf_segment_ids = omf_list
        .omf_segment_ids
        .into_iter()
        .enumerate()
        .filter(|(idx, _)| mask[*idx])
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(_, i)| i)
        .collect();

    let edge_mappings = omf_list
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
        edge_list_id: omf_list.edge_list_id,
        edges,
        geometries,
        classes,
        speeds,
        speed_lookup: omf_list.speed_lookup,
        bearings,
        omf_segment_ids,
        edge_mappings,
    })
}
