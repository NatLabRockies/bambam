use std::collections::{HashMap, HashSet, VecDeque};

use super::{
    OsmNodeDataSerializable, OsmNodeId, OsmNodesSerializable, OsmWayDataSerializable,
    OsmWaysSerializable, Vertex, VertexLookup,
};
use crate::{
    algorithm::truncation::ComponentFilter,
    model::{
        feature::highway::Highway,
        osm::{
            graph::{
                osm_element_filter::ElementFilter, osm_node_data::OsmConnectorRecord, OsmNodeData,
                OsmWays,
            },
            import_ops::{
                extract_osm_connectors_and_segments, process_osm_segment_splits, OsmCollection,
                OsmSegmentRecord,
            },
            OsmError,
        },
    },
};
use bambam_network::{
    attach_infrastructure_to_vertices,
    serialize_ops::{self as net_ops, clean_omf_edge_list, compute_vertex_remapping},
    EdgeMappingRecord, IslandDetectionAlgorithm, OmfVertexComplete, SegmentHeading,
    VectorizedEdgeList, VertexMappingRecord, DEFAULT_INFRASTRUCTURE_MATCH_DISTANCE_METERS,
    GLOBAL_AVG_SPEED_KEY,
};
use geo::Geometry;
use rayon::prelude::*;
use routee_compass_core::model::network::{EdgeId, EdgeList, EdgeListId, VertexId};

pub struct OsmGraphVectorized {
    /// the collection of OSM nodes associated via their OSMID
    pub nodes: OsmNodesSerializable,
    /// list of OSM ways in vectorized EdgeId order
    pub ways: OsmWaysSerializable,
    /// for each OsmNodeId, the vertex index and Vertex
    pub vertex_lookup: VertexLookup,
    /// enriched vertices with traffic_signals, stop, give_way, crossing
    pub vertices_complete: Vec<OmfVertexComplete>,
    /// standardized vertex ID to OSM node ID mapping
    pub vertex_mappings: Vec<VertexMappingRecord>,
    /// standardized edge ID to OSM way ID + linear reference mapping
    pub edge_mappings: Vec<EdgeMappingRecord>,
    /// primary vectorized edge list from bambam-network
    pub edge_list: Option<VectorizedEdgeList<Highway>>,
    /// optional per-mode vectorized edge lists (e.g., `walk`, `bike`, `drive`)
    pub mode_edge_lists: Vec<(String, VectorizedEdgeList<Highway>)>,
    /// raw OSM nodes and ways collection for Parquet retention
    pub raw_collection: Option<OsmCollection>,
}

impl OsmGraphVectorized {
    /// Builds an [`OsmGraphVectorized`] using the OMF-style Connector + Segment + Split
    /// workflow from `bambam-network`.
    #[allow(clippy::too_many_arguments)]
    pub fn from_osm_maps(
        source_label: impl Into<String>,
        nodes_map: HashMap<OsmNodeId, OsmNodeData>,
        ways_map: OsmWays,
        extent_opt: Option<&Geometry<f32>>,
        component_filter: &ComponentFilter,
        island_detection_opt: Option<&IslandDetectionAlgorithm>,
        truncate_by_edge: bool,
        simplify: bool,
        ignore_errors: bool,
        infrastructure_match_distance_meters: Option<f64>,
    ) -> Result<OsmGraphVectorized, OsmError> {
        Self::from_osm_maps_with_modes(
            source_label,
            nodes_map,
            ways_map,
            extent_opt,
            component_filter,
            island_detection_opt,
            truncate_by_edge,
            simplify,
            ignore_errors,
            infrastructure_match_distance_meters,
            None,
        )
    }

    /// Builds an [`OsmGraphVectorized`] with optional mode-specific edge lists
    /// (e.g. `["walk", "bike", "drive"]`), sharing the exact same vertex space and
    /// output layout as `bambam-omf`.
    #[allow(clippy::too_many_arguments)]
    pub fn from_osm_maps_with_modes(
        source_label: impl Into<String>,
        nodes_map: HashMap<OsmNodeId, OsmNodeData>,
        ways_map: OsmWays,
        extent_opt: Option<&Geometry<f32>>,
        component_filter: &ComponentFilter,
        island_detection_opt: Option<&IslandDetectionAlgorithm>,
        truncate_by_edge: bool,
        simplify: bool,
        ignore_errors: bool,
        infrastructure_match_distance_meters: Option<f64>,
        modes: Option<&[String]>,
    ) -> Result<OsmGraphVectorized, OsmError> {
        let raw_collection = OsmCollection::from_maps(source_label, &nodes_map, &ways_map);

        let (connectors, infrastructure, segments) = extract_osm_connectors_and_segments(
            &nodes_map,
            &ways_map,
            extent_opt,
            truncate_by_edge,
            simplify,
            ignore_errors,
        )?;

        if connectors.is_empty() {
            return Err(OsmError::NoNodesFound);
        }
        if segments.is_empty() {
            return Err(OsmError::NoWaysFound);
        }

        let (mut vertices, vertex_lookup_str) =
            net_ops::create_vertices_and_lookup(&connectors, None)?;

        let all_segment_refs: Vec<&OsmSegmentRecord> = segments.iter().collect();
        let (mut vec_edge_list, mut split_way_meta) = build_osm_edge_list_from_segments(
            &all_segment_refs,
            &vertices,
            &vertex_lookup_str,
            EdgeListId(0),
        )?;

        // Determine edges to remove from ComponentFilter and/or IslandDetectionAlgorithm
        let mut island_edges: Vec<(EdgeListId, EdgeId)> = Vec::new();
        island_edges.extend(find_component_filter_removed_edges(
            &vec_edge_list.edges,
            vertices.len(),
            component_filter,
        ));
        if let Some(island_alg) = island_detection_opt {
            let alg_removed = island_alg.run(&[&vec_edge_list.edges], &vertices)?;
            island_edges.extend(alg_removed);
        }

        let connector_by_old_vertex: Vec<&OsmConnectorRecord> = connectors.iter().collect();
        let kept_connectors: Vec<OsmConnectorRecord>;

        if !island_edges.is_empty() {
            let edges_to_remove: HashSet<EdgeId> =
                island_edges.iter().map(|(_, eid)| *eid).collect();
            let mask: Vec<bool> = vec_edge_list
                .edges
                .0
                .iter()
                .map(|e| !edges_to_remove.contains(&e.edge_id))
                .collect();

            let edge_lists_slice = [vec_edge_list];
            let vertex_remapping =
                compute_vertex_remapping(&vertices, &edge_lists_slice, &island_edges)?;
            let [single_list] = edge_lists_slice;

            vertices = vertices
                .into_iter()
                .filter_map(|v| {
                    vertex_remapping[v.vertex_id.0].map(|new_vid| Vertex {
                        vertex_id: new_vid,
                        ..v
                    })
                })
                .collect();

            kept_connectors = connector_by_old_vertex
                .into_iter()
                .enumerate()
                .filter_map(|(old_vid, conn)| vertex_remapping[old_vid].map(|_| conn.clone()))
                .collect();

            split_way_meta = split_way_meta
                .into_iter()
                .enumerate()
                .filter(|(idx, _)| mask[*idx])
                .map(|(_, item)| item)
                .collect();

            vec_edge_list = clean_omf_edge_list(single_list, mask, &vertex_remapping)?;
        } else {
            kept_connectors = connectors;
        }

        // Build final node/vertex tables and lookup
        let mut nodes: OsmNodesSerializable = Vec::with_capacity(kept_connectors.len());
        let mut vertex_lookup: VertexLookup = HashMap::with_capacity(kept_connectors.len());
        let mut final_vertex_lookup_str: HashMap<String, usize> =
            HashMap::with_capacity(kept_connectors.len());
        let mut connector_ids: Vec<String> = Vec::with_capacity(kept_connectors.len());
        let mut vertex_mappings: Vec<VertexMappingRecord> = Vec::with_capacity(kept_connectors.len());

        for (vertex_id, (conn, vertex)) in kept_connectors.iter().zip(vertices.iter()).enumerate() {
            nodes.push(OsmNodeDataSerializable::from(&conn.node));
            vertex_lookup.insert(conn.node.osmid, (vertex_id, *vertex));
            final_vertex_lookup_str.insert(conn.id_str.clone(), vertex_id);
            connector_ids.push(conn.id_str.clone());
            vertex_mappings.push(VertexMappingRecord {
                vertex_id,
                source_id: conn.id_str.clone(),
            });
        }

        let match_dist = infrastructure_match_distance_meters
            .unwrap_or(DEFAULT_INFRASTRUCTURE_MATCH_DISTANCE_METERS);
        let mut vertices_complete = attach_infrastructure_to_vertices(
            &vertices,
            &connector_ids,
            &infrastructure,
            match_dist,
        );
        // Ensure direct OSM node tags on the connector vertex itself are also reflected
        for (vc, conn) in vertices_complete.iter_mut().zip(kept_connectors.iter()) {
            if conn.node.has_traffic_signals() {
                vc.traffic_signals = true;
            }
            if conn.node.has_stop() {
                vc.stop = true;
            }
            if conn.node.has_give_way() {
                vc.give_way = true;
            }
            if conn.node.has_crossing() {
                vc.crossing = true;
            }
        }

        // Build OsmWayDataSerializable rows matching vec_edge_list
        let mut ways: OsmWaysSerializable = Vec::with_capacity(vec_edge_list.edges.len());
        for ((edge, geom), (seg, heading)) in vec_edge_list
            .edges
            .0
            .iter()
            .zip(vec_edge_list.geometries.iter())
            .zip(split_way_meta.iter())
        {
            let mut node_seq = seg.node_sequence.clone();
            if *heading == SegmentHeading::Backward {
                node_seq.reverse();
            }
            let length_meters = edge.distance.get::<uom::si::length::meter>();
            let way_ser = OsmWayDataSerializable::from_split_way(
                &seg.way,
                &node_seq,
                edge.src_vertex_id,
                edge.dst_vertex_id,
                geom.clone(),
                length_meters,
            )?;
            ways.push(way_ser);
        }

        let edge_mappings = vec_edge_list.edge_mappings.clone();

        // Build optional per-mode edge lists sharing the final vertex space
        let mut mode_edge_lists = Vec::new();
        if let Some(mode_list) = modes.filter(|m| !m.is_empty()) {
            for (mode_idx, mode_name) in mode_list.iter().enumerate() {
                let mode_filter = ElementFilter::default_for_modes(Some(std::slice::from_ref(mode_name)));
                let allowed_tags: Option<HashSet<Highway>> = match mode_filter {
                    ElementFilter::HighwayTags { tags } => Some(tags),
                    _ => None,
                };
                let mode_segments: Vec<&OsmSegmentRecord> = segments
                    .iter()
                    .filter(|s| {
                        final_vertex_lookup_str.contains_key(&s.src_node_id.to_string())
                            && final_vertex_lookup_str.contains_key(&s.dst_node_id.to_string())
                            && allowed_tags
                                .as_ref()
                                .is_none_or(|tags| tags.contains(&s.highway))
                    })
                    .collect();

                if !mode_segments.is_empty() {
                    let (mode_vec_list, _) = build_osm_edge_list_from_segments(
                        &mode_segments,
                        &vertices,
                        &final_vertex_lookup_str,
                        EdgeListId(mode_idx),
                    )?;
                    mode_edge_lists.push((mode_name.clone(), mode_vec_list));
                }
            }
        }

        Ok(OsmGraphVectorized {
            nodes,
            ways,
            vertex_lookup,
            vertices_complete,
            vertex_mappings,
            edge_mappings,
            edge_list: Some(vec_edge_list),
            mode_edge_lists,
            raw_collection: Some(raw_collection),
        })
    }
}

type BuiltOsmEdgeList<'a> = (
    VectorizedEdgeList<Highway>,
    Vec<(&'a OsmSegmentRecord, SegmentHeading)>,
);

fn build_osm_edge_list_from_segments<'a>(
    segment_refs: &[&'a OsmSegmentRecord],
    vertices: &[Vertex],
    vertex_lookup_str: &HashMap<String, usize>,
    edge_list_id: EdgeListId,
) -> Result<BuiltOsmEdgeList<'a>, OsmError> {
    let segment_lookup = net_ops::create_segment_lookup::<OsmSegmentRecord, Highway>(segment_refs);

    let mut splits = Vec::new();
    for heading in [SegmentHeading::Forward, SegmentHeading::Backward] {
        let directed_splits =
            net_ops::find_splits(segment_refs, Some(&heading), process_osm_segment_splits)?;
        splits.extend(directed_splits);
    }

    let edges = net_ops::create_edges::<OsmSegmentRecord, Highway>(
        segment_refs,
        &segment_lookup,
        &splits,
        vertices,
        vertex_lookup_str,
        edge_list_id,
    )?;
    let geometries = net_ops::create_geometries::<OsmSegmentRecord, Highway>(
        segment_refs,
        &segment_lookup,
        &splits,
    )?;
    let bearings = net_ops::bearing_deg_from_geometries(&geometries)?;
    let classes = net_ops::create_segment_full_types::<OsmSegmentRecord, Highway>(
        segment_refs,
        &segment_lookup,
        &splits,
    )?;
    let initial_speeds = net_ops::create_speeds::<OsmSegmentRecord, Highway>(
        segment_refs,
        &segment_lookup,
        &splits,
    )?;
    let speed_lookup_by_hw = net_ops::create_speed_by_segment_type_lookup::<OsmSegmentRecord, Highway>(
        &initial_speeds,
        segment_refs,
        &segment_lookup,
        &splits,
        &classes,
    )?;

    let global_speed = net_ops::get_global_average_speed::<OsmSegmentRecord, Highway>(
        &initial_speeds,
        segment_refs,
        &segment_lookup,
        &splits,
    )
    .unwrap_or(25.0);

    let speeds: Vec<f64> = initial_speeds
        .into_par_iter()
        .zip(&classes)
        .map(|(opt_speed, hw)| {
            opt_speed
                .or_else(|| speed_lookup_by_hw.get(hw).copied())
                .unwrap_or(global_speed)
        })
        .collect();

    let mut speed_lookup: HashMap<String, f64> = speed_lookup_by_hw
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    speed_lookup.insert(GLOBAL_AVG_SPEED_KEY.to_string(), global_speed);

    let mut edge_mappings: Vec<EdgeMappingRecord> = Vec::with_capacity(splits.len());
    let mut split_way_meta: Vec<(&'a OsmSegmentRecord, SegmentHeading)> =
        Vec::with_capacity(splits.len());

    for (edge_idx, split) in splits.iter().enumerate() {
        let seg_idx = segment_lookup[&split.src.segment_id];
        let seg = segment_refs[seg_idx];
        let way_id_str = seg.parent_way_id.to_string();
        edge_mappings.push(EdgeMappingRecord {
            edge_id: edge_idx,
            source_id: way_id_str,
            start_lr: seg.parent_start_lr,
            end_lr: seg.parent_end_lr,
            heading: split.heading.clone(),
        });
        split_way_meta.push((seg, split.heading.clone()));
    }

    let vec_edge_list = VectorizedEdgeList {
        edge_list_id,
        edges: EdgeList(edges.into_boxed_slice()),
        geometries,
        classes,
        speeds,
        speed_lookup,
        bearings,
        edge_mappings,
    };

    Ok((vec_edge_list, split_way_meta))
}

/// Evaluates `ComponentFilter` (`Largest`, `TopK`, `LeastK`, `KeepAll`) over the vectorized
/// `EdgeList` using undirected BFS, returning the `(EdgeListId, EdgeId)` pairs to prune.
fn find_component_filter_removed_edges(
    edge_list: &EdgeList,
    n_vertices: usize,
    component_filter: &ComponentFilter,
) -> Vec<(EdgeListId, EdgeId)> {
    if matches!(component_filter, ComponentFilter::KeepAll) || edge_list.is_empty() {
        return Vec::new();
    }

    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n_vertices];
    let mut vertex_edges: Vec<Vec<(EdgeListId, EdgeId)>> = vec![Vec::new(); n_vertices];
    for edge in edge_list.edges() {
        let u = edge.src_vertex_id.0;
        let v = edge.dst_vertex_id.0;
        if u < n_vertices && v < n_vertices {
            adj[u].push(v);
            adj[v].push(u);
            vertex_edges[u].push((edge.edge_list_id, edge.edge_id));
        }
    }

    let mut visited = vec![false; n_vertices];
    let mut components: Vec<Vec<OsmNodeId>> = Vec::new();
    let mut component_vertices: Vec<Vec<usize>> = Vec::new();

    for start_v in 0..n_vertices {
        if visited[start_v] || (adj[start_v].is_empty() && vertex_edges[start_v].is_empty()) {
            continue;
        }
        let mut comp_v = Vec::new();
        let mut queue = VecDeque::new();
        visited[start_v] = true;
        queue.push_back(start_v);
        while let Some(curr) = queue.pop_front() {
            comp_v.push(curr);
            for &nbr in &adj[curr] {
                if !visited[nbr] {
                    visited[nbr] = true;
                    queue.push_back(nbr);
                }
            }
        }
        components.push(comp_v.iter().map(|&idx| OsmNodeId(idx as i64)).collect());
        component_vertices.push(comp_v);
    }

    let kept_pseudo = component_filter.assign_components(components);
    let kept_start_ids: HashSet<usize> = kept_pseudo
        .iter()
        .filter_map(|c| c.first().map(|id| id.0 as usize))
        .collect();

    let mut kept_vertices: HashSet<VertexId> = HashSet::new();
    for comp in component_vertices {
        if comp.first().is_some_and(|first| kept_start_ids.contains(first)) {
            for v in comp {
                kept_vertices.insert(VertexId(v));
            }
        }
    }

    edge_list
        .edges()
        .filter(|e| {
            !kept_vertices.contains(&e.src_vertex_id) || !kept_vertices.contains(&e.dst_vertex_id)
        })
        .map(|e| (e.edge_list_id, e.edge_id))
        .collect()
}
