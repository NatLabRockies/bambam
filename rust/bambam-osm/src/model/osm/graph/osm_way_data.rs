use super::{OsmNodeId, OsmNodes, OsmWayId};
use crate::model::{
    feature::highway::Highway,
    osm::{graph::osm_way_ops, OsmError},
};
use bambam_network::{
    SegmentAccessRestriction, SegmentAccessRestrictionWhen, SegmentAccessType, SegmentClass,
    SegmentFullType, SegmentHeading, SegmentMode, SegmentRecognized, SegmentSubclass,
    SegmentSubtype, SegmentUsing,
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
    #[serde(default)]
    pub foot: Option<String>,
    #[serde(default)]
    pub bicycle: Option<String>,
    #[serde(default)]
    pub motor_vehicle: Option<String>,
    #[serde(default)]
    pub motorcar: Option<String>,
    #[serde(default)]
    pub motorcycle: Option<String>,
    #[serde(default)]
    pub vehicle: Option<String>,
    #[serde(default)]
    pub hgv: Option<String>,
    #[serde(default)]
    pub bus: Option<String>,
    #[serde(default)]
    pub oneway_bicycle: Option<String>,
    #[serde(default)]
    pub oneway_foot: Option<String>,
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
                "foot" => out.foot = Some(String::from(v.trim())),
                "bicycle" => out.bicycle = Some(String::from(v.trim())),
                "motor_vehicle" => out.motor_vehicle = Some(String::from(v.trim())),
                "motorcar" => out.motorcar = Some(String::from(v.trim())),
                "motorcycle" => out.motorcycle = Some(String::from(v.trim())),
                "vehicle" => out.vehicle = Some(String::from(v.trim())),
                "hgv" => out.hgv = Some(String::from(v.trim())),
                "bus" => out.bus = Some(String::from(v.trim())),
                "oneway:bicycle" => out.oneway_bicycle = Some(String::from(v.trim())),
                "oneway:foot" => out.oneway_foot = Some(String::from(v.trim())),
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

    /// Maps this OSM way's `highway`, `service`, `footway`, and `cycleway` tags into the shared
    /// Overture Maps [`SegmentFullType`] (`SegmentSubtype`, `SegmentClass`, `Option<SegmentSubclass>`).
    pub fn to_segment_full_type(&self, highway: &Highway) -> SegmentFullType {
        let (class, hw_subclass) = match highway {
            Highway::Motorway => (SegmentClass::Motorway, None),
            Highway::MotorwayLink => (SegmentClass::Motorway, Some(SegmentSubclass::Link)),
            Highway::Trunk => (SegmentClass::Trunk, None),
            Highway::TrunkLink => (SegmentClass::Trunk, Some(SegmentSubclass::Link)),
            Highway::Primary => (SegmentClass::Primary, None),
            Highway::PrimaryLink => (SegmentClass::Primary, Some(SegmentSubclass::Link)),
            Highway::Secondary => (SegmentClass::Secondary, None),
            Highway::SecondaryLink => (SegmentClass::Secondary, Some(SegmentSubclass::Link)),
            Highway::Tertiary => (SegmentClass::Tertiary, None),
            Highway::TertiaryLink => (SegmentClass::Tertiary, Some(SegmentSubclass::Link)),
            Highway::Residential => (SegmentClass::Residential, None),
            Highway::LivingStreet => (SegmentClass::LivingStreet, None),
            Highway::Unclassified => (SegmentClass::Unclassified, None),
            Highway::Service => {
                let sc = match self.service.as_deref().map(str::trim) {
                    Some("alley") => Some(SegmentSubclass::Alley),
                    Some("driveway") => Some(SegmentSubclass::Driveway),
                    Some("parking_aisle") => Some(SegmentSubclass::ParkingAisle),
                    _ => None,
                };
                (SegmentClass::Service, sc)
            }
            Highway::Pedestrian => (SegmentClass::Pedestrian, None),
            Highway::Footway => {
                let sc = match self.footway.as_deref().map(str::trim) {
                    Some("sidewalk") => Some(SegmentSubclass::Sidewalk),
                    Some("crossing") => Some(SegmentSubclass::Crosswalk),
                    _ => None,
                };
                (SegmentClass::Footway, sc)
            }
            Highway::Sidewalk => (SegmentClass::Footway, Some(SegmentSubclass::Sidewalk)),
            Highway::Crossing => (SegmentClass::Footway, Some(SegmentSubclass::Crosswalk)),
            Highway::Steps => (SegmentClass::Steps, None),
            Highway::Path => (SegmentClass::Path, None),
            Highway::Track => (SegmentClass::Track, None),
            Highway::Cycleway => {
                let sc = match self.cycleway.as_deref().map(str::trim) {
                    Some("crossing") => Some(SegmentSubclass::CycleCrossing),
                    _ => None,
                };
                (SegmentClass::Cycleway, sc)
            }
            Highway::Bridleway => (SegmentClass::Bridleway, None),
            Highway::Road => (SegmentClass::Unknown, None),
            other => (SegmentClass::Custom(other.to_string()), None),
        };
        SegmentFullType(SegmentSubtype::Road, class, hw_subclass)
    }

    /// Synthesizes Overture-compatible [`SegmentAccessRestriction`] records from this OSM way's
    /// `access`, `service`, `oneway`, `junction`, and mode-specific tags (`foot`, `bicycle`,
    /// `motor_vehicle`, `motorcar`, `motorcycle`, `vehicle`, `hgv`, `bus`, `oneway:bicycle`, `oneway:foot`).
    pub fn to_access_restrictions(&self) -> Vec<SegmentAccessRestriction> {
        let mut restrictions = Vec::new();

        // 1. General `access=*` or `service=private`
        if let Some(acc) = self.access.as_deref() {
            if let Some(r) = build_osm_access_restriction(acc, None, None) {
                restrictions.push(r);
            }
        }
        if matches!(self.service.as_deref(), Some("private")) {
            restrictions.push(SegmentAccessRestriction {
                access_type: SegmentAccessType::Denied,
                when: None,
                vehicle: None,
            });
        }

        // 2. Mode-specific access tags
        let all_vehicle_modes = vec![
            SegmentMode::Vehicle,
            SegmentMode::Bicycle,
            SegmentMode::MotorVehicle,
            SegmentMode::Car,
            SegmentMode::Truck,
            SegmentMode::Motorcycle,
            SegmentMode::Bus,
            SegmentMode::Hgv,
            SegmentMode::Hov,
        ];
        let motor_vehicle_modes = vec![
            SegmentMode::MotorVehicle,
            SegmentMode::Car,
            SegmentMode::Truck,
            SegmentMode::Motorcycle,
            SegmentMode::Bus,
            SegmentMode::Hgv,
            SegmentMode::Hov,
        ];

        if let Some(val) = self.vehicle.as_deref() {
            if let Some(r) =
                build_osm_access_restriction(val, Some(all_vehicle_modes.clone()), None)
            {
                restrictions.push(r);
            }
        }
        if let Some(val) = self.motor_vehicle.as_deref() {
            if let Some(r) =
                build_osm_access_restriction(val, Some(motor_vehicle_modes.clone()), None)
            {
                restrictions.push(r);
            }
        }
        if let Some(val) = self.motorcar.as_deref() {
            if let Some(r) = build_osm_access_restriction(val, Some(vec![SegmentMode::Car]), None) {
                restrictions.push(r);
            }
        }
        if let Some(val) = self.motorcycle.as_deref() {
            if let Some(r) =
                build_osm_access_restriction(val, Some(vec![SegmentMode::Motorcycle]), None)
            {
                restrictions.push(r);
            }
        }
        if let Some(val) = self.hgv.as_deref() {
            if let Some(r) = build_osm_access_restriction(
                val,
                Some(vec![SegmentMode::Hgv, SegmentMode::Truck]),
                None,
            ) {
                restrictions.push(r);
            }
        }
        if let Some(val) = self.bus.as_deref() {
            if let Some(r) = build_osm_access_restriction(val, Some(vec![SegmentMode::Bus]), None) {
                restrictions.push(r);
            }
        }
        if let Some(val) = self.bicycle.as_deref() {
            if let Some(r) =
                build_osm_access_restriction(val, Some(vec![SegmentMode::Bicycle]), None)
            {
                restrictions.push(r);
            }
        }
        if let Some(val) = self.foot.as_deref() {
            if let Some(r) = build_osm_access_restriction(val, Some(vec![SegmentMode::Foot]), None)
            {
                restrictions.push(r);
            }
        }

        // 3. Directional (`oneway`) restrictions
        // In OSM (and OMF), `oneway` applies to vehicles, NOT pedestrians (`foot`), unless
        // `highway=footway|steps|pedestrian|path|corridor` or `oneway:foot` is explicitly set.
        if self.is_one_way() {
            let denied_heading = if self.is_reverse() {
                SegmentHeading::Forward
            } else {
                SegmentHeading::Backward
            };

            let is_pedestrian_way = matches!(
                self.highway.as_deref(),
                Some("footway" | "steps" | "pedestrian" | "corridor")
            );
            let oneway_modes = if is_pedestrian_way {
                None
            } else {
                Some(all_vehicle_modes)
            };

            restrictions.push(SegmentAccessRestriction {
                access_type: SegmentAccessType::Denied,
                when: Some(SegmentAccessRestrictionWhen {
                    heading: Some(denied_heading.clone()),
                    mode: oneway_modes,
                    ..Default::default()
                }),
                vehicle: None,
            });

            // `oneway:bicycle=no` or `cycleway=opposite*` allows counter-flow cycling
            let contraflow_bike = matches!(
                self.oneway_bicycle.as_deref().map(str::trim),
                Some("no" | "false" | "0")
            ) || matches!(
                self.cycleway.as_deref().map(str::trim),
                Some("opposite" | "opposite_lane" | "opposite_track" | "opposite_share_busway")
            );
            if contraflow_bike {
                restrictions.push(SegmentAccessRestriction {
                    access_type: SegmentAccessType::Allowed,
                    when: Some(SegmentAccessRestrictionWhen {
                        heading: Some(denied_heading),
                        mode: Some(vec![SegmentMode::Bicycle]),
                        ..Default::default()
                    }),
                    vehicle: None,
                });
            }
        }

        if let Some(ow_foot) = self.oneway_foot.as_deref().map(str::trim) {
            if matches!(ow_foot, "yes" | "true" | "1") {
                restrictions.push(SegmentAccessRestriction {
                    access_type: SegmentAccessType::Denied,
                    when: Some(SegmentAccessRestrictionWhen {
                        heading: Some(SegmentHeading::Backward),
                        mode: Some(vec![SegmentMode::Foot]),
                        ..Default::default()
                    }),
                    vehicle: None,
                });
            } else if matches!(ow_foot, "-1" | "reverse") {
                restrictions.push(SegmentAccessRestriction {
                    access_type: SegmentAccessType::Denied,
                    when: Some(SegmentAccessRestrictionWhen {
                        heading: Some(SegmentHeading::Forward),
                        mode: Some(vec![SegmentMode::Foot]),
                        ..Default::default()
                    }),
                    vehicle: None,
                });
            }
        }

        restrictions
    }
}

fn build_osm_access_restriction(
    raw_val: &str,
    modes: Option<Vec<SegmentMode>>,
    heading: Option<SegmentHeading>,
) -> Option<SegmentAccessRestriction> {
    let val = raw_val.trim().to_ascii_lowercase();
    let (access_type, using, recognized) = match val.as_str() {
        "no" | "private" | " agricultural" | "forestry" | "emergency" | "military" | "razed" => {
            (SegmentAccessType::Denied, None, None)
        }
        "yes" | "permissive" | "public" | "use_sidepath" => {
            (SegmentAccessType::Allowed, None, None)
        }
        "designated" | "official" => (SegmentAccessType::Designated, None, None),
        "destination" | "delivery" | "customers" => (
            SegmentAccessType::Allowed,
            Some(vec![SegmentUsing::AtDestination]),
            None,
        ),
        "permit" => (
            SegmentAccessType::Allowed,
            None,
            Some(vec![SegmentRecognized::AsPermitted]),
        ),
        "employees" => (
            SegmentAccessType::Allowed,
            None,
            Some(vec![SegmentRecognized::AsEmployee]),
        ),
        "students" => (
            SegmentAccessType::Allowed,
            None,
            Some(vec![SegmentRecognized::AsStudent]),
        ),
        "disabled" => (
            SegmentAccessType::Allowed,
            None,
            Some(vec![SegmentRecognized::AsDisabled]),
        ),
        _ => return None,
    };

    let has_when = modes.is_some() || heading.is_some() || using.is_some() || recognized.is_some();
    let when = if has_when {
        Some(SegmentAccessRestrictionWhen {
            heading,
            mode: modes,
            using,
            recognized,
            ..Default::default()
        })
    } else {
        None
    };

    Some(SegmentAccessRestriction {
        access_type,
        when,
        vehicle: None,
    })
}

impl<'a> From<&'a osmpbf::elements::Way<'a>> for OsmWayData {
    fn from(value: &'a osmpbf::elements::Way) -> Self {
        OsmWayData::new(value)
    }
}
