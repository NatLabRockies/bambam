use super::{
    graph::{
        osm_element_filter::ElementFilter, osm_node_data::OsmConnectorRecord,
        osm_segment::OsmSegment, osm_way_ops, AdjacencyListDeprecated, OsmNodeId, OsmNodes,
        OsmWayId, OsmWays,
    },
    OsmError,
};
use crate::{
    algorithm::buffer,
    model::{
        feature::highway::Highway,
        osm::graph::{
            osm_node_data::OsmNodeData, osm_way_data::OsmWayData, AdjacencyDirection, AdjacencyList,
        },
    },
};
use arrow::datatypes::FieldRef;
use bambam_network::{ConnectorInSegment, ImportSegment, NetworkImportError, SegmentHeading, SegmentSplit};
use geo::{BoundingRect, Coord, Geometry, Haversine, Intersects, Length, Line, LineString};
use itertools::Itertools;
use kdam::{term, tqdm, Bar, BarExt};
use osmpbf::{Element, ElementReader};
use parquet::{
    arrow::{arrow_reader::ParquetRecordBatchReaderBuilder, ArrowWriter},
    basic::Compression,
    file::{metadata::KeyValue, properties::WriterProperties},
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_arrow::schema::{SchemaLike, TracingOptions};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    path::Path,
    str::FromStr,
};

pub const RAW_DIRNAME: &str = "raw";
pub const RAW_NODES_PARQUET: &str = "nodes.parquet";
pub const RAW_WAYS_PARQUET: &str = "ways.parquet";
pub const PARQUET_SOURCE_METADATA_KEY: &str = "osm_source";

/// an estimate of this is fine, it's used to buffer the extent of the study area
/// by a smidge so we don't prematurely truncate edges when reading the network source.
// pub const BUFFER_500M_IN_DEGREES: f32 = 0.0045000045;
pub const BUFFER_500M: f32 = 500.0;

/// reads a PBF file and stores the Ways and Nodes in lookup objects. filters out nodes
/// and ways based on the filter and extent_opt arguments:
/// - if provided, extent_opt will filter out nodes with points found outside of the extent
/// - the provided [`ElementFilter`] filters rows based on their [`Highway`] tag
/// - ways that had their nodes removed are also removed
pub fn read_pbf(
    filepath: &str,
    filter: ElementFilter,
    extent_opt: &Option<Geometry<f32>>,
) -> Result<(OsmNodes, OsmWays), OsmError> {
    let fp = Path::new(filepath);
    let reader = ElementReader::from_path(fp).map_err(|e| OsmError::PbfLibError { source: e })?;

    // if provided an extent geometry, it is buffered by 500 meters, and then used as a node filter function
    let ext_buffered_opt = match extent_opt {
        Some(g) => {
            log::info!("buffering extent for initial download filtering");
            let g_buf = buffer::scale_exterior(g, BUFFER_500M).map_err(|e| {
                OsmError::ConfigurationError(format!("failure buffering extent: {e}"))
            })?;
            Ok::<_, OsmError>(Some(g_buf))
        }
        None => Ok(None),
    }?;
    let within_extent_fn = Box::new(|g: &OsmNodeData| match ext_buffered_opt.as_ref() {
        Some(ext) => g.intersects(ext),
        None => true,
    });

    term::hide_cursor().map_err(|e| OsmError::InternalError(e.to_string()))?;
    let mut reader_bar = Bar::builder()
        .desc(filepath)
        .position(0)
        .unit(" rows")
        .unit_scale(true)
        .build()
        .map_err(OsmError::InternalError)?;
    let mut nodes_bar = Bar::builder()
        .desc("nodes retained")
        .position(1)
        .build()
        .map_err(OsmError::InternalError)?;
    let mut ways_bar = Bar::builder()
        .desc("ways retained")
        .position(2)
        .build()
        .map_err(OsmError::InternalError)?;

    let mut nodes_map: OsmNodes = HashMap::default();
    let mut ways_map: OsmWays = HashMap::default();
    // pull in all Node and Way rows. along the way, track which NodeOsmids are
    // present in the ways that have been accepted by the filter function.
    reader
        .for_each(|e| {
            let valid_element = filter.accept(&e);
            match e {
                Element::Node(_) if !valid_element => {
                    // Skip invalid nodes
                }
                Element::Node(node) => {
                    if node.id() == 0 {
                        log::warn!(
                            "node missing OSMID at ({},{}) ignored",
                            node.lon(),
                            node.lat()
                        );
                    } else {
                        let n = OsmNodeData::from(&node);
                        if nodes_map.contains_key(&n.osmid) {
                            log::warn!(
                                "node with OSMID {} occurs more than once in this file",
                                n.osmid
                            );
                        }
                        if within_extent_fn(&n) {
                            let _ = nodes_bar.update(1);
                            nodes_map.insert(n.osmid, n);
                        }
                    }
                }
                Element::DenseNode(_) if !valid_element => {
                    // Skip invalid dense nodes
                }
                Element::DenseNode(dense) => {
                    // from documentation on DenseNode:
                    // So, if you want to [pattern match on] `Node`, you also likely want to match [`DenseNode`].
                    let n = OsmNodeData::from(&dense);
                    if nodes_map.contains_key(&n.osmid) {
                        log::warn!(
                            "node with OSMID {} occurs more than once in this file",
                            n.osmid
                        );
                    }
                    if within_extent_fn(&n) {
                        let _ = nodes_bar.update(1);
                        nodes_map.insert(n.osmid, n);
                    }
                }
                Element::Way(_) if !valid_element => {
                    // Skip invalid ways
                }
                Element::Way(way) => {
                    let w = OsmWayData::new(&way);
                    if ways_map.contains_key(&w.osmid) {
                        log::warn!(
                            "way with OSMID {} occurs more than once in this file",
                            w.osmid
                        );
                    }
                    let _ = ways_bar.update(1);
                    ways_map.insert(w.osmid, w);
                }
                Element::Relation(_) => {}
            }
            let _ = reader_bar.update(1);
        })
        .map_err(|e| OsmError::PbfLibError { source: e })?;

    // close the 3 nested progress bars
    eprintln!();
    eprintln!();
    eprintln!();
    term::show_cursor().map_err(|e| OsmError::InternalError(e.to_string()))?;

    if nodes_map.is_empty() {
        return Err(OsmError::NoNodesFound);
    }
    if ways_map.is_empty() {
        return Err(OsmError::NoWaysFound);
    }

    // // # G.add_nodes_from(nodes.items())
    // // # _add_paths(G, paths.values(), bidirectional)  # where paths is Map[Osmid, Way]

    if extent_opt.is_some() {
        // Clip ways to contiguous runs of nodes that exist within the buffered extent,
        // rather than discarding an entire long highway if one of its ends extends beyond the buffer.
        let mut retained_ways: OsmWays = HashMap::default();
        let mut connected_nodes: HashSet<OsmNodeId> = HashSet::new();

        for way in ways_map.into_values() {
            let mut run_idx = 0usize;
            for slice in way.nodes.split(|n_id| !nodes_map.contains_key(n_id)) {
                if slice.len() >= 2 {
                    for node_id in slice {
                        connected_nodes.insert(*node_id);
                    }
                    let mut sub_way = way.clone();
                    sub_way.nodes = slice.to_vec();
                    // If a way was split into multiple disconnected runs across the boundary,
                    // derive a deterministic synthetic key while keeping the original osmid in `way_ids`.
                    if run_idx > 0 {
                        if !sub_way.way_ids.contains(&way.osmid) {
                            sub_way.way_ids.push(way.osmid);
                        }
                        let synthetic_id = OsmWayId(
                            way.osmid
                                .0
                                .wrapping_mul(1_000_000)
                                .wrapping_add(run_idx as i64),
                        );
                        sub_way.osmid = synthetic_id;
                        retained_ways.insert(synthetic_id, sub_way);
                    } else {
                        retained_ways.insert(way.osmid, sub_way);
                    }
                    run_idx += 1;
                }
            }
        }
        ways_map = retained_ways;

        // Retain connected way nodes plus any standalone control/signal nodes inside the extent
        nodes_map.retain(|node_id, node_data| {
            connected_nodes.contains(node_id) || node_data.has_control_or_intersection_tag()
        });
    }

    log::info!(
        "{} ways and {} nodes collected from OSM pbf resource.",
        ways_map.len(),
        nodes_map.len(),
    );
    Ok((nodes_map, ways_map))
}

pub fn build_adjacencies(ways_map: &OsmWays) -> Result<AdjacencyList, OsmError> {
    let mut adj: AdjacencyList = HashMap::new();

    let ways_iter = tqdm!(
        ways_map.values(),
        total = ways_map.len(),
        desc = "building adjacency list"
    );
    for way in ways_iter {
        // # extract/remove the ordered list of nodes from this path element so
        // # we don't add it as a superfluous attribute to the edge later
        let mut nodes = way.nodes.clone();

        // # reverse the order of nodes in the path if this path is both one-way
        // # and only allows travel in the opposite direction of nodes' order
        let oneway = way.is_one_way();
        let reverse = way.is_reverse();
        if oneway && reverse {
            nodes.reverse();
        }

        // # set the oneway attribute, but only if when not forcing all edges to
        // # oneway with the all_oneway setting. With the all_oneway setting, you
        // # want to preserve the original OSM oneway attribute for later clarity
        // --> SKIPPED since we don't support this

        // # zip path nodes to get (u, v) tuples like [(0,1), (1,2), (2,3)].
        // # add all the edge tuples and give them the path's tag:value attrs
        for (src_id, dst_id) in nodes.iter().tuple_windows() {
            // # G.add_edges_from(edges, **path)
            // insert forward-oriented segment
            let fwd_segs = adj
                .entry((*src_id, AdjacencyDirection::Forward))
                .or_default();
            insert_op(dst_id, way, oneway, fwd_segs)?;

            if !oneway {
                // # G.add_edges_from((v, u) for u, v in edges], **path)
                // insert reverse-oriented segment
                let rev_segs = adj
                    .entry((*dst_id, AdjacencyDirection::Reverse))
                    .or_default();
                insert_op(src_id, way, false, rev_segs)?;
            }
        }
    }
    eprintln!();

    log::info!(
        "adjacency list has {} nodes, {} segments",
        adj.len(),
        adj.values().map(|adj| adj.len()).sum::<usize>()
    );
    Ok(adj)
}

/// inserts adjacencies into the adjacency list during initialization.
/// not intended for simplified graphs, as this maintains the invariant that there
/// exists one way between each (src, dst) node pair.
fn insert_op(
    target_id: &OsmNodeId,
    way: &OsmWayData,
    is_oneway: bool,
    entry: &mut HashMap<OsmNodeId, Vec<OsmSegment>>,
) -> Result<(), OsmError> {
    let highway = match &way.highway {
        Some(h) => {
            let highway = h
                .parse::<Highway>()
                .map_err(|e| OsmError::InvalidOsmData(format!("unknown highway tag '{h}': {e}")))?;
            Some(highway)
        }
        None => None,
    };
    let next_segment = OsmSegment::new(way.osmid, highway, is_oneway);

    // from osmnx.graph.simplification:378:
    //      # ...if multiple edges exist between
    //      # them (see above), we retain only one in the simplified graph
    //      # We can't assume that there exists an edge from u to v
    //      # with key=0, so we get a list of all edges from u to v
    //      # and just take the first one.
    // aka, OSMNX randomly picks a tiebreaker. here let's pick the option that
    // is higher in the road network hierarchy if possible (defaulting to random).
    match entry.get(target_id) {
        Some(prev_segments) => {
            match &prev_segments.as_slice() {
                [prev_segment] if &next_segment <= prev_segment => {
                    let _ = entry.insert(*target_id, vec![next_segment]);
                    // newly-inserted segment has greater highway authority, replace the old one
                }
                _ => {} // NOOP
            }
        }
        None => {
            let _ = entry.insert(*target_id, vec![next_segment]);
        }
    }
    Ok(())
}

// fn insert_op2(
//     target_id: &OsmNodeId,
//     way: &OsmWayData,
//     entry: &mut HashMap<OsmNodeId, OsmWayData>,
// ) -> Result<(), OsmError> {
//     // determine if we should insert this new way, depending on whether there is an
//     // existing way in this entry, and if so, how their highway tags compare.
//     // rjf 2025-03-06: should we actually combine these instead?
//     let insert_way = match (
//         way.get_highway(),
//         entry.get(target_id).map(|w| w.get_highway()),
//     ) {
//         (Ok(None), _) => false,
//         (Ok(Some(_highway)), None) => true,
//         (Ok(Some(new_highway)), Some(Ok(Some(old_highway)))) => new_highway < old_highway,
//         (_, Some(Ok(None))) => {
//             return Err(OsmError::InternalError(String::from(
//                 "existing way in the graph has no highway tag",
//             )))
//         }
//         (Ok(_), Some(Err(e))) => {
//             return Err(OsmError::InternalError(format!(
//                 "existing way in the graph has an invalid highway tag '{}': {}",
//                 way.highway.clone().unwrap_or_default(),
//                 e
//             )))
//         }
//         (Err(e), None) => return Err(OsmError::InvalidOsmData(format!("{e}"))),
//         (Err(e), Some(_)) => return Err(OsmError::InvalidOsmData(format!("{e}"))),
//     };

//     if insert_way {
//         let _ = entry.insert(*target_id, way.clone());
//     }

//     Ok(())
// }

pub fn build_rev_adjacency_list(fwd: &AdjacencyListDeprecated) -> AdjacencyListDeprecated {
    let mut rev: AdjacencyListDeprecated = HashMap::new();
    let segments_iter = tqdm!(
        fwd.iter(),
        desc = "create reverse adjacency list",
        total = fwd.len()
    );
    for (src, edges) in segments_iter {
        for (dst, edge) in edges.iter() {
            match rev.get_mut(dst) {
                Some(rev_edges) => {
                    rev_edges.insert(*src, edge.clone());
                }
                None => {
                    rev.insert(*dst, HashMap::from([(*src, edge.clone())]));
                }
            }
        }
    }
    rev
}

/// Raw OSM collection containing extracted nodes and ways, supporting always-on
/// GeoParquet/Parquet persistence in `<output_directory>/raw/`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OsmCollection {
    pub source: String,
    pub nodes: Vec<OsmNodeData>,
    pub ways: Vec<OsmWayData>,
}

impl OsmCollection {
    pub fn from_maps(source: impl Into<String>, nodes: &OsmNodes, ways: &OsmWays) -> Self {
        let sorted_nodes = nodes
            .values()
            .sorted_by_key(|n| n.osmid)
            .cloned()
            .collect_vec();
        let sorted_ways = ways
            .values()
            .sorted_by_key(|w| w.osmid)
            .cloned()
            .collect_vec();
        Self {
            source: source.into(),
            nodes: sorted_nodes,
            ways: sorted_ways,
        }
    }

    /// Write raw OSM nodes and ways to `<output_directory>/raw/nodes.parquet` and `ways.parquet`.
    pub fn to_parquet(&self, output_directory: &Path) -> Result<(), OsmError> {
        let raw_dir = output_directory.join(RAW_DIRNAME);
        bambam_network::fs::create_dirs(&raw_dir)?;

        write_records_to_parquet(
            &self.nodes,
            &raw_dir.join(RAW_NODES_PARQUET),
            &self.source,
        )?;
        write_records_to_parquet(
            &self.ways,
            &raw_dir.join(RAW_WAYS_PARQUET),
            &self.source,
        )?;
        Ok(())
    }

    /// Read raw OSM nodes and ways from `<path>/raw/` or `<path>/`.
    pub fn from_parquet_dir(path: &Path) -> Result<Self, OsmError> {
        let raw_dir = if path.join(RAW_WAYS_PARQUET).exists() {
            path.to_path_buf()
        } else if path.join(RAW_DIRNAME).join(RAW_WAYS_PARQUET).exists() {
            path.join(RAW_DIRNAME)
        } else {
            return Err(OsmError::ConfigurationError(format!(
                "expected '{RAW_WAYS_PARQUET}' in '{}' or '{}/{RAW_DIRNAME}'",
                path.display(),
                path.display()
            )));
        };

        let (nodes, n_src) = read_records_from_parquet::<OsmNodeData>(&raw_dir.join(RAW_NODES_PARQUET))?;
        let (ways, w_src) = read_records_from_parquet::<OsmWayData>(&raw_dir.join(RAW_WAYS_PARQUET))?;
        let source = w_src
            .or(n_src)
            .unwrap_or_else(|| format!("file://{}", raw_dir.to_string_lossy()));

        Ok(Self {
            source,
            nodes,
            ways,
        })
    }
}

fn write_records_to_parquet<T>(
    records: &[T],
    filepath: &Path,
    source: &str,
) -> Result<(), OsmError>
where
    T: Serialize + DeserializeOwned,
{
    let tracing_options = TracingOptions::default()
        .allow_null_fields(true)
        .enums_without_data_as_strings(true)
        .map_as_struct(false);
    let fields = Vec::<FieldRef>::from_type::<T>(tracing_options).map_err(|e| {
        OsmError::GraphSerializationError(format!(
            "failed to trace Arrow schema for '{}': {e}",
            filepath.display()
        ))
    })?;
    let batch = serde_arrow::to_record_batch::<&[T]>(&fields, &records).map_err(|e| {
        OsmError::GraphSerializationError(format!(
            "failed to serialize records into Arrow RecordBatch for '{}': {e}",
            filepath.display()
        ))
    })?;

    let file = File::create(filepath).map_err(|e| {
        OsmError::GraphSerializationError(format!(
            "failed to create '{}': {e}",
            filepath.display()
        ))
    })?;

    let props = WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .set_key_value_metadata(Some(vec![KeyValue::new(
            PARQUET_SOURCE_METADATA_KEY.to_string(),
            Some(source.to_string()),
        )]))
        .build();

    let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(props)).map_err(|e| {
        OsmError::GraphSerializationError(format!(
            "failed to initialize Parquet writer for '{}': {e}",
            filepath.display()
        ))
    })?;

    writer.write(&batch).map_err(|e| {
        OsmError::GraphSerializationError(format!(
            "failed to write Parquet batch to '{}': {e}",
            filepath.display()
        ))
    })?;

    writer.close().map_err(|e| {
        OsmError::GraphSerializationError(format!(
            "failed to close Parquet writer for '{}': {e}",
            filepath.display()
        ))
    })?;

    Ok(())
}

fn read_records_from_parquet<T>(filepath: &Path) -> Result<(Vec<T>, Option<String>), OsmError>
where
    T: DeserializeOwned,
{
    let file = File::open(filepath).map_err(|e| {
        OsmError::ConfigurationError(format!("failed to open '{}': {e}", filepath.display()))
    })?;

    let builder = ParquetRecordBatchReaderBuilder::try_new(file).map_err(|e| {
        OsmError::ConfigurationError(format!(
            "failed to open Parquet reader for '{}': {e}",
            filepath.display()
        ))
    })?;

    let source = builder
        .metadata()
        .file_metadata()
        .key_value_metadata()
        .and_then(|kvs| {
            kvs.iter()
                .find(|kv| kv.key == PARQUET_SOURCE_METADATA_KEY)
                .and_then(|kv| kv.value.clone())
        });

    let reader = builder.build().map_err(|e| {
        OsmError::ConfigurationError(format!(
            "failed to build Parquet RecordBatch reader for '{}': {e}",
            filepath.display()
        ))
    })?;

    let mut all_records = Vec::new();
    for batch_result in reader {
        let batch = batch_result.map_err(|e| {
            OsmError::ConfigurationError(format!(
                "failed to read RecordBatch from '{}': {e}",
                filepath.display()
            ))
        })?;
        let mut rows: Vec<T> = serde_arrow::from_record_batch(&batch).map_err(|e| {
            OsmError::ConfigurationError(format!(
                "failed to deserialize RecordBatch from '{}': {e}",
                filepath.display()
            ))
        })?;
        all_records.append(&mut rows);
    }

    Ok((all_records, source))
}

/// Topological road segment between two OSM connector nodes, implementing
/// `bambam_network::ImportSegment<Highway>` for split-based graph construction.
#[derive(Debug, Clone)]
pub struct OsmSegmentRecord {
    pub segment_key: String,
    pub parent_way_id: OsmWayId,
    pub parent_start_lr: f64,
    pub parent_end_lr: f64,
    pub src_node_id: OsmNodeId,
    pub dst_node_id: OsmNodeId,
    pub node_sequence: Vec<OsmNodeId>,
    pub way: OsmWayData,
    pub linestring: LineString<f32>,
    pub highway: Highway,
    pub speed_mph: Option<f64>,
}

impl ImportSegment<Highway> for OsmSegmentRecord {
    fn segment_id(&self) -> &str {
        &self.segment_key
    }

    fn get_linestring(&self) -> Result<&LineString<f32>, NetworkImportError> {
        Ok(&self.linestring)
    }

    fn get_split_speed(
        &self,
        _start_lr: f64,
        _end_lr: f64,
        _heading: &SegmentHeading,
    ) -> Result<Option<f64>, NetworkImportError> {
        Ok(self.speed_mph)
    }

    fn get_split_class(
        &self,
        _start_lr: f64,
        _end_lr: f64,
    ) -> Result<Highway, NetworkImportError> {
        Ok(self.highway.clone())
    }

    fn class_to_string(class: &Highway) -> String {
        class.to_string()
    }
}

/// Produces directed `SegmentSplit`s for an `OsmSegmentRecord`, respecting OSM `oneway`
/// and reverse-direction (`oneway=-1` / `reverse`) semantics.
pub fn process_osm_segment_splits(
    segment: &OsmSegmentRecord,
    heading_filter: Option<&SegmentHeading>,
) -> Result<Vec<SegmentSplit>, OsmError> {
    let oneway = segment.way.is_one_way();
    let reverse = segment.way.is_reverse();

    let supported_headings = if oneway && reverse {
        vec![SegmentHeading::Backward]
    } else if oneway {
        vec![SegmentHeading::Forward]
    } else {
        vec![SegmentHeading::Forward, SegmentHeading::Backward]
    };

    let src = ConnectorInSegment::new(
        segment.segment_key.clone(),
        segment.src_node_id.to_string(),
        0.0,
    );
    let dst = ConnectorInSegment::new(
        segment.segment_key.clone(),
        segment.dst_node_id.to_string(),
        1.0,
    );

    let splits = supported_headings
        .into_iter()
        .filter(|h| heading_filter.is_none_or(|filter_h| filter_h == h))
        .map(|heading| SegmentSplit::new(src.clone(), dst.clone(), heading))
        .collect();

    Ok(splits)
}

pub type OsmExtractedNetwork = (
    Vec<OsmConnectorRecord>,
    Vec<OsmConnectorRecord>,
    Vec<OsmSegmentRecord>,
);

/// Extracts topological connectors, infrastructure control points, and `OsmSegmentRecord`s
/// from raw OSM nodes and ways using the OMF-style Connector + Segment workflow:
/// - When `simplify == true`, intermediate degree-2 shape nodes are kept purely as geometry
///   coordinates on `OsmSegmentRecord.linestring` rather than becoming graph vertices.
/// - When `simplify == false`, every way node becomes a connector vertex.
pub fn extract_osm_connectors_and_segments(
    nodes_map: &OsmNodes,
    ways_map: &OsmWays,
    extent_opt: Option<&Geometry<f32>>,
    truncate_by_edge: bool,
    simplify: bool,
    ignore_errors: bool,
) -> Result<OsmExtractedNetwork, OsmError> {
    // 1. Count node occurrences across all accepted ways and identify way endpoints
    let mut node_degree: HashMap<OsmNodeId, usize> = HashMap::new();
    let mut endpoint_nodes: HashSet<OsmNodeId> = HashSet::new();

    let sorted_ways = ways_map
        .values()
        .sorted_by_key(|w| w.osmid)
        .collect_vec();

    for way in &sorted_ways {
        let valid_nodes: Vec<OsmNodeId> = way
            .nodes
            .iter()
            .copied()
            .filter(|nid| nodes_map.contains_key(nid))
            .collect();
        if valid_nodes.len() < 2 {
            continue;
        }
        if let Some(first) = valid_nodes.first() {
            endpoint_nodes.insert(*first);
        }
        if let Some(last) = valid_nodes.last() {
            endpoint_nodes.insert(*last);
        }
        for nid in valid_nodes {
            *node_degree.entry(nid).or_insert(0) += 1;
        }
    }

    // 2. Slice each way at its topological connector nodes
    let mut segments: Vec<OsmSegmentRecord> = Vec::new();
    let mut referenced_connector_ids: HashSet<OsmNodeId> = HashSet::new();

    for way in sorted_ways {
        let valid_nodes: Vec<OsmNodeId> = way
            .nodes
            .iter()
            .copied()
            .filter(|nid| nodes_map.contains_key(nid))
            .collect();
        if valid_nodes.len() < 2 {
            continue;
        }

        let highway = match &way.highway {
            Some(h) => match Highway::from_str(h) {
                Ok(hw) => hw,
                Err(_) if ignore_errors => Highway::Unclassified,
                Err(e) => {
                    return Err(OsmError::InvalidOsmData(format!(
                        "unable to parse highway tag '{h}' on way {}: {e}",
                        way.osmid
                    )))
                }
            },
            None => Highway::Unclassified,
        };

        let speed_mph = way
            .maxspeed
            .as_deref()
            .and_then(|s| {
                osm_way_ops::deserialize_speed(s, Some(OsmWayData::VALUE_DELIMITER), true)
                    .ok()
                    .flatten()
            })
            .map(|vel| vel.get::<uom::si::velocity::mile_per_hour>());

        // Compute cumulative Haversine distances along the way for linear referencing
        let mut cum_dists: Vec<f32> = Vec::with_capacity(valid_nodes.len());
        cum_dists.push(0.0);
        for (u_id, v_id) in valid_nodes.iter().tuple_windows() {
            let u = &nodes_map[u_id];
            let v = &nodes_map[v_id];
            let step_len =
                Haversine.length(&Line::new(Coord::from((u.x, u.y)), Coord::from((v.x, v.y))));
            let prev = *cum_dists.last().unwrap_or(&0.0);
            cum_dists.push(prev + step_len);
        }
        let total_way_dist = *cum_dists.last().unwrap_or(&0.0);
        if total_way_dist <= bambam_network::F32_DISTANCE_TOLERANCE {
            continue;
        }

        // Determine which indices in `valid_nodes` are topological connectors
        let connector_indices: Vec<usize> = (0..valid_nodes.len())
            .filter(|&idx| {
                if idx == 0 || idx + 1 == valid_nodes.len() || !simplify {
                    return true;
                }
                let nid = valid_nodes[idx];
                if endpoint_nodes.contains(&nid)
                    || node_degree.get(&nid).copied().unwrap_or(0) >= 2
                {
                    return true;
                }
                nodes_map
                    .get(&nid)
                    .is_some_and(|n| n.has_control_or_intersection_tag())
            })
            .collect();

        let original_way_id = way.way_ids.first().copied().unwrap_or(way.osmid);

        for (&idx_a, &idx_b) in connector_indices.iter().tuple_windows() {
            let sub_nodes = &valid_nodes[idx_a..=idx_b];
            let sub_coords: Vec<Coord<f32>> = sub_nodes
                .iter()
                .map(|nid| {
                    let n = &nodes_map[nid];
                    Coord::from((n.x, n.y))
                })
                .collect();
            let sub_linestring = LineString::new(sub_coords);
            let sub_len = Haversine.length(&sub_linestring);
            if sub_len <= bambam_network::F32_DISTANCE_TOLERANCE {
                continue;
            }

            // Apply polygon extent filter if present
            if let Some(ext) = extent_opt {
                let keep = if truncate_by_edge {
                    sub_linestring
                        .bounding_rect()
                        .is_some_and(|bbox| ext.intersects(&bbox) && ext.intersects(&sub_linestring))
                } else {
                    sub_nodes
                        .iter()
                        .all(|nid| nodes_map.get(nid).is_some_and(|n| n.intersects(ext)))
                };
                if !keep {
                    continue;
                }
            }

            let src_node_id = valid_nodes[idx_a];
            let dst_node_id = valid_nodes[idx_b];
            let start_lr = (cum_dists[idx_a] / total_way_dist) as f64;
            let end_lr = (cum_dists[idx_b] / total_way_dist) as f64;

            referenced_connector_ids.insert(src_node_id);
            referenced_connector_ids.insert(dst_node_id);

            let mut sub_way = way.clone();
            sub_way.osmid = original_way_id;
            sub_way.nodes = sub_nodes.to_vec();

            let segment_key = format!("{}@{idx_a}..{idx_b}", way.osmid);
            segments.push(OsmSegmentRecord {
                segment_key,
                parent_way_id: original_way_id,
                parent_start_lr: start_lr.clamp(0.0, 1.0),
                parent_end_lr: end_lr.clamp(0.0, 1.0),
                src_node_id,
                dst_node_id,
                node_sequence: sub_nodes.to_vec(),
                way: sub_way,
                linestring: sub_linestring,
                highway: highway.clone(),
                speed_mph,
            });
        }
    }

    let connectors: Vec<OsmConnectorRecord> = referenced_connector_ids
        .iter()
        .sorted()
        .filter_map(|nid| nodes_map.get(nid).cloned().map(OsmConnectorRecord::new))
        .collect();

    let infrastructure: Vec<OsmConnectorRecord> = nodes_map
        .values()
        .filter(|n| {
            n.has_traffic_signals() || n.has_stop() || n.has_give_way() || n.has_crossing()
        })
        .filter(|n| extent_opt.is_none_or(|ext| n.intersects(ext)))
        .sorted_by_key(|n| n.osmid)
        .cloned()
        .map(OsmConnectorRecord::new)
        .collect();

    Ok((connectors, infrastructure, segments))
}

// #[cfg(test)]
// mod tests {
//     use super::reverse_segments;
//     use crate::model::osm::graph::{osm_segment::OsmSegment, AdjacencyList};
//     use std::collections::HashMap;

//     #[test]
//     fn test_rev() {
//         let fwd: AdjacencyList = HashMap::from([
//             (0, HashMap::from([(1, OsmSegment::new(100, None, false))])),
//             (1, HashMap::from([(0, OsmSegment::new(101, None, false))])),
//         ]);
//         let rev = reverse_segments(&fwd);
//         assert_eq!(
//             fwd.get(&0).map(|edges| edges.get(&1).cloned()),
//             rev.get(&1).map(|edges| edges.get(&0).cloned())
//         )
//     }
// }
