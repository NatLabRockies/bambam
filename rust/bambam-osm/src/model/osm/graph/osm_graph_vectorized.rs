use std::collections::{HashMap, HashSet, VecDeque};

use super::{
    OsmNodeDataSerializable, OsmNodeId, OsmNodesSerializable, OsmWayDataSerializable,
    OsmWaysSerializable, Vertex, VertexLookup,
};
use crate::{
    algorithm::truncation::ComponentFilter,
    model::osm::{
        graph::{osm_node_data::OsmConnectorRecord, OsmNodeData, OsmWays},
        import_ops::{
            extract_osm_connectors_and_segments_with_match_distance, process_osm_segment_splits,
            OsmCollection, OsmSegmentRecord,
        },
        OsmError,
    },
};
use bambam_network::{
    attach_infrastructure_to_vertices,
    serialize_ops::{self as net_ops, clean_edge_list, compute_vertex_remapping},
    CliBoundingBox, EdgeListStats, EdgeMappingRecord, IslandDetectionAlgorithm,
    NetworkEdgeListConfiguration, NetworkGraphSource, NetworkGraphStats, NetworkGraphSummary,
    SegmentAccessRestrictionWhen, SegmentFullType, SegmentHeading, VectorizedEdgeList,
    VertexCompleteRecord, VertexMappingRecord, DEFAULT_INFRASTRUCTURE_MATCH_DISTANCE_METERS,
    GLOBAL_AVG_SPEED_KEY,
};
use geo::{BoundingRect, Geometry};
use indexmap::IndexMap;
use rayon::prelude::*;
use routee_compass_core::model::network::{EdgeId, EdgeList, EdgeListId, VertexId};

pub type OsmEdgeList = VectorizedEdgeList<SegmentFullType>;

pub struct OsmGraphVectorized {
    /// study region / dataset name for `summary.toml`
    pub study_region: String,
    /// bounding box of the extent (if provided) for `summary.toml`
    pub bbox: Option<CliBoundingBox>,
    /// RouteE Compass vertices in final `VertexId` order
    pub vertices: Vec<Vertex>,
    /// the collection of OSM nodes associated via their OSMID
    pub nodes: OsmNodesSerializable,
    /// list of OSM ways in vectorized EdgeId order (populated from the first edge list for legacy tests)
    pub ways: OsmWaysSerializable,
    /// for each OsmNodeId, the vertex index and Vertex
    pub vertex_lookup: VertexLookup,
    /// enriched vertices with traffic_signals, stop, give_way, crossing
    pub vertices_complete: Vec<VertexCompleteRecord>,
    /// standardized vertex ID to OSM node ID mapping
    pub vertex_mappings: Vec<VertexMappingRecord>,
    /// standardized edge ID to OSM way ID + linear reference mapping (first edge list)
    pub edge_mappings: Vec<EdgeMappingRecord>,
    /// primary vectorized edge list (first edge list)
    pub edge_list: Option<OsmEdgeList>,
    /// mode-specific vectorized edge lists matching `bambam-omf` (`edge_lists` + `edge_list_config`)
    pub edge_lists: Vec<OsmEdgeList>,
    /// mode configurations corresponding to `edge_lists`
    pub edge_list_config: Vec<NetworkEdgeListConfiguration>,
    /// optional per-mode `(mode_name, OsmEdgeList)` pairs
    pub mode_edge_lists: Vec<(String, OsmEdgeList)>,
    /// raw OSM nodes and ways collection for Parquet retention
    pub raw_collection: Option<OsmCollection>,
}

impl OsmGraphVectorized {
    /// Builds an [`OsmGraphVectorized`] using the OMF-style Connector + Segment + Split
    /// workflow from `bambam-network` with default multimodal (`walk`, `bike`, `drive`)
    /// configurations.
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
        let configs = NetworkEdgeListConfiguration::default_multimodal();
        Self::from_osm_maps_with_config(
            source_label,
            None,
            nodes_map,
            ways_map,
            extent_opt,
            &configs,
            component_filter,
            island_detection_opt,
            truncate_by_edge,
            simplify,
            ignore_errors,
            infrastructure_match_distance_meters,
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
        let configs = match modes.filter(|m| !m.is_empty()) {
            Some(mode_names) => mode_names
                .iter()
                .map(|m| {
                    NetworkEdgeListConfiguration::default_for_mode(m)
                        .map_err(OsmError::ConfigurationError)
                })
                .collect::<Result<Vec<_>, _>>()?,
            None => NetworkEdgeListConfiguration::default_multimodal(),
        };
        Self::from_osm_maps_with_config(
            source_label,
            None,
            nodes_map,
            ways_map,
            extent_opt,
            &configs,
            component_filter,
            island_detection_opt,
            truncate_by_edge,
            simplify,
            ignore_errors,
            infrastructure_match_distance_meters,
        )
    }

    /// Builds an [`OsmGraphVectorized`] using explicit [`NetworkEdgeListConfiguration`]s
    /// (such as from `travel-mode-filter.json`) and multi-mode island detection, mirroring
    /// `OmfGraphVectorized::new_with_match_distance`.
    #[allow(clippy::too_many_arguments)]
    pub fn from_osm_maps_with_config(
        source_label: impl Into<String>,
        study_region_opt: Option<String>,
        nodes_map: HashMap<OsmNodeId, OsmNodeData>,
        ways_map: OsmWays,
        extent_opt: Option<&Geometry<f32>>,
        configuration: &[NetworkEdgeListConfiguration],
        component_filter: &ComponentFilter,
        island_detection_opt: Option<&IslandDetectionAlgorithm>,
        truncate_by_edge: bool,
        simplify: bool,
        ignore_errors: bool,
        infrastructure_match_distance_meters: Option<f64>,
    ) -> Result<OsmGraphVectorized, OsmError> {
        let source_str = source_label.into();
        let study_region = study_region_opt.unwrap_or_else(|| {
            std::path::Path::new(&source_str)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("osm_network")
                .to_string()
        });

        let bbox = extent_opt
            .and_then(|g| g.bounding_rect())
            .and_then(|rect| CliBoundingBox::from_coords(&rect.min(), &rect.max()).ok());

        let raw_collection = OsmCollection::from_maps(source_str, &nodes_map, &ways_map);

        let (connectors, infrastructure, segments) =
            extract_osm_connectors_and_segments_with_match_distance(
                &nodes_map,
                &ways_map,
                extent_opt,
                truncate_by_edge,
                simplify,
                ignore_errors,
                infrastructure_match_distance_meters,
            )?;

        if connectors.is_empty() {
            return Err(OsmError::NoNodesFound);
        }
        if segments.is_empty() {
            return Err(OsmError::NoWaysFound);
        }

        let effective_configs: Vec<NetworkEdgeListConfiguration> = if configuration.is_empty() {
            NetworkEdgeListConfiguration::default_multimodal()
        } else {
            configuration.to_vec()
        };

        log::info!("Creating vertex lookup");
        let (mut vertices, mut vertex_lookup_str) =
            net_ops::create_vertices_and_lookup(&connectors, None)?;

        log::info!("Processing edge lists");
        let mut edge_lists: Vec<OsmEdgeList> = Vec::with_capacity(effective_configs.len());
        let mut per_list_way_meta: Vec<Vec<(&OsmSegmentRecord, SegmentHeading)>> =
            Vec::with_capacity(effective_configs.len());

        for (index, edge_list_config) in effective_configs.iter().enumerate() {
            let edge_list_id = EdgeListId(index);

            let mut filter = edge_list_config.filter.clone();
            filter.sort();

            log::info!(
                "Filtering edge list {edge_list_id} (mode '{}')",
                edge_list_config.mode
            );
            let mode_segments: Vec<&OsmSegmentRecord> = segments
                .par_iter()
                .filter(|r| filter.iter().all(|f| f.matches_filter(*r)))
                .collect();

            let (vec_edge_list, split_way_meta) = build_osm_edge_list_from_segments(
                &mode_segments,
                &vertices,
                &vertex_lookup_str,
                edge_list_id,
                Some(edge_list_config),
            )?;
            edge_lists.push(vec_edge_list);
            per_list_way_meta.push(split_way_meta);
        }

        // Determine edges to remove from IslandDetectionAlgorithm and/or ComponentFilter across all mode edge lists
        log::info!("Computing islands across mode edge lists");
        let mut island_edges: Vec<(EdgeListId, EdgeId)> = Vec::new();
        for el in &edge_lists {
            island_edges.extend(find_component_filter_removed_edges(
                &el.edges,
                vertices.len(),
                component_filter,
            ));
        }
        if let Some(island_alg) = island_detection_opt {
            let ref_edge_lists: Vec<&EdgeList> =
                edge_lists.iter().map(|e| &e.edges).collect();
            let alg_removed = island_alg.run(&ref_edge_lists, &vertices)?;
            island_edges.extend(alg_removed);
        } else {
            // Also check per-mode island_algorithm_config if set on individual NetworkEdgeListConfiguration
            for (el, cfg) in edge_lists.iter().zip(effective_configs.iter()) {
                if let Some(ref mode_island_alg) = cfg.island_algorithm_config {
                    let alg_removed = mode_island_alg.run(&[&el.edges], &vertices)?;
                    island_edges.extend(alg_removed);
                }
            }
        }

        let connector_by_old_vertex: Vec<&OsmConnectorRecord> = connectors.iter().collect();
        let kept_connectors: Vec<OsmConnectorRecord>;

        if !island_edges.is_empty() {
            let mut edges_lookup: HashMap<EdgeListId, HashSet<EdgeId>> = HashMap::new();
            for (el_id, eid) in &island_edges {
                edges_lookup.entry(*el_id).or_default().insert(*eid);
            }

            let vertex_remapping =
                compute_vertex_remapping(&vertices, &edge_lists, &island_edges)?;

            vertices = vertices
                .into_iter()
                .filter_map(|v| {
                    vertex_remapping[v.vertex_id.0].map(|new_vid| Vertex {
                        vertex_id: new_vid,
                        ..v
                    })
                })
                .collect();

            vertex_lookup_str.retain(|_, v| vertex_remapping[*v].is_some());
            for v in vertex_lookup_str.values_mut() {
                *v = vertex_remapping[*v]
                    .ok_or_else(|| {
                        OsmError::InternalError(format!(
                            "vertex index {v} expected after island computation but was flagged for deletion"
                        ))
                    })?
                    .0;
            }

            kept_connectors = connector_by_old_vertex
                .into_iter()
                .enumerate()
                .filter_map(|(old_vid, conn)| vertex_remapping[old_vid].map(|_| conn.clone()))
                .collect();

            let mut cleaned_lists = Vec::with_capacity(edge_lists.len());
            let mut cleaned_meta = Vec::with_capacity(per_list_way_meta.len());

            for (el, meta) in edge_lists.into_iter().zip(per_list_way_meta.into_iter()) {
                let to_remove = edges_lookup.get(&el.edge_list_id);
                let mask: Vec<bool> = el
                    .edges
                    .0
                    .iter()
                    .map(|e| !to_remove.is_some_and(|set| set.contains(&e.edge_id)))
                    .collect();

                let filtered_meta = meta
                    .into_iter()
                    .enumerate()
                    .filter(|(idx, _)| mask[*idx])
                    .map(|(_, item)| item)
                    .collect();
                let cleaned_el = clean_edge_list(el, mask, &vertex_remapping)?;
                cleaned_lists.push(cleaned_el);
                cleaned_meta.push(filtered_meta);
            }

            edge_lists = cleaned_lists;
            per_list_way_meta = cleaned_meta;
        } else {
            kept_connectors = connectors;
        }

        // Build final node/vertex tables and lookup
        let mut nodes: OsmNodesSerializable = Vec::with_capacity(kept_connectors.len());
        let mut vertex_lookup: VertexLookup = HashMap::with_capacity(kept_connectors.len());
        let mut connector_ids: Vec<String> = Vec::with_capacity(kept_connectors.len());
        let mut vertex_mappings: Vec<VertexMappingRecord> =
            Vec::with_capacity(kept_connectors.len());

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
        // Reflect direct or topologically propagated OSM node control tags on the connector vertex itself
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
            if !conn.node.consolidated_ids.is_empty() {
                let mut ids: Vec<String> = vc
                    .infrastructure_ids
                    .as_deref()
                    .map(|s| {
                        s.split(bambam_network::INFRASTRUCTURE_ID_DELIMITER)
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default();
                for cid in &conn.node.consolidated_ids {
                    let s = cid.to_string();
                    if !ids.contains(&s) {
                        ids.push(s);
                    }
                }
                if !ids.is_empty() {
                    vc.infrastructure_ids =
                        Some(ids.join(bambam_network::INFRASTRUCTURE_ID_DELIMITER));
                }
            }
        }

        // Populate legacy `ways` and `edge_mappings` from the first non-empty edge list (or first list)
        let primary_idx = edge_lists
            .iter()
            .position(|el| !el.edges.is_empty())
            .unwrap_or(0);
        let mut ways: OsmWaysSerializable = Vec::new();
        let mut edge_mappings: Vec<EdgeMappingRecord> = Vec::new();
        let mut primary_edge_list: Option<OsmEdgeList> = None;

        if let (Some(first_el), Some(first_meta)) = (
            edge_lists.get(primary_idx),
            per_list_way_meta.get(primary_idx),
        ) {
            ways.reserve(first_el.edges.len());
            for ((edge, geom), (seg, heading)) in first_el
                .edges
                .0
                .iter()
                .zip(first_el.geometries.iter())
                .zip(first_meta.iter())
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
            edge_mappings = first_el.edge_mappings.clone();
            primary_edge_list = Some(first_el.clone());
        }

        let mode_edge_lists: Vec<(String, OsmEdgeList)> = effective_configs
            .iter()
            .zip(edge_lists.iter())
            .map(|(cfg, el)| (cfg.mode.clone(), el.clone()))
            .collect();

        Ok(OsmGraphVectorized {
            study_region,
            bbox,
            vertices,
            nodes,
            ways,
            vertex_lookup,
            vertices_complete,
            vertex_mappings,
            edge_mappings,
            edge_list: primary_edge_list,
            edge_lists,
            edge_list_config: effective_configs,
            mode_edge_lists,
            raw_collection: Some(raw_collection),
        })
    }

    /// Builds a [`NetworkGraphSummary`] (`summary.toml`) matching the OMF summary structure.
    pub fn to_summary(&self) -> NetworkGraphSummary {
        let release = self
            .raw_collection
            .as_ref()
            .map(|c| format!("file://{}", c.source))
            .unwrap_or_else(|| "osm".to_string());
        let source = NetworkGraphSource::new(&release, &self.study_region, self.bbox.as_ref());
        let mut edge_list = IndexMap::new();
        for (cfg, el) in self.edge_list_config.iter().zip(self.edge_lists.iter()) {
            let stats = EdgeListStats::from_edge_list(el, |cls| cls.as_str());
            edge_list.insert(cfg.mode.clone(), stats);
        }
        let stats = NetworkGraphStats {
            vertices: self.vertices.len(),
            edge_list,
        };
        NetworkGraphSummary { source, stats }
    }
}

type BuiltOsmEdgeList<'a> = (
    VectorizedEdgeList<SegmentFullType>,
    Vec<(&'a OsmSegmentRecord, SegmentHeading)>,
);

fn build_osm_edge_list_from_segments<'a>(
    segment_refs: &[&'a OsmSegmentRecord],
    vertices: &[Vertex],
    vertex_lookup_str: &HashMap<String, usize>,
    edge_list_id: EdgeListId,
    edge_list_config: Option<&NetworkEdgeListConfiguration>,
) -> Result<BuiltOsmEdgeList<'a>, OsmError> {
    if segment_refs.is_empty() {
        let mut speed_lookup = HashMap::new();
        speed_lookup.insert(GLOBAL_AVG_SPEED_KEY.to_string(), 25.0);
        return Ok((
            VectorizedEdgeList {
                edge_list_id,
                edges: EdgeList(Box::new([])),
                geometries: Vec::new(),
                classes: Vec::new(),
                speeds: Vec::new(),
                speed_lookup,
                bearings: Vec::new(),
                edge_mappings: Vec::new(),
            },
            Vec::new(),
        ));
    }

    let segment_lookup =
        net_ops::create_segment_lookup::<OsmSegmentRecord, SegmentFullType>(segment_refs);

    let mut splits = Vec::new();
    for heading in [SegmentHeading::Forward, SegmentHeading::Backward] {
        let mut when: SegmentAccessRestrictionWhen = edge_list_config
            .map(SegmentAccessRestrictionWhen::from)
            .unwrap_or_default();
        when.heading = Some(heading);

        let directed_splits =
            net_ops::find_splits(segment_refs, Some(&when), process_osm_segment_splits)?;
        splits.extend(directed_splits);
    }

    let edges = net_ops::create_edges::<OsmSegmentRecord, SegmentFullType>(
        segment_refs,
        &segment_lookup,
        &splits,
        vertices,
        vertex_lookup_str,
        edge_list_id,
    )?;
    let geometries = net_ops::create_geometries::<OsmSegmentRecord, SegmentFullType>(
        segment_refs,
        &segment_lookup,
        &splits,
    )?;
    let bearings = net_ops::bearing_deg_from_geometries(&geometries)?;
    let classes = net_ops::create_classes::<OsmSegmentRecord, SegmentFullType>(
        segment_refs,
        &segment_lookup,
        &splits,
    )?;
    let initial_speeds = net_ops::create_speeds::<OsmSegmentRecord, SegmentFullType>(
        segment_refs,
        &segment_lookup,
        &splits,
    )?;
    let speed_lookup_by_cls =
        net_ops::create_speed_by_class_lookup::<OsmSegmentRecord, SegmentFullType>(
            &initial_speeds,
            segment_refs,
            &segment_lookup,
            &splits,
            &classes,
        )?;

    let global_speed = net_ops::get_global_average_speed::<OsmSegmentRecord, SegmentFullType>(
        &initial_speeds,
        segment_refs,
        &segment_lookup,
        &splits,
    )
    .unwrap_or(25.0);

    let speeds: Vec<f64> = initial_speeds
        .into_par_iter()
        .zip(&classes)
        .map(|(opt_speed, cls)| {
            opt_speed
                .or_else(|| speed_lookup_by_cls.get(cls).copied())
                .unwrap_or(global_speed)
        })
        .collect();

    let mut speed_lookup: HashMap<String, f64> = speed_lookup_by_cls
        .into_iter()
        .map(|(k, v)| (k.as_str(), v))
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
        if comp
            .first()
            .is_some_and(|first| kept_start_ids.contains(first))
        {
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
