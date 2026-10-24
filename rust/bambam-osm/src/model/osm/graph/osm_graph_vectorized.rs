use std::collections::{HashMap, HashSet, VecDeque};

use super::{
    CompassIndex, Itertools, OsmError, OsmGraph, OsmNodeDataSerializable, OsmNodeId,
    OsmNodesSerializable, OsmWayDataSerializable, OsmWaysSerializable, Vertex, VertexLookup,
};
use crate::{
    algorithm::truncation::ComponentFilter,
    model::{
        feature::highway::Highway,
        osm::{
            graph::{
                osm_node_data::OsmConnectorRecord, osm_way_data_serializable::create_linestring_for_od_path,
                OsmNodeData, OsmWayData, OsmWays,
            },
            import_ops::{
                extract_osm_connectors_and_segments, process_osm_segment_splits, OsmCollection,
                OsmSegmentRecord,
            },
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
use geo::{Convert, Geometry};
use geozero::ToWkt;
use kdam::tqdm;
use rayon::prelude::*;
use routee_compass_core::model::network::{EdgeId, EdgeList, EdgeListId, VertexId};

pub struct OsmGraphVectorized {
    /// the collection of OSM nodes associated via their OSMID
    pub nodes: OsmNodesSerializable,
    /// just a list of OSM ways in an arbitrary order. these are unique by OSMID but
    /// not guaranteed to be unique by source and destination node (i.e., multigraph).
    pub ways: OsmWaysSerializable,
    /// for each OsmNodeId, the vertex index
    pub vertex_lookup: VertexLookup,
    /// enriched vertices with traffic_signals, stop, give_way, crossing
    pub vertices_complete: Vec<OmfVertexComplete>,
    /// standardized vertex ID to OSM node ID mapping
    pub vertex_mappings: Vec<VertexMappingRecord>,
    /// standardized edge ID to OSM way ID + linear reference mapping
    pub edge_mappings: Vec<EdgeMappingRecord>,
    /// shared vectorized edge list from bambam-network
    pub edge_list: Option<VectorizedEdgeList<Highway>>,
    /// raw OSM nodes and ways collection for Parquet retention
    pub raw_collection: Option<OsmCollection>,
    /// legacy reference graph (empty under the split-based pipeline)
    pub reference_graph: OsmGraph,
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

        let segment_refs: Vec<&OsmSegmentRecord> = segments.iter().collect();
        let segment_lookup = net_ops::create_segment_lookup::<OsmSegmentRecord, Highway>(&segment_refs);

        let mut splits = Vec::new();
        for heading in [SegmentHeading::Forward, SegmentHeading::Backward] {
            let directed_splits = net_ops::find_splits(
                &segment_refs,
                Some(&heading),
                process_osm_segment_splits,
            )?;
            splits.extend(directed_splits);
        }

        let edge_list_id = EdgeListId(0);
        let edges = net_ops::create_edges::<OsmSegmentRecord, Highway>(
            &segment_refs,
            &segment_lookup,
            &splits,
            &vertices,
            &vertex_lookup_str,
            edge_list_id,
        )?;
        let geometries = net_ops::create_geometries::<OsmSegmentRecord, Highway>(
            &segment_refs,
            &segment_lookup,
            &splits,
        )?;
        let bearings = net_ops::bearing_deg_from_geometries(&geometries)?;
        let classes = net_ops::create_segment_full_types::<OsmSegmentRecord, Highway>(
            &segment_refs,
            &segment_lookup,
            &splits,
        )?;
        let initial_speeds = net_ops::create_speeds::<OsmSegmentRecord, Highway>(
            &segment_refs,
            &segment_lookup,
            &splits,
        )?;
        let speed_lookup_by_hw = net_ops::create_speed_by_segment_type_lookup::<OsmSegmentRecord, Highway>(
            &initial_speeds,
            &segment_refs,
            &segment_lookup,
            &splits,
            &classes,
        )?;

        let global_speed = net_ops::get_global_average_speed::<OsmSegmentRecord, Highway>(
            &initial_speeds,
            &segment_refs,
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

        // Build parent OSM way ID + parent linear reference mappings and serializable way records
        let mut edge_mappings: Vec<EdgeMappingRecord> = Vec::with_capacity(splits.len());
        let mut omf_segment_ids: Vec<(String, f64)> = Vec::with_capacity(splits.len());
        let mut split_way_meta: Vec<(&OsmSegmentRecord, SegmentHeading)> =
            Vec::with_capacity(splits.len());

        for (edge_idx, split) in splits.iter().enumerate() {
            let seg_idx = segment_lookup[&split.src.segment_id];
            let seg = segment_refs[seg_idx];
            let way_id_str = seg.parent_way_id.to_string();
            edge_mappings.push(EdgeMappingRecord {
                edge_id: edge_idx,
                source_id: way_id_str.clone(),
                start_lr: seg.parent_start_lr,
                end_lr: seg.parent_end_lr,
                heading: split.heading.clone(),
            });
            omf_segment_ids.push((way_id_str, seg.parent_start_lr));
            split_way_meta.push((seg, split.heading.clone()));
        }

        let mut vec_edge_list = VectorizedEdgeList {
            edge_list_id,
            edges: EdgeList(edges.into_boxed_slice()),
            geometries,
            classes,
            speeds,
            speed_lookup,
            bearings,
            omf_segment_ids,
            edge_mappings,
        };

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
        let mut connector_ids: Vec<String> = Vec::with_capacity(kept_connectors.len());
        let mut vertex_mappings: Vec<VertexMappingRecord> = Vec::with_capacity(kept_connectors.len());

        for (vertex_id, (conn, vertex)) in kept_connectors.iter().zip(vertices.iter()).enumerate() {
            nodes.push(OsmNodeDataSerializable::from(&conn.node));
            vertex_lookup.insert(conn.node.osmid, (vertex_id, *vertex));
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

        Ok(OsmGraphVectorized {
            nodes,
            ways,
            vertex_lookup,
            vertices_complete,
            vertex_mappings,
            edge_mappings,
            edge_list: Some(vec_edge_list),
            raw_collection: Some(raw_collection),
            reference_graph: OsmGraph::empty(),
        })
    }

    /// vectorizes an [`OsmGraph`] such that the position of each node and way in each vector
    /// (their index) becomes their respective VectorId/EdgeId.
    pub fn new(
        graph: OsmGraph,
        ignore_serialization_errors: bool,
    ) -> Result<OsmGraphVectorized, OsmError> {
        // create vertex_ids, serializable nodes and vertex lookup (one-pass)
        let mut nodes: OsmNodesSerializable = Vec::with_capacity(graph.n_connected_nodes());
        let mut vertex_lookup: HashMap<OsmNodeId, (CompassIndex, Vertex)> = HashMap::new();
        let node_iter = tqdm!(
            graph.connected_node_data_iterator(true).enumerate(),
            total = graph.n_connected_nodes(),
            desc = "osm nodes to compass vertices"
        );
        for (vertex_id, result) in node_iter {
            let node = result?;
            let node_ser = OsmNodeDataSerializable::from(node);
            nodes.insert(vertex_id, node_ser);

            let vertex = Vertex::new(vertex_id, node.x, node.y);
            vertex_lookup.insert(node.osmid, (vertex_id, vertex));
        }
        eprintln!();

        // create edge_ids and serializable ways
        let triplet_iter = tqdm!(
            graph.connected_multiedge_way_triplet_iterator(true),
            total = graph.n_connected_ways(),
            desc = "osm ways to compass edges"
        );
        let mut ways: OsmWaysSerializable = vec![];
        for (idx, traj_result) in triplet_iter.enumerate() {
            match traj_result {
                Ok(None) => {}
                Ok(Some(traj)) if traj.is_empty() => {
                    return Err(OsmError::InternalError(format!(
                        "way with EdgeId {idx} has no trajectories"
                    )))
                }
                Ok(Some(traj)) if invalid_linestring(&traj, &graph) => {
                    let linestring = debug_linestring(&traj, &graph);
                    log::warn!("connected ways triplet iterator provided invalid trajectory with linestring: '{linestring}'");
                }
                Ok(Some(traj)) => match OsmWayDataSerializable::new(traj, &graph, &vertex_lookup) {
                    Ok(way) => ways.push(way),
                    Err(_) if ignore_serialization_errors => {}
                    Err(e) => return Err(e),
                },
                Err(e) => return Err(OsmError::GraphModificationError(e.to_string())),
            }
        }
        eprintln!();

        let result = OsmGraphVectorized {
            nodes,
            ways,
            vertex_lookup,
            vertices_complete: vec![],
            vertex_mappings: vec![],
            edge_mappings: vec![],
            edge_list: None,
            raw_collection: None,
            reference_graph: graph,
        };
        Ok(result)
    }
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

fn invalid_linestring(
    traj: &[(&OsmNodeData, &OsmWayData, &OsmNodeData)],
    graph: &OsmGraph,
) -> bool {
    let nodes_connected = traj
        .iter()
        .flat_map(|(_, e, _)| e.nodes.clone())
        .collect::<HashSet<_>>();
    let mut coords = nodes_connected
        .iter()
        .flat_map(|n| graph.get_node_data(n).ok())
        .map(|n| n.get_point())
        .collect_vec();
    coords.dedup_by(|a, b| {
        ((a.x() * 10000.0f32) as i64) == ((b.x() * 10000.0f32) as i64)
            && ((a.y() * 10000.0f32) as i64) == ((b.y() * 10000.0f32) as i64)
    });
    nodes_connected.len() < 2
}

fn debug_linestring(
    traj: &[(&OsmNodeData, &OsmWayData, &OsmNodeData)],
    graph: &OsmGraph,
) -> String {
    match traj.first() {
        None => "invalid: empty linestring".to_string(),
        Some((src, way, dst)) => create_linestring_for_od_path(&src.osmid, &dst.osmid, way, graph)
            .map(|l| {
                let l_f64: geo::LineString<f64> = l.convert();
                geo::Geometry::from(l_f64).to_wkt().unwrap_or_default()
            })
            .unwrap_or_else(|_| "invalid: unable to construct linestring".to_string()),
    }
}
