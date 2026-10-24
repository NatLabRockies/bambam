use std::{collections::HashMap, hash::Hash};

use geo::{Haversine, Length, LineString};
use routee_compass_core::model::network::{Edge, EdgeId, EdgeListId, Vertex, VertexId};

use crate::{
    consts,
    error::NetworkImportError,
    model::{ConnectorInSegment, ImportSegment, SegmentHeading},
    vertex_serializable::EdgeMappingRecord,
};

/// splits at the connectors (vertices) ignoring linear-referenced split points
/// for other attributes such as speed. does not require creating additional vertices.
#[derive(Debug, Clone)]
pub struct SegmentSplit {
    pub src: ConnectorInSegment,
    pub dst: ConnectorInSegment,
    pub heading: SegmentHeading,
}

impl SegmentSplit {
    /// constructs a new simple segment split based purely on the linear references between
    /// connectors along with any heading information relevant to the active travel mode.
    pub fn new(src: ConnectorInSegment, dst: ConnectorInSegment, heading: SegmentHeading) -> Self {
        Self { src, dst, heading }
    }

    /// creates an `Edge` from this split.
    pub fn create_edge_from_split<S, C>(
        &self,
        edge_id: EdgeId,
        edge_list_id: EdgeListId,
        segments: &[&S],
        segment_lookup: &HashMap<String, usize>,
        _vertices: &[Vertex],
        vertex_lookup: &HashMap<String, usize>,
    ) -> Result<Edge, NetworkImportError>
    where
        C: Clone + Eq + Hash + Send + Sync,
        S: ImportSegment<C>,
    {
        use NetworkImportError as E;

        let SegmentSplit { src, dst, heading } = self;

        if src.segment_id != dst.segment_id {
            let msg = format!(
                "attempting to create edge where src segment != dst segment ('{}' != '{}')",
                src.segment_id, dst.segment_id
            );
            return Err(E::InvalidSegmentConnectors(msg));
        }

        let src_id = vertex_lookup
            .get(&src.connector_id)
            .ok_or_else(|| E::InvalidSegmentConnectors(format!("segment references unknown connector {}", src.connector_id)))?;

        let dst_id = vertex_lookup
            .get(&dst.connector_id)
            .ok_or_else(|| E::InvalidSegmentConnectors(format!("segment references unknown connector {}", dst.connector_id)))?;

        let (src_vertex_id, dst_vertex_id) = match heading {
            SegmentHeading::Forward => (VertexId(*src_id), VertexId(*dst_id)),
            SegmentHeading::Backward => (VertexId(*dst_id), VertexId(*src_id)),
        };

        if dst.linear_reference < src.linear_reference {
            return Err(E::InvalidSegmentConnectors(format!(
                "SimpleConnectorSplit: at_dst ({}) < at_src ({}) for connectors {} -> {}",
                dst.linear_reference, src.linear_reference, src.connector_id, dst.connector_id
            )));
        }

        let segment = self.get_segment(segments, segment_lookup)?;
        let dst_distance = segment.get_distance_at_meters(dst.linear_reference.0)?;
        let src_distance = segment.get_distance_at_meters(src.linear_reference.0)?;
        let distance_f32 = dst_distance - src_distance;
        let distance = uom::si::length::Length::new::<uom::si::length::meter>(distance_f32 as f64);

        Ok(Edge {
            edge_list_id,
            edge_id,
            src_vertex_id,
            dst_vertex_id,
            distance,
        })
    }

    /// extracts the LineString geometry corresponding to this split based on linear reference.
    pub fn create_geometry_from_split<S, C>(
        &self,
        segments: &[&S],
        segment_lookup: &HashMap<String, usize>,
    ) -> Result<LineString<f32>, NetworkImportError>
    where
        C: Clone + Eq + Hash + Send + Sync,
        S: ImportSegment<C>,
    {
        let SegmentSplit { src, dst, heading } = self;
        let segment = self.get_segment(segments, segment_lookup)?;

        let distance_to_src = segment.get_distance_at_meters(src.linear_reference.0)?;
        let distance_to_dst = segment.get_distance_at_meters(dst.linear_reference.0)?;
        let segment_geometry = segment.get_linestring()?;

        let mut out_coords = vec![];
        out_coords.push(segment.get_coord_at(src.linear_reference.0)?);

        let mut total_distance = 0.;
        for line in segment_geometry.lines() {
            let line_distance = Haversine.length(&line);
            total_distance += line_distance;

            if total_distance <= distance_to_src + consts::F32_DISTANCE_TOLERANCE {
                continue;
            }
            if total_distance + consts::F32_DISTANCE_TOLERANCE >= distance_to_dst {
                break;
            }

            out_coords.push(line.end);
        }

        out_coords.push(segment.get_coord_at(dst.linear_reference.0)?);

        if *heading == SegmentHeading::Backward {
            out_coords.reverse();
        }
        Ok(LineString::new(out_coords))
    }

    pub fn get_split_speed<S, C>(
        &self,
        segments: &[&S],
        segment_lookup: &HashMap<String, usize>,
    ) -> Result<Option<f64>, NetworkImportError>
    where
        C: Clone + Eq + Hash + Send + Sync,
        S: ImportSegment<C>,
    {
        let segment = self.get_segment(segments, segment_lookup)?;
        segment.get_split_speed(
            self.src.linear_reference.0,
            self.dst.linear_reference.0,
            &self.heading,
        )
    }

    pub fn get_split_segment_full_type<S, C>(
        &self,
        segments: &[&S],
        segment_lookup: &HashMap<String, usize>,
    ) -> Result<C, NetworkImportError>
    where
        C: Clone + Eq + Hash + Send + Sync,
        S: ImportSegment<C>,
    {
        let segment = self.get_segment(segments, segment_lookup)?;
        segment.get_split_class(self.src.linear_reference.0, self.dst.linear_reference.0)
    }

    pub fn get_split_length_meters<S, C>(
        &self,
        segments: &[&S],
        segment_lookup: &HashMap<String, usize>,
    ) -> Result<f32, NetworkImportError>
    where
        C: Clone + Eq + Hash + Send + Sync,
        S: ImportSegment<C>,
    {
        let segment = self.get_segment(segments, segment_lookup)?;
        let start = self.src.linear_reference.0;
        let end = self.dst.linear_reference.0;
        Ok(segment.get_distance_at_meters(end)? - segment.get_distance_at_meters(start)?)
    }

    pub fn get_edge_mapping<S, C>(
        &self,
        edge_id: usize,
        segments: &[&S],
        segment_lookup: &HashMap<String, usize>,
    ) -> Result<EdgeMappingRecord, NetworkImportError>
    where
        C: Clone + Eq + Hash + Send + Sync,
        S: ImportSegment<C>,
    {
        let segment = self.get_segment(segments, segment_lookup)?;
        Ok(EdgeMappingRecord {
            edge_id,
            source_id: segment.segment_id().to_string(),
            start_lr: self.src.linear_reference.0,
            end_lr: self.dst.linear_reference.0,
            heading: self.heading.clone(),
        })
    }

    fn get_segment<'a, S, C>(
        &self,
        segments: &'a [&S],
        segment_lookup: &HashMap<String, usize>,
    ) -> Result<&'a S, NetworkImportError>
    where
        C: Clone + Eq + Hash + Send + Sync,
        S: ImportSegment<C>,
    {
        use NetworkImportError as E;
        let segment_id = &self.src.segment_id;
        let segment_idx = segment_lookup.get(segment_id).ok_or_else(|| {
            let msg = format!("missing lookup entry for segment {segment_id}");
            E::InvalidSegmentConnectors(msg)
        })?;
        Ok(*segments.get(*segment_idx).ok_or_else(|| {
            let msg =
                format!("missing lookup entry for segment {segment_id} with index {segment_idx}");
            E::InvalidSegmentConnectors(msg)
        })?)
    }
}
