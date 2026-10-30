use super::osm_way_ops::{self, deserialize_linestring, serialize_linestring};
use super::{OsmNodeId, OsmWayData, OsmWayId};
use crate::model::{feature::highway::Highway, osm::OsmError};
use bambam_modal_metrics::common::cycleway_tag::CyclewayTag;
use bambam_modal_metrics::network_traits::edge_for_modal_metric::EdgeForModalMetric;
use bambam_modal_metrics::network_traits::spatial_edge::SpatialEdge;
use geo::LineString;
use itertools::Itertools;
use routee_compass_core::model::network::VertexId;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OsmWayDataSerializable {
    pub osmid: OsmWayId,
    pub src_vertex_id: VertexId,
    pub dst_vertex_id: VertexId,
    pub nodes: Option<String>,
    pub access: Option<String>,
    pub area: Option<String>,
    pub bridge: Option<String>,
    pub est_width: Option<String>,
    pub highway: Highway,
    pub sidewalk: Option<String>,
    pub cycleway: Option<String>,
    pub footway: Option<String>,
    pub junction: Option<String>,
    pub landuse: Option<String>,
    pub lanes: Option<String>,
    pub maxspeed: Option<String>,
    pub maxspeed_raw: Option<String>,
    pub name: Option<String>,
    pub oneway: Option<String>,
    pub _ref: Option<String>,
    pub service: Option<String>,
    pub tunnel: Option<String>,
    pub width: Option<String>,
    /// when ways are simplified, the list of composite way ids are stored here.
    /// the Way.osmid will remain present in any aggregate way_ids collection.
    pub way_ids: Option<String>,
    #[serde(
        serialize_with = "serialize_linestring",
        deserialize_with = "deserialize_linestring"
    )]
    pub linestring: LineString<f32>,
    pub length_meters: f64,
}

impl OsmWayDataSerializable {
    /// a delimter for aggregated fields which does not collide with CSV delimiters
    pub const VALUE_DELIMITER: &'static str = ";";

    /// Constructs a serializable OSM way record directly from a split sub-segment,
    /// its resolved source/destination `VertexId`s, and its `LineString<f32>` geometry.
    pub fn from_split_way(
        way: &OsmWayData,
        node_sequence: &[OsmNodeId],
        src_vertex_id: VertexId,
        dst_vertex_id: VertexId,
        linestring: LineString<f32>,
        length_meters: f64,
    ) -> Result<Self, OsmError> {
        let highway = top_highway(&way.highway, OsmWayData::VALUE_DELIMITER)?;
        Ok(Self {
            osmid: way.osmid,
            src_vertex_id,
            dst_vertex_id,
            highway,
            linestring,
            length_meters,
            area: max(way.area.as_ref()),
            est_width: min(way.est_width.as_ref()),
            lanes: min(way.lanes.as_ref()),
            maxspeed: min(way.maxspeed.as_ref()),
            width: min(way.width.as_ref()),
            _ref: unique(way._ref.as_ref()),
            access: unique(way.access.as_ref()),
            bridge: unique(way.bridge.as_ref()),
            cycleway: unique(way.cycleway.as_ref()),
            footway: unique(way.footway.as_ref()),
            junction: unique(way.junction.as_ref()),
            landuse: unique(way.landuse.as_ref()),
            maxspeed_raw: replace_delimiter(way.maxspeed.as_ref()),
            name: unique(way.name.as_ref()),
            nodes: join_node_ids(node_sequence),
            oneway: unique(way.oneway.as_ref()),
            sidewalk: unique(way.sidewalk.as_ref()),
            service: unique(way.service.as_ref()),
            tunnel: unique(way.tunnel.as_ref()),
            way_ids: join_way_ids(way.way_ids.as_slice()),
        })
    }

    pub fn get_string_at_field(&self, fieldname: &str) -> Result<Option<String>, String> {
        match fieldname {
            "access" => Ok(self.access.clone()),
            "area" => Ok(self.area.clone()),
            "bridge" => Ok(self.bridge.clone()),
            "est_width" => Ok(self.est_width.clone()),
            "highway" => Ok(Some(self.highway.to_string())),
            "sidewalk" => Ok(self.sidewalk.clone()),
            "cycleway" => Ok(self.cycleway.clone()),
            "footway" => Ok(self.footway.clone()),
            "junction" => Ok(self.junction.clone()),
            "landuse" => Ok(self.landuse.clone()),
            "lanes" => Ok(self.lanes.clone()),
            "maxspeed" => Ok(self.maxspeed.clone()),
            "maxspeed_raw" => Ok(self.maxspeed_raw.clone()),
            "name" => Ok(self.name.clone()),
            "oneway" => Ok(self.oneway.clone()),
            "ref" => Ok(self._ref.clone()),
            "service" => Ok(self.service.clone()),
            "tunnel" => Ok(self.tunnel.clone()),
            "width" => Ok(self.width.clone()),
            _ => Err(format!("unknown edge field {fieldname}")),
        }
    }

    /// follows the rules described in
    /// https://wiki.openstreetmap.org/wiki/Key:maxspeed#Values
    pub fn get_speed(
        &self,
        key: &str,
        ignore_invalid_entries: bool,
    ) -> Result<Option<uom::si::f64::Velocity>, String> {
        match self.get_string_at_field(key) {
            Ok(None) => Ok(None),
            Ok(Some(s)) => osm_way_ops::deserialize_speed(
                &s,
                Some(Self::VALUE_DELIMITER),
                ignore_invalid_entries,
            ),
            Err(e) => Err(e),
        }
    }
}

/// OSM Way modal metric implementation
impl EdgeForModalMetric for OsmWayDataSerializable {
    fn get_traffic_speed_limit(&self) -> Option<i32> {
        // TODO: since the US speed limits are in MPH, we grab the maxspeed_raw because it is typically
        // of the form "XX mph". We should ideally try to grab either "maxspeed" or "maxspeed_raw"
        // so that this will work for Non-US speed limits.
        match self.get_speed("maxspeed_raw", true) {
            Ok(Some(velocity)) => {
                let mph = velocity.get::<uom::si::velocity::mile_per_hour>();
                Some(mph.round() as i32)
            }
            _ => None,
        }
    }

    fn get_cycleway_tag(&self) -> Option<CyclewayTag> {
        if self.highway == Highway::Cycleway {
            return Some(CyclewayTag::DedicatedWithBuffer);
        }
        self.cycleway.as_ref().map(|tag| CyclewayTag::new(tag))
    }

    fn is_walkable(&self) -> bool {
        let is_sidewalk = self.is_sidewalk();

        let is_footway = self.is_footway();

        let is_walkable_highway = self.is_walkable_highway();

        is_sidewalk || is_footway || is_walkable_highway
    }

    fn is_walkable_highway(&self) -> bool {
        matches!(
            self.highway,
            Highway::Residential
                | Highway::Unclassified
                | Highway::LivingStreet
                | Highway::Service
                | Highway::Pedestrian
                | Highway::Trailhead
                | Highway::Track
                | Highway::Footway
                | Highway::Bridleway
                | Highway::Steps
                | Highway::Corridor
                | Highway::Path
                | Highway::Cycleway
                | Highway::Elevator
        )
    }

    fn is_sidewalk(&self) -> bool {
        self.sidewalk
            .as_ref()
            .is_some_and(|s| s != "no" && s != "none")
            || self.footway == Some("sidewalk".to_string())
            || self.highway == Highway::Sidewalk
    }

    fn is_footway(&self) -> bool {
        self.footway
            .as_ref()
            .is_some_and(|s| s != "no" && s != "none")
            || self.highway == Highway::Footway
    }

    fn is_pedestrian_priority(&self) -> bool {
        matches!(self.highway, Highway::LivingStreet | Highway::Pedestrian)
    }

    fn is_unbikeable(&self) -> bool {
        matches!(
            self.highway,
            Highway::Motorway | Highway::Trunk | Highway::MotorwayLink | Highway::TrunkLink
        )
    }

    fn is_non_motorized(&self) -> bool {
        matches!(
            self.highway,
            Highway::Cycleway
                | Highway::Path
                | Highway::Footway
                | Highway::Pedestrian
                | Highway::LivingStreet
        )
    }

    fn is_oneway(&self) -> bool {
        self.oneway.as_deref() == Some("yes")
    }
}

impl SpatialEdge for OsmWayDataSerializable {
    fn id(&self) -> String {
        self.osmid.to_string()
    }

    fn linestring(&self) -> Option<&LineString<f32>> {
        Some(&self.linestring)
    }

    fn src_vertex_id(&self) -> usize {
        self.src_vertex_id.0
    }
}
/// shorten the value, assumed a delimited string of categoricals, so that
/// it contains only the unique set of categories.
fn unique(value: Option<&String>) -> Option<String> {
    let split = value.map(|v| v.split(OsmWayData::VALUE_DELIMITER).collect_vec());

    match split {
        None => None,
        Some(values) if values.is_empty() => None,
        Some(mut nonempty_values) => {
            nonempty_values.dedup();
            let result = nonempty_values.join(OsmWayDataSerializable::VALUE_DELIMITER);
            Some(result)
        }
    }
}

/// take the minimum numerical value present in the input rows, with the following algorithm:
///   - if numbers can be all integers, find the min
///   - else if numbers can be all floating point, find the min
///   - else return whatever value already is with delimiter replaced for CSV
fn min(value: Option<&String>) -> Option<String> {
    let split = value
        .map(|v| v.split(OsmWayData::VALUE_DELIMITER).collect_vec())
        .unwrap_or_default();
    let parsed_min_i64 = as_parsed(&split, |vs: &[i64]| vs.iter().cloned().min());
    parsed_min_i64.or_else(|| {
        as_parsed(&split, |vs: &[f64]| {
            vs.iter()
                .cloned()
                .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        })
    })
}

/// take the maximum numerical value present in the input rows, with the following algorithm:
///   - if numbers can be all integers, find the max
///   - else if numbers can be all floating point, find the max
///   - else return whatever value already is with delimiter replaced for CSV
fn max(value: Option<&String>) -> Option<String> {
    let split = value
        .map(|v| v.split(OsmWayData::VALUE_DELIMITER).collect_vec())
        .unwrap_or_default();
    let parsed_max_i64 = as_parsed(&split, |vs: &[i64]| vs.iter().cloned().max());
    parsed_max_i64.or_else(|| {
        as_parsed(&split, |vs: &[f64]| {
            vs.iter()
                .cloned()
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        })
    })
}

/// parses the values as a list of T and aggregates over those T to produce a single value
/// which is then serialized again to string.
fn as_parsed<T, F>(values: &[&str], agg: F) -> Option<String>
where
    T: FromStr + std::fmt::Display,
    F: FnOnce(&[T]) -> Option<T>,
{
    let as_t = values
        .iter()
        .map(|s| s.parse::<T>())
        .collect::<Result<Vec<_>, _>>();
    match as_t {
        Ok(ts) => {
            let agg_result = agg(&ts);
            agg_result.map(|agg_t| format!("{agg_t}"))
        }
        Err(_) => None,
    }
}

fn replace_delimiter(value: Option<&String>) -> Option<String> {
    value.as_ref().map(|v| {
        v.replace(
            OsmWayData::VALUE_DELIMITER,
            OsmWayDataSerializable::VALUE_DELIMITER,
        )
    })
}

fn join_node_ids(value: &[OsmNodeId]) -> Option<String> {
    match value[..] {
        [] => None,
        _ => {
            let joined = value
                .iter()
                .map(|id| format!("{id}"))
                .join(OsmWayDataSerializable::VALUE_DELIMITER);
            Some(joined)
        }
    }
}

fn join_way_ids(value: &[OsmWayId]) -> Option<String> {
    match value[..] {
        [] => None,
        _ => {
            let joined = value
                .iter()
                .map(|id| format!("{id}"))
                .join(OsmWayDataSerializable::VALUE_DELIMITER);
            Some(joined)
        }
    }
}

/// if the highway value is non-empty, split it by the expected delimiter and take the top-ranked Highway
/// tag by it's Highway::hierarchy().
fn top_highway(
    highway_value: &Option<String>,
    delimiter: &'static str,
) -> Result<Highway, OsmError> {
    match highway_value {
        None => Err(OsmError::InternalError(String::from(
            "output Way has no Highway key",
        ))),
        Some(h_str) => {
            let tags = h_str
                .split(delimiter)
                .map(|h| {
                    Highway::from_str(h).map_err(|e| {
                        OsmError::InvalidOsmData(format!("found invalid highway tag {e}"))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let highway = tags
                .into_iter()
                .max_by_key(|t| t.hierarchy())
                .ok_or_else(|| {
                    OsmError::InternalError(String::from(
                        "non-empty row Highway tag has empty set of tags",
                    ))
                })?;
            Ok(highway)
        }
    }
}
