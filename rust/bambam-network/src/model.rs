use geo::{Coord, Haversine, InterpolatableLine, Length, LineString, Point};
use ordered_float::OrderedFloat;
use routee_compass_core::model::network::Vertex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::NetworkImportError;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SegmentHeading {
    Forward,
    Backward,
}

/// represents a connector found within a segment
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConnectorInSegment {
    pub segment_id: String,
    pub connector_id: String,
    pub linear_reference: OrderedFloat<f64>,
}

impl ConnectorInSegment {
    /// records an existing connector record as a connector within a segment.
    pub fn new(segment_id: String, connector_id: String, linear_reference: f64) -> Self {
        Self {
            segment_id,
            connector_id,
            linear_reference: OrderedFloat(linear_reference),
        }
    }

    /// creates a new connector within a segment by concatenating its segment id and reference.
    pub fn new_without_connector_id(segment_id: String, linear_reference: f64) -> Self {
        let connector_id = format!("{segment_id}@{linear_reference}");
        Self {
            segment_id,
            connector_id,
            linear_reference: OrderedFloat(linear_reference),
        }
    }
}

/// Trait implemented by connector/node records (e.g., OMF Connector or OSM intersection Node)
/// that become vertices in a RouteE Compass network.
pub trait ImportConnector: Sync {
    fn connector_id(&self) -> &str;
    fn try_to_vertex(&self, idx: usize) -> Result<Vertex, NetworkImportError>;

    /// Optional intrinsic traffic signal flag on the connector itself (e.g. from an OSM node tag).
    fn has_traffic_signals(&self) -> bool {
        false
    }
    /// Optional intrinsic stop sign flag on the connector itself.
    fn has_stop(&self) -> bool {
        false
    }
    /// Optional intrinsic give-way/yield flag on the connector itself.
    fn has_give_way(&self) -> bool {
        false
    }
    /// Optional intrinsic pedestrian crossing flag on the connector itself.
    fn has_crossing(&self) -> bool {
        false
    }
    /// Optional list of consolidated infrastructure IDs propagated onto this connector.
    fn consolidated_infrastructure_ids(&self) -> Option<Vec<String>> {
        None
    }
}

/// Trait implemented by point/polygon infrastructure records (e.g., OMF Infrastructure or
/// standalone OSM control features) that can be spatially attached to nearby vertices.
pub trait ImportInfrastructure: Sync {
    fn infrastructure_id(&self) -> &str;
    fn reference_point(&self) -> Option<Point<f32>>;
    fn is_traffic_signals(&self) -> bool;
    fn is_stop(&self) -> bool;
    fn is_give_way(&self) -> bool;
    fn is_crossing(&self) -> bool;
}

/// Trait implemented by linear transportation segments (e.g., OMF Segment or OSM Way)
/// to support split-based graph construction.
pub trait ImportSegment<C = String>: Sync
where
    C: Clone + Eq + std::hash::Hash + Send + Sync,
{
    fn segment_id(&self) -> &str;
    fn get_linestring(&self) -> Result<&LineString<f32>, NetworkImportError>;
    fn get_split_speed(
        &self,
        start_lr: f64,
        end_lr: f64,
        heading: &SegmentHeading,
    ) -> Result<Option<f64>, NetworkImportError>;
    fn get_split_class(&self, start_lr: f64, end_lr: f64) -> Result<C, NetworkImportError>;
    fn class_to_string(class: &C) -> String;

    fn get_distance_at_meters(&self, at: f64) -> Result<f32, NetworkImportError> {
        if !(0.0..=1.0).contains(&at) {
            return Err(NetworkImportError::InvalidLinearReference(at));
        }
        let linestring = self.get_linestring()?;
        Ok(Haversine.length(linestring) * at as f32)
    }

    fn get_coord_at(&self, at: f64) -> Result<Coord<f32>, NetworkImportError> {
        if !(0.0..=1.0).contains(&at) {
            return Err(NetworkImportError::InvalidLinearReference(at));
        }
        let linestring = self.get_linestring()?;
        match linestring.point_at_ratio_from_start(&Haversine, at as f32) {
            Some(pt) => Ok(pt.0),
            None => Err(NetworkImportError::InternalError(format!(
                "unexpected error getting point for segment {} at {at}",
                self.segment_id()
            ))),
        }
    }
}
