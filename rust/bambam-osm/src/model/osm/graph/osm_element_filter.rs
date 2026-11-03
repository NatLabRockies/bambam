use crate::model::feature::highway::Highway;
use bambam_network::{MatchBehavior, NetworkEdgeListConfiguration, SegmentClass, TravelModeFilter};
use osmpbf::{Element, Way};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, str::FromStr};

/// Pre-filters PBF elements during initial ingestion based on structural road criteria
/// and the union of [`Highway`] tags included across configured [`NetworkEdgeListConfiguration`]s.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct ElementFilter {
    pub tags: Option<HashSet<Highway>>,
}

impl ElementFilter {
    pub fn accept(&self, element: &Element) -> bool {
        match element {
            Element::Way(way) => match get_highway_tag(way) {
                Some(highway) => {
                    passes_structural_way_checks(way, &highway)
                        && self.tags.as_ref().is_none_or(|t| t.contains(&highway))
                }
                None => false,
            },
            Element::Relation(_) => false,
            _ => true,
        }
    }

    /// Derives an [`ElementFilter`] from a list of [`NetworkEdgeListConfiguration`]s by taking
    /// the union of all included [`SegmentClass`] values across the configured travel modes.
    pub fn from_edge_list_configs(configs: &[NetworkEdgeListConfiguration]) -> Self {
        if configs.is_empty() {
            return Self { tags: None };
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
                return Self { tags: None };
            }
        }
        if tags.is_empty() {
            Self { tags: None }
        } else {
            Self { tags: Some(tags) }
        }
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

/// Structural exclusions for OSM ways: rejects `area=yes` polygons and non-active/non-road
/// `highway` lifecycle tags. Access restrictions (`access=private`, `service=private`, etc.)
/// are deferred to mode-specific `TravelModeFilter` evaluation so mode-level overrides survive.
fn passes_structural_way_checks(way: &Way<'_>, highway: &Highway) -> bool {
    // ["area"!~"yes"]
    if way.tags().any(|(k, v)| k == "area" && v == "yes") {
        log::debug!("['area'!~'yes']");
        return false;
    }

    // ["highway"!~"abandoned|construction|no|planned|platform|proposed|raceway|razed"]
    let not_in_use = match highway {
        Highway::Other(s) if matches!(s.as_str(), "abandoned" | "no" | "planned" | "razed") => true,
        Highway::Construction | Highway::Platform | Highway::Proposed | Highway::Raceway => true,
        _ => false,
    };

    if not_in_use {
        log::debug!(
            "['highway'~'abandoned|construction|no|planned|platform|proposed|raceway|razed']"
        );
        return false;
    }

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
