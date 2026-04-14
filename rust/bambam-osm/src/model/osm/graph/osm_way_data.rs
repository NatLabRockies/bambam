use super::{OsmNodeId, OsmNodes, OsmWayId};
use crate::model::{
    feature::highway::Highway,
    osm::{graph::osm_way_ops, OsmError},
};
use geo::{Coord, LineString};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct OsmWayData {
    pub osmid: OsmWayId,
    pub nodes: Vec<OsmNodeId>,
    pub access: Option<String>,
    pub area: Option<String>,
    pub bridge: Option<String>,
    pub est_width: Option<String>,
    pub highway: Option<String>,
    pub sidewalk: Option<String>,
    pub cycleway: Option<String>,
    pub footway: Option<String>,
    pub junction: Option<String>,
    pub landuse: Option<String>,
    pub lanes: Option<String>,
    pub maxspeed: Option<String>,
    pub name: Option<String>,
    pub oneway: Option<String>,
    pub _ref: Option<String>,
    pub service: Option<String>,
    pub tunnel: Option<String>,
    pub width: Option<String>,
    /// when ways are simplified, the list of composite way ids are stored here.
    /// the Way.osmid will remain present in any aggregate way_ids collection.
    pub way_ids: Vec<OsmWayId>,
}

impl OsmWayData {
    pub const VALUE_DELIMITER: &'static str = "#";

    pub fn new(way: &osmpbf::elements::Way) -> OsmWayData {
        let mut out = OsmWayData {
            osmid: OsmWayId(way.id()),
            nodes: way.refs().map(OsmNodeId).collect_vec(),
            ..Default::default()
        };

        // as in osmnx.graph._convert_path, remove duplicates in the node path (by identity function)
        out.nodes.dedup();
        if out.nodes.is_empty() {
            log::warn!(
                "way {} had {} nodes but after deduplication has 0",
                way.id(),
                way.refs().len()
            );
        }

        for (k, v) in way.tags() {
            match k {
                "access" => out.access = Some(String::from(v.trim())),
                "area" => out.area = Some(String::from(v.trim())),
                "bridge" => out.bridge = Some(String::from(v.trim())),
                "est_width" => out.est_width = Some(String::from(v.trim())),
                "highway" => out.highway = Some(String::from(v.trim())),
                "sidewalk" => out.sidewalk = Some(String::from(v.trim())),
                "cycleway" => out.cycleway = Some(String::from(v.trim())),
                "footway" => out.footway = Some(String::from(v.trim())),
                "junction" => out.junction = Some(String::from(v.trim())),
                "landuse" => out.landuse = Some(String::from(v.trim())),
                "lanes" => out.lanes = Some(String::from(v.trim())),
                "maxspeed" => out.maxspeed = Some(String::from(v.trim())),
                "name" => out.name = Some(String::from(v.trim())),
                "oneway" => out.oneway = Some(String::from(v.trim())),
                "ref" => out._ref = Some(String::from(v.trim())),
                "service" => out.service = Some(String::from(v.trim())),
                "tunnel" => out.tunnel = Some(String::from(v.trim())),
                "width" => out.width = Some(String::from(v.trim())),
                _ => {}
            }
        }
        out
    }

    pub fn create_linestring(
        &self,
        raw_nodes: &OsmNodes,
        ignore_missing: bool,
    ) -> Result<LineString<f32>, OsmError> {
        let coords = self
            .nodes
            .iter()
            .map(|id| match raw_nodes.get(id) {
                Some(node) => Ok(Some(Coord::from((node.x, node.y)))),
                None if ignore_missing => Ok(None),
                None => Err(OsmError::InternalError(format!(
                    "node '{}' present in way '{}' not found in pbf nodelist",
                    id, self.osmid
                ))),
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect_vec();
        Ok(LineString(coords))
    }

    // pub fn length_meters(&self, nodes: &OsmNodes) -> Result<f32, String> {
    //     let linestring = self.create_linestring(nodes)?;
    //     Ok(linestring.length::<Haversine>())
    // }

    pub fn get_string_at_field(&self, fieldname: &str) -> Result<Option<String>, String> {
        match fieldname {
            "access" => Ok(self.access.clone()),
            "area" => Ok(self.area.clone()),
            "bridge" => Ok(self.bridge.clone()),
            "est_width" => Ok(self.est_width.clone()),
            "highway" => Ok(self.highway.clone()),
            "sidewalk" => Ok(self.sidewalk.clone()),
            "footway" => Ok(self.footway.clone()),
            "cycleway" => Ok(self.cycleway.clone()),
            "junction" => Ok(self.junction.clone()),
            "landuse" => Ok(self.landuse.clone()),
            "lanes" => Ok(self.lanes.clone()),
            "maxspeed" => Ok(self.maxspeed.clone()),
            "name" => Ok(self.name.clone()),
            "oneway" => Ok(self.oneway.clone()),
            "ref" => Ok(self._ref.clone()),
            "service" => Ok(self.service.clone()),
            "tunnel" => Ok(self.tunnel.clone()),
            "width" => Ok(self.width.clone()),
            _ => Err(format!("unknown edge field {fieldname}")),
        }
    }

    pub fn get_f64_at_field(&self, fieldname: &str) -> Result<Option<f64>, String> {
        let s = self.get_string_at_field(fieldname)?;
        match s {
            None => Ok(None),
            Some(string_value) => {
                let f64_value = string_value.parse::<f64>().map_err(|e| {
                    format!(
                        "could not parse value {string_value} of osm way field {fieldname} as numeric: {e}"
                    )
                })?;
                Ok(Some(f64_value))
            }
        }
    }

    /// follows the rules described in
    /// https://wiki.openstreetmap.org/wiki/Key:maxspeed#Values
    pub fn get_speed_value(
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

    /// interpret the highway field as a [`Highway`] enumeration type.
    pub fn get_highway(&self) -> Result<Option<Highway>, OsmError> {
        match &self.highway {
            Some(h) => {
                let highway = Highway::from_str(h).map_err(|e| {
                    OsmError::InvalidOsmData(format!("unable to deserialize Highway tag {h}: {e}"))
                })?;
                Ok(Some(highway))
            }
            None => Ok(None),
        }
    }

    /// compares this way with another one. if this one's highway tag is better
    /// than the other, returns true.
    ///
    /// todo: can we think up a better name here?
    pub fn has_dominating_highway_hierarchy(&self, other: &OsmWayData) -> Result<bool, OsmError> {
        match (self.get_highway(), other.get_highway()) {
            (Ok(Some(a)), Ok(Some(b))) => Ok(a < b),
            (Ok(Some(_)), Ok(None)) => Ok(true),
            (Ok(None), Ok(Some(_))) => Ok(false),
            (Ok(None), Ok(None)) => Ok(false),
            (_, Err(e)) => Err(e),
            (Err(e), _) => Err(e),
        }
    }

    /// osmnx.graph._is_path_one_way
    ///   the values OSM uses in its 'oneway' tag to denote True, and to denote
    ///   travel can only occur in the opposite direction of the node order. see:
    ///   https://wiki.openstreetmap.org/wiki/Key:oneway
    ///   https://www.geofabrik.de/de/data/geofabrik-osm-gis-standard-0.7.pdf
    ///     ```python
    ///     oneway_values = {"yes", "true", "1", "-1", "reverse", "T", "F"}
    ///     reversed_values = {"-1", "reverse", "T"}
    ///     ```
    pub fn is_one_way(&self) -> bool {
        // "rule 1" is the `all_oneway` OSMNX configuration option which doesn't apply for us
        // "rule 2" is the "bidirectional" OSMNX network type (aka undirected), doesn't apply for us
        // "rule 3" checks the oneway tag
        if let Some(oneway) = &self.oneway {
            matches!(
                oneway.as_str().trim(),
                "yes" | "true" | "1" | "-1" | "reverse" | "T" | "F"
            )
        } else if let Some(junction) = &self.junction {
            // "rule 4" states that "roundabouts are also one-way but are not explicitly tagged as such"
            junction.as_str().trim() == "roundabout"
        } else {
            false
        }
    }

    /// osmnx.graph._is_path_reversed
    pub fn is_reverse(&self) -> bool {
        // python: `"oneway" in attrs and attrs["oneway"] in reversed_values`
        if let Some(oneway) = &self.oneway {
            matches!(oneway.as_str(), "-1" | "reverse" | "T")
        } else {
            false
        }
    }

    pub fn src_node_id(&self) -> Result<OsmNodeId, OsmError> {
        self.nodes.first().cloned().ok_or_else(|| {
            OsmError::InvalidOsmData(format!("way data for way {} has no nodes", self.osmid))
        })
    }

    pub fn dst_node_id(&self) -> Result<OsmNodeId, OsmError> {
        self.nodes.iter().next_back().cloned().ok_or_else(|| {
            OsmError::InvalidOsmData(format!("way data for way {} has no nodes", self.osmid))
        })
    }
}

impl<'a> From<&'a osmpbf::elements::Way<'a>> for OsmWayData {
    fn from(value: &'a osmpbf::elements::Way) -> Self {
        OsmWayData::new(value)
    }
}

impl TryFrom<&[&OsmWayData]> for OsmWayData {
    type Error = OsmError;

    fn try_from(ways: &[&OsmWayData]) -> Result<Self, Self::Error> {
        let way_ids = ways
            .iter()
            .flat_map(|w| {
                let mut this_ids = w.way_ids.clone();
                this_ids.push(w.osmid);
                this_ids
            })
            .collect_vec();
        let new_way_id = *way_ids.first().ok_or_else(|| {
            OsmError::GraphSimplificationError(String::from(
                "attempting to build aggregated way from empty collection",
            ))
        })?;

        let mut nodes = ways.iter().flat_map(|w| w.nodes.clone()).collect_vec();
        nodes.dedup();

        // let maxspeed: Option<String> = aggregate_speed("maxspeed", ways)?;

        // we always want to aggregate to a single OSM:Highway key for this way data
        let highway = ways
            .iter()
            .flat_map(|w| w.highway.clone().map(|h| Highway::from_str(&h)))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                OsmError::GraphConsolidationError(format!(
                    "failure aggregating 'highway' tag on segment: {e}"
                ))
            })?
            .into_iter()
            .min_by_key(|h| h.hierarchy())
            .map(|h| h.to_string());

        // oneway is "true" for any aggregated link in our system
        let access = merge_fieldname(ways, "access", Self::VALUE_DELIMITER)?;
        let area = merge_fieldname(ways, "area", Self::VALUE_DELIMITER)?;
        let bridge = merge_fieldname(ways, "bridge", Self::VALUE_DELIMITER)?;
        let est_width = merge_fieldname(ways, "est_width", Self::VALUE_DELIMITER)?;
        // let highway = merge_fieldname(ways, "highway", Self::VALUE_DELIMITER)?;
        let junction = merge_fieldname(ways, "junction", Self::VALUE_DELIMITER)?;
        let sidewalk = merge_fieldname(ways, "sidewalk", Self::VALUE_DELIMITER)?;
        let cycleway = merge_fieldname(ways, "cycleway", Self::VALUE_DELIMITER)?;
        let footway = merge_fieldname(ways, "footway", Self::VALUE_DELIMITER)?;
        let landuse = merge_fieldname(ways, "landuse", Self::VALUE_DELIMITER)?;
        let lanes = merge_fieldname(ways, "lanes", Self::VALUE_DELIMITER)?;
        let maxspeed = merge_fieldname(ways, "maxspeed", Self::VALUE_DELIMITER)?;
        let name = merge_fieldname(ways, "name", Self::VALUE_DELIMITER)?;
        let oneway = Some(String::from("true"));
        let _ref = merge_fieldname(ways, "ref", Self::VALUE_DELIMITER)?;
        let service = merge_fieldname(ways, "service", Self::VALUE_DELIMITER)?;
        let tunnel = merge_fieldname(ways, "tunnel", Self::VALUE_DELIMITER)?;
        let width = merge_fieldname(ways, "width", Self::VALUE_DELIMITER)?;

        let new_way = OsmWayData {
            osmid: new_way_id,
            nodes,
            access,
            area,
            bridge,
            est_width,
            highway,
            sidewalk,
            cycleway,
            footway,
            junction,
            landuse,
            lanes,
            maxspeed,
            name,
            oneway,
            _ref,
            service,
            tunnel,
            width,
            way_ids,
        };

        Ok(new_way)
    }
}

fn merge_fieldname(
    ways: &[&OsmWayData],
    fieldname: &str,
    sep: &str,
) -> Result<Option<String>, OsmError> {
    let opt_values = ways
        .iter()
        .map(|w| w.get_string_at_field(fieldname))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            OsmError::GraphSimplificationError(format!(
                "failure merging '{fieldname}' field across ways: {e}"
            ))
        })?;
    let result = opt_values
        .into_iter()
        .flatten()
        .reduce(|a, b| format!("{a}{sep}{b}"));
    Ok(result)
}
