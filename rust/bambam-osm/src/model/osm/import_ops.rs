use super::{
    graph::{
        osm_element_filter::ElementFilter, osm_node_data::OsmConnectorRecord, osm_way_ops,
        OsmNodeId, OsmNodes, OsmWayId, OsmWays,
    },
    OsmError,
};
use crate::{
    algorithm::buffer,
    model::{
        feature::highway::Highway,
        osm::graph::{osm_node_data::OsmNodeData, osm_way_data::OsmWayData},
    },
};
use arrow::datatypes::FieldRef;
use bambam_network::{
    evaluate_segment_headings, ConnectorInSegment, FilterableSegment, ImportSegment,
    NetworkImportError, SegmentAccessRestriction, SegmentAccessRestrictionWhen, SegmentClass,
    SegmentFullType, SegmentHeading, SegmentSplit, SegmentSubclass, SegmentSubtype,
    DEFAULT_INFRASTRUCTURE_MATCH_DISTANCE_METERS,
};
use geo::{BoundingRect, Coord, Geometry, Haversine, Intersects, Length, Line, LineString};
use itertools::Itertools;
use kdam::{term, Bar, BarExt};
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

        write_records_to_parquet(&self.nodes, &raw_dir.join(RAW_NODES_PARQUET), &self.source)?;
        write_records_to_parquet(&self.ways, &raw_dir.join(RAW_WAYS_PARQUET), &self.source)?;
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

        let (nodes, n_src) =
            read_records_from_parquet::<OsmNodeData>(&raw_dir.join(RAW_NODES_PARQUET))?;
        let (ways, w_src) =
            read_records_from_parquet::<OsmWayData>(&raw_dir.join(RAW_WAYS_PARQUET))?;
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

fn write_records_to_parquet<T>(records: &[T], filepath: &Path, source: &str) -> Result<(), OsmError>
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
        OsmError::GraphSerializationError(format!("failed to create '{}': {e}", filepath.display()))
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
/// `bambam_network::ImportSegment<SegmentFullType>` and `bambam_network::FilterableSegment`
/// for split-based graph construction.
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
    pub full_type: SegmentFullType,
    pub access_restrictions: Vec<SegmentAccessRestriction>,
    pub speed_mph: Option<f64>,
}

impl FilterableSegment for OsmSegmentRecord {
    fn segment_subtype(&self) -> Option<&SegmentSubtype> {
        Some(&self.full_type.0)
    }

    fn segment_class(&self) -> Option<&SegmentClass> {
        Some(&self.full_type.1)
    }

    fn segment_subclass(&self) -> Option<&SegmentSubclass> {
        self.full_type.2.as_ref()
    }

    fn segment_access_restrictions(&self) -> Option<&[SegmentAccessRestriction]> {
        if self.access_restrictions.is_empty() {
            None
        } else {
            Some(&self.access_restrictions)
        }
    }
}

impl ImportSegment<SegmentFullType> for OsmSegmentRecord {
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
    ) -> Result<SegmentFullType, NetworkImportError> {
        Ok(self.full_type.clone())
    }

    fn class_to_string(class: &SegmentFullType) -> String {
        class.as_str()
    }
}

/// Produces directed `SegmentSplit`s for an `OsmSegmentRecord`, evaluating mode-aware
/// directional access restrictions via `bambam_network::evaluate_segment_headings`.
pub fn process_osm_segment_splits(
    segment: &OsmSegmentRecord,
    when: Option<&SegmentAccessRestrictionWhen>,
) -> Result<Vec<SegmentSplit>, OsmError> {
    let restrictions = if segment.access_restrictions.is_empty() {
        None
    } else {
        Some(segment.access_restrictions.as_slice())
    };
    let supported_headings = evaluate_segment_headings(restrictions, when);

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
/// - When `simplify == true`, ways are split strictly at topological intersections (`node_degree >= 2`)
///   and way endpoints. Intermediate degree-1 shape nodes are kept purely as geometry coordinates on
///   `OsmSegmentRecord.linestring`.
/// - Any intermediate control nodes (`traffic_signals`, `stop`, `give_way`, `crossing`) on a way segment
///   within `DEFAULT_INFRASTRUCTURE_MATCH_DISTANCE_METERS` along the way are propagated directly to the
///   nearer endpoint connector of that exact way segment, ensuring traffic controls never jump onto
///   adjacent sidewalks or alleys.
/// - Only detached (`node_degree == 0`) control nodes are returned in the `infrastructure` vector for
///   spatial `rstar` attachment.
pub fn extract_osm_connectors_and_segments(
    nodes_map: &OsmNodes,
    ways_map: &OsmWays,
    extent_opt: Option<&Geometry<f32>>,
    truncate_by_edge: bool,
    simplify: bool,
    ignore_errors: bool,
) -> Result<OsmExtractedNetwork, OsmError> {
    extract_osm_connectors_and_segments_with_match_distance(
        nodes_map,
        ways_map,
        extent_opt,
        truncate_by_edge,
        simplify,
        ignore_errors,
        None,
    )
}

/// Extracts topological connectors, infrastructure control points, and `OsmSegmentRecord`s
/// with a configurable along-way matching distance for in-way traffic controls.
pub fn extract_osm_connectors_and_segments_with_match_distance(
    nodes_map: &OsmNodes,
    ways_map: &OsmWays,
    extent_opt: Option<&Geometry<f32>>,
    truncate_by_edge: bool,
    simplify: bool,
    ignore_errors: bool,
    infrastructure_match_distance_meters: Option<f64>,
) -> Result<OsmExtractedNetwork, OsmError> {
    let match_dist_m = infrastructure_match_distance_meters
        .unwrap_or(DEFAULT_INFRASTRUCTURE_MATCH_DISTANCE_METERS) as f32;

    // 1. Count node occurrences across all accepted ways and identify way endpoints
    let mut node_degree: HashMap<OsmNodeId, usize> = HashMap::new();
    let mut endpoint_nodes: HashSet<OsmNodeId> = HashSet::new();

    let sorted_ways = ways_map.values().sorted_by_key(|w| w.osmid).collect_vec();

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

    // 2. Slice each way at its topological connector nodes and propagate in-way controls
    let mut segments: Vec<OsmSegmentRecord> = Vec::new();
    let mut referenced_connector_ids: HashSet<OsmNodeId> = HashSet::new();
    // Propagated control attributes & source node IDs keyed by connector OsmNodeId
    let mut propagated_highway_tags: HashMap<OsmNodeId, Vec<String>> = HashMap::new();
    let mut propagated_control_ids: HashMap<OsmNodeId, Vec<OsmNodeId>> = HashMap::new();

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

        let full_type = way.to_segment_full_type(&highway);
        let access_restrictions = way.to_access_restrictions();

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
        // (strictly way endpoints or topological intersections where node_degree >= 2)
        let connector_indices: Vec<usize> = (0..valid_nodes.len())
            .filter(|&idx| {
                if idx == 0 || idx + 1 == valid_nodes.len() || !simplify {
                    return true;
                }
                let nid = valid_nodes[idx];
                endpoint_nodes.contains(&nid) || node_degree.get(&nid).copied().unwrap_or(0) >= 2
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
                    sub_linestring.bounding_rect().is_some_and(|bbox| {
                        ext.intersects(&bbox) && ext.intersects(&sub_linestring)
                    })
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

            // Propagate any intermediate control nodes (idx_a < k < idx_b) along this way
            // to the nearer topological endpoint connector (src_node_id or dst_node_id)
            // within `match_dist_m`.
            for k in (idx_a + 1)..idx_b {
                let mid_nid = valid_nodes[k];
                if let Some(mid_node) = nodes_map.get(&mid_nid) {
                    if mid_node.has_traffic_signals()
                        || mid_node.has_stop()
                        || mid_node.has_give_way()
                        || mid_node.has_crossing()
                    {
                        let d_src = cum_dists[k] - cum_dists[idx_a];
                        let d_dst = cum_dists[idx_b] - cum_dists[k];
                        let min_d = d_src.min(d_dst);
                        if min_d <= match_dist_m {
                            let target_nid = if d_src <= d_dst {
                                src_node_id
                            } else {
                                dst_node_id
                            };
                            if let Some(hw_tag) = &mid_node.highway {
                                let entry =
                                    propagated_highway_tags.entry(target_nid).or_default();
                                if !entry.contains(hw_tag) {
                                    entry.push(hw_tag.clone());
                                }
                            }
                            let id_entry = propagated_control_ids.entry(target_nid).or_default();
                            if !id_entry.contains(&mid_nid) {
                                id_entry.push(mid_nid);
                            }
                        }
                    }
                }
            }

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
                full_type: full_type.clone(),
                access_restrictions: access_restrictions.clone(),
                speed_mph,
            });
        }
    }

    let connectors: Vec<OsmConnectorRecord> = referenced_connector_ids
        .iter()
        .sorted()
        .filter_map(|nid| {
            let mut node = nodes_map.get(nid)?.clone();
            if let Some(extra_tags) = propagated_highway_tags.get(nid) {
                let mut combined: Vec<String> = node
                    .highway
                    .iter()
                    .cloned()
                    .chain(extra_tags.iter().cloned())
                    .unique()
                    .collect();
                if !combined.is_empty() {
                    node.highway = Some(combined.drain(..).join(OsmNodeData::VALUE_DELIMITER));
                }
            }
            if let Some(extra_ids) = propagated_control_ids.get(nid) {
                for id in extra_ids {
                    if !node.consolidated_ids.contains(id) {
                        node.consolidated_ids.push(*id);
                    }
                }
            }
            Some(OsmConnectorRecord::new(node))
        })
        .collect();

    // Only detached (`node_degree == 0`) control nodes use spatial R-tree snapping;
    // in-way control nodes were already topologically propagated along their way.
    let infrastructure: Vec<OsmConnectorRecord> = nodes_map
        .values()
        .filter(|n| node_degree.get(&n.osmid).copied().unwrap_or(0) == 0)
        .filter(|n| n.has_traffic_signals() || n.has_stop() || n.has_give_way() || n.has_crossing())
        .filter(|n| extent_opt.is_none_or(|ext| n.intersects(ext)))
        .sorted_by_key(|n| n.osmid)
        .cloned()
        .map(OsmConnectorRecord::new)
        .collect();

    Ok((connectors, infrastructure, segments))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::osm::graph::{OsmNodeData, OsmNodeId, OsmWayData, OsmWayId};

    #[test]
    fn test_osm_along_way_signal_propagation_and_taxonomy() {
        // Way with 3 nodes: u (0,0) -> mid (0.0001, 0) (~11m east, tagged traffic_signals) -> v (0.001, 0) (~111m east)
        // Plus a nearby sidewalk node s (0.0001, 0.00003) (~3m north of mid) on a separate sidewalk way.
        // Because mid is an in-way degree-1 node on the road way within 15m of u, its traffic_signals
        // tag must propagate along the road way to u (and NOT split the road way, nor snap to the sidewalk!).
        let mut nodes_map = HashMap::new();
        nodes_map.insert(
            OsmNodeId(1),
            OsmNodeData {
                osmid: OsmNodeId(1),
                x: -105.0000,
                y: 39.7500,
                ..Default::default()
            },
        );
        nodes_map.insert(
            OsmNodeId(2),
            OsmNodeData {
                osmid: OsmNodeId(2),
                x: -104.9999,
                y: 39.7500,
                highway: Some("traffic_signals".to_string()),
                ..Default::default()
            },
        );
        nodes_map.insert(
            OsmNodeId(3),
            OsmNodeData {
                osmid: OsmNodeId(3),
                x: -104.9980,
                y: 39.7500,
                ..Default::default()
            },
        );
        nodes_map.insert(
            OsmNodeId(10),
            OsmNodeData {
                osmid: OsmNodeId(10),
                x: -104.9999,
                y: 39.75003,
                ..Default::default()
            },
        );
        nodes_map.insert(
            OsmNodeId(11),
            OsmNodeData {
                osmid: OsmNodeId(11),
                x: -104.9980,
                y: 39.75003,
                ..Default::default()
            },
        );

        let mut ways_map = HashMap::new();
        ways_map.insert(
            OsmWayId(100),
            OsmWayData {
                osmid: OsmWayId(100),
                nodes: vec![OsmNodeId(1), OsmNodeId(2), OsmNodeId(3)],
                highway: Some("primary_link".to_string()),
                oneway: Some("yes".to_string()),
                ..Default::default()
            },
        );
        ways_map.insert(
            OsmWayId(200),
            OsmWayData {
                osmid: OsmWayId(200),
                nodes: vec![OsmNodeId(10), OsmNodeId(11)],
                highway: Some("footway".to_string()),
                footway: Some("sidewalk".to_string()),
                ..Default::default()
            },
        );

        let (connectors, infrastructure, segments) =
            extract_osm_connectors_and_segments_with_match_distance(
                &nodes_map,
                &ways_map,
                None,
                true,
                true,
                true,
                Some(15.0),
            )
            .unwrap();

        // Only the 4 way endpoints (1, 3, 10, 11) should be connectors; node 2 is NOT a split point
        assert_eq!(connectors.len(), 4);
        assert!(infrastructure.is_empty());
        assert_eq!(segments.len(), 2);

        // Node 1 received the propagated traffic_signals tag and consolidated_id 2; node 10 did not
        let conn1 = connectors.iter().find(|c| c.node.osmid == OsmNodeId(1)).unwrap();
        let conn10 = connectors.iter().find(|c| c.node.osmid == OsmNodeId(10)).unwrap();
        assert!(conn1.node.has_traffic_signals());
        assert!(conn1.node.consolidated_ids.contains(&OsmNodeId(2)));
        assert!(!conn10.node.has_traffic_signals());

        // Check SegmentFullType mapping: `road-primary-link` and `road-footway-sidewalk`
        let seg_road = segments.iter().find(|s| s.parent_way_id == OsmWayId(100)).unwrap();
        let seg_sw = segments.iter().find(|s| s.parent_way_id == OsmWayId(200)).unwrap();
        assert_eq!(seg_road.full_type.as_str(), "road-primary-link");
        assert_eq!(seg_sw.full_type.as_str(), "road-footway-sidewalk");
    }
}
