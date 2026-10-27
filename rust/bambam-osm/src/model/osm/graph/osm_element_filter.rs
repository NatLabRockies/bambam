use crate::model::{feature::highway::Highway, osm::overpass::FilterQuery};
use bambam_network::{
    MatchBehavior, NetworkEdgeListConfiguration, SegmentClass, TravelModeFilter,
};
use osmpbf::{Element, Way};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, str::FromStr};

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ElementFilter {
    #[default]
    NoFilter,
    OsmnxAllPublic,
    HighwayTags {
        tags: HashSet<Highway>,
    },
    OverpassQueries {
        queries: Vec<FilterQuery>,
    },
}

impl ElementFilter {
    pub fn accept(&self, element: &Element) -> bool {
        use ElementFilter as F;
        match self {
            F::NoFilter => true,
            F::OsmnxAllPublic => osmnx_all_public_filter(element),
            F::HighwayTags { tags } => custom_highway_tag_filter(element, tags),
            F::OverpassQueries { queries } => custom_overpass_queries_filter(element, queries),
        }
    }

    /// Derives an [`ElementFilter`] from a list of [`NetworkEdgeListConfiguration`]s by taking
    /// the union of all included [`SegmentClass`] values across the configured travel modes.
    pub fn from_edge_list_configs(configs: &[NetworkEdgeListConfiguration]) -> Self {
        if configs.is_empty() {
            return Self::OsmnxAllPublic;
        }
        let mut tags = HashSet::new();
        for cfg in configs {
            let mut mode_has_include = false;
            for f in &cfg.filter {
                match f {
                    TravelModeFilter::MatchesClasses {
                        classes,
                        behavior: MatchBehavior::Include,
                        ..
                    } => {
                        mode_has_include = true;
                        for cls in classes {
                            tags.extend(segment_class_to_highways(cls));
                        }
                    }
                    TravelModeFilter::MatchesClassesWithSubclasses {
                        classes,
                        behavior: MatchBehavior::Include,
                        ..
                    } => {
                        mode_has_include = true;
                        for cls in classes.keys() {
                            tags.extend(segment_class_to_highways(cls));
                        }
                    }
                    _ => {}
                }
            }
            if !mode_has_include {
                return Self::OsmnxAllPublic;
            }
        }
        if tags.is_empty() {
            Self::OsmnxAllPublic
        } else {
            Self::HighwayTags { tags }
        }
    }

    /// Returns a built-in `ElementFilter` matching the requested travel mode names
    /// (`"walk"`, `"bike"`, `"drive"`), or `ElementFilter::OsmnxAllPublic` if `None` or empty.
    pub fn default_for_modes(modes: Option<&[String]>) -> Self {
        let Some(mode_list) = modes.filter(|m| !m.is_empty()) else {
            return Self::OsmnxAllPublic;
        };

        let mut configs = Vec::new();
        for mode in mode_list {
            match NetworkEdgeListConfiguration::default_for_mode(mode) {
                Ok(cfg) => configs.push(cfg),
                Err(_) => return Self::OsmnxAllPublic,
            }
        }
        Self::from_edge_list_configs(&configs)
    }
}

fn segment_class_to_highways(cls: &SegmentClass) -> Vec<Highway> {
    match cls {
        SegmentClass::Motorway => vec![Highway::Motorway, Highway::MotorwayLink],
        SegmentClass::Trunk => vec![Highway::Trunk, Highway::TrunkLink],
        SegmentClass::Primary => vec![Highway::Primary, Highway::PrimaryLink],
        SegmentClass::Secondary => vec![Highway::Secondary, Highway::SecondaryLink],
        SegmentClass::Tertiary => vec![Highway::Tertiary, Highway::TertiaryLink],
        SegmentClass::Residential => vec![Highway::Residential],
        SegmentClass::LivingStreet => vec![Highway::LivingStreet],
        SegmentClass::Unclassified => vec![Highway::Unclassified],
        SegmentClass::Service => vec![Highway::Service],
        SegmentClass::Pedestrian => vec![Highway::Pedestrian],
        SegmentClass::Footway => vec![Highway::Footway, Highway::Sidewalk, Highway::Crossing],
        SegmentClass::Steps => vec![Highway::Steps],
        SegmentClass::Path => vec![Highway::Path],
        SegmentClass::Track => vec![Highway::Track],
        SegmentClass::Cycleway => vec![Highway::Cycleway],
        SegmentClass::Bridleway => vec![Highway::Bridleway],
        SegmentClass::Unknown => vec![Highway::Road],
        SegmentClass::Custom(s) => Highway::from_str(s).into_iter().collect(),
    }
}

/// filters ways that do not have a highway tag present in the tags argument,
/// or that fail public-road checks (`area=yes`, `access=private`, `service=private`).
/// has no effect on Relation/Node/DenseNode Elements.
///
/// # Arguments
///
/// * `element` - OSM element to test filtering
/// * `tags` - list of [`Highway`] tags that are accepted
///
/// # Returns
///
/// true if the [`Element`] is a [`Way`] with a valid [`Highway`] tag, or is not a [`Way`]
fn custom_highway_tag_filter(element: &Element, tags: &HashSet<Highway>) -> bool {
    match element {
        Element::Way(way) => {
            if !osmnx_all_public_filter(element) {
                return false;
            }
            match get_highway_tag(way) {
                Some(highway) => tags.contains(&highway),
                None => false, // throw out Ways without Highway tags
            }
        }
        Element::Relation(_) => false,
        _ => true,
    }
}

/// applies overpass queries that are filter-type queries
fn custom_overpass_queries_filter(element: &Element, queries: &[FilterQuery]) -> bool {
    queries.iter().all(|q| q.filter(element))
}

/// OSMNX definition:
/// filters["all_public"] = (
///     f'["highway"]["area"!~"yes"]{settings.default_access}'
///     f'["highway"!~"abandoned|construction|no|planned|platform|proposed|raceway|razed"]'
///     f'["service"!~"private"]'
/// )
///
/// # Return
///
/// * true if we accept this Element, false if it does not pass a filter criteria
fn osmnx_all_public_filter(e: &Element) -> bool {
    match e {
        Element::Node(_) => return true,
        Element::DenseNode(_) => return true,
        Element::Relation(_) => return false,
        _ => {}
    }
    // ["highway"]

    let highway = match get_tag(e, "highway") {
        Some(h) => match Highway::from_str(&h) {
            Err(_) => return false,
            Ok(highway) => highway,
        },
        None => {
            log::debug!("no 'highway' tag");
            return false;
        }
    };

    // ["area"!~"yes"]
    if let Some(area_is_yes) = get_tag(e, "area").map(|a| a == "yes") {
        if area_is_yes {
            log::debug!("['area'!~'yes']");
            return false;
        }
    }
    // settings.default_access aka ["access"!~"private"]
    if let Some(access_private) = get_tag(e, "access").map(|a| a == "private") {
        if access_private {
            log::debug!("['access'!~'private']");
            return false;
        }
    }

    // ["highway"!~"abandoned|construction|no|planned|platform|proposed|raceway|razed"]
    let not_in_use = match highway {
        Highway::Other(s) if s == *"abandoned" => true,
        Highway::Construction => true,
        Highway::Other(s) if s == *"no" => true,
        Highway::Other(s) if s == *"planned" => true,
        Highway::Platform => true,
        Highway::Proposed => true,
        Highway::Raceway => true,
        Highway::Other(s) if s == *"razed" => true,
        _ => false,
    };

    if not_in_use {
        log::debug!(
            "['highway'~'abandoned|construction|no|planned|platform|proposed|raceway|razed']"
        );
        return false;
    }

    // ["service"!~"private"]
    let service_private = matches!(get_tag(e, "service"), Some(service) if service == "private");
    if service_private {
        log::debug!("['service'~'private']");
        return false;
    }

    // all clear
    true
}

/// uses a linear scan to find the [`Highway`] tag if present.
fn get_highway_tag(way: &Way<'_>) -> Option<Highway> {
    let v = way.tags().find(|(k, _)| *k == "highway");
    match v {
        Some((_, v)) => Highway::from_str(v).ok(),
        None => None,
    }
}

/// extract the value associated with a given tag. this operation is
/// _O(n)_ since it needs to scan the TagsIter of an Element.
fn get_tag(e: &Element<'_>, tag: &str) -> Option<String> {
    match e {
        Element::Node(node) => node
            .tags()
            .find(|(k, _)| *k == tag)
            .map(|(_, v)| String::from(v)),
        Element::DenseNode(dense_node) => dense_node
            .tags()
            .find(|(k, _)| *k == tag)
            .map(|(_, v)| String::from(v)),
        Element::Way(way) => way
            .tags()
            .find(|(k, _)| *k == tag)
            .map(|(_, v)| String::from(v)),
        _ => None,
    }
}

// fn deserialize_overpass_queries<'de, D>(d: D) -> Result<ElementFilter, D::Error>
// where
//     D: Deserializer<'de>,
// {
//     // define a visitor that deserializes
//     // `ActualData` encoded as json within a string
//     struct JsonStringVisitor;

//     impl<'de> serde::de::Visitor<'de> for JsonStringVisitor {
//         type Value = ActualData;

//         fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
//             formatter.write_str("a string containing json data")
//         }

//         fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
//         where
//             A: serde::de::MapAccess<'de>,
//         {
//             let mut row: Option<(String, Vec<String>)> = map.next_entry()?;
//             let mut values: Option<Vec<String>> = None;
//             while row.is_some() {
//                 match row {
//                     None => break,
//                     Some((k, v)) if &k == "queries" => values = Some(v),
//                     _ => {}
//                 }
//                 row = map.next_entry()?;
//             }
//         }
//     }

//     // use our visitor to deserialize an `ActualValue`
//     deserializer.deserialize_any(JsonStringVisitor)
// }
