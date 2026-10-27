use std::{
    collections::{HashMap, HashSet},
    fmt,
    str::FromStr,
};

use geo::Coord;
use opening_hours_syntax::rules::OpeningHoursExpression;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{island_detection::IslandDetectionAlgorithm, model::SegmentHeading};

/// Bounding box specification in WGS84 lon/lat degrees.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CliBoundingBox {
    pub xmin: f32,
    pub xmax: f32,
    pub ymin: f32,
    pub ymax: f32,
}

impl CliBoundingBox {
    pub fn new(xmin: f32, ymin: f32, xmax: f32, ymax: f32) -> Result<Self, String> {
        if xmin > xmax {
            return Err(format!(
                "invalid bbox: xmin {xmin} greater than xmax {xmax}"
            ));
        }
        if ymin > ymax {
            return Err(format!(
                "invalid bbox: ymin {ymin} greater than ymax {ymax}"
            ));
        }
        Ok(Self {
            xmin,
            xmax,
            ymin,
            ymax,
        })
    }

    pub fn from_coords(min: &Coord<f32>, max: &Coord<f32>) -> Result<Self, String> {
        Self::new(min.x, min.y, max.x, max.y)
    }
}

impl FromStr for CliBoundingBox {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(',').collect();
        if parts.len() != 4 {
            return Err(format!("expected format: xmin,xmax,ymin,ymax, got: {s}"));
        }

        let xmin = parse_lon(parts[0])?;
        let xmax = parse_lon(parts[1])?;

        let ymin = parse_lat(parts[2])?;
        let ymax = parse_lat(parts[3])?;

        if xmin >= xmax {
            Err(format!(
                "bbox: xmin must be less than xmax, but found [{xmin},{xmax}]"
            ))
        } else if ymin >= ymax {
            Err(format!(
                "bbox: ymin must be less than ymax, but found [{ymin},{ymax}]"
            ))
        } else {
            Ok(CliBoundingBox {
                xmin,
                xmax,
                ymin,
                ymax,
            })
        }
    }
}

impl fmt::Display for CliBoundingBox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{},{},{},{}", self.xmin, self.xmax, self.ymin, self.ymax)
    }
}

fn parse_lat(lat: &str) -> Result<f32, String> {
    parse_num(lat, -90.0, 90.0).map_err(|e| format!("invalid latitude: {e}"))
}

fn parse_lon(lon: &str) -> Result<f32, String> {
    parse_num(lon, -180.0, 180.0).map_err(|e| format!("invalid longitude: {e}"))
}

fn parse_num(s: &str, min: f32, max: f32) -> Result<f32, String> {
    let v = s
        .trim()
        .parse::<f32>()
        .map_err(|_| format!("not a number: {s}"))?;
    if v < min || max < v {
        Err(format!(
            "number '{v}' is not valid, must be in range [{min},{max}]"
        ))
    } else {
        Ok(v)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SegmentSubtype {
    Road,
    Rail,
    Water,
}

impl fmt::Display for SegmentSubtype {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SegmentSubtype::Road => "road",
            SegmentSubtype::Rail => "rail",
            SegmentSubtype::Water => "water",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, JsonSchema)]
pub enum SegmentClass {
    Motorway,
    Primary,
    Secondary,
    Tertiary,
    Residential,
    LivingStreet,
    Trunk,
    Unclassified,
    Service,
    Pedestrian,
    Footway,
    Steps,
    Path,
    Track,
    Cycleway,
    Bridleway,
    Unknown,
    Custom(String),
}

impl Serialize for SegmentClass {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl fmt::Display for SegmentClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SegmentClass::Motorway => "motorway",
            SegmentClass::Primary => "primary",
            SegmentClass::Secondary => "secondary",
            SegmentClass::Tertiary => "tertiary",
            SegmentClass::Residential => "residential",
            SegmentClass::LivingStreet => "living_street",
            SegmentClass::Trunk => "trunk",
            SegmentClass::Unclassified => "unclassified",
            SegmentClass::Service => "service",
            SegmentClass::Pedestrian => "pedestrian",
            SegmentClass::Footway => "footway",
            SegmentClass::Steps => "steps",
            SegmentClass::Path => "path",
            SegmentClass::Track => "track",
            SegmentClass::Cycleway => "cycleway",
            SegmentClass::Bridleway => "bridleway",
            SegmentClass::Unknown => "unknown",
            SegmentClass::Custom(s) => s.as_str(),
        };
        f.write_str(s)
    }
}

impl<'de> Deserialize<'de> for SegmentClass {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(match s.as_str() {
            "motorway" => Self::Motorway,
            "primary" => Self::Primary,
            "secondary" => Self::Secondary,
            "tertiary" => Self::Tertiary,
            "residential" => Self::Residential,
            "living_street" => Self::LivingStreet,
            "trunk" => Self::Trunk,
            "unclassified" => Self::Unclassified,
            "service" => Self::Service,
            "pedestrian" => Self::Pedestrian,
            "footway" => Self::Footway,
            "steps" => Self::Steps,
            "path" => Self::Path,
            "track" => Self::Track,
            "cycleway" => Self::Cycleway,
            "bridleway" => Self::Bridleway,
            "unknown" => Self::Unknown,
            _ => Self::Custom(s),
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SegmentSubclass {
    Link,
    Sidewalk,
    Crosswalk,
    ParkingAisle,
    Driveway,
    Alley,
    CycleCrossing,
}

impl fmt::Display for SegmentSubclass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SegmentSubclass::Link => "link",
            SegmentSubclass::Sidewalk => "sidewalk",
            SegmentSubclass::Crosswalk => "crosswalk",
            SegmentSubclass::ParkingAisle => "parking_aisle",
            SegmentSubclass::Driveway => "driveway",
            SegmentSubclass::Alley => "alley",
            SegmentSubclass::CycleCrossing => "cycle_crossing",
        };
        f.write_str(s)
    }
}

/// Fully qualified segment type including type, class and subclass. E.g. `road-service-driveway`.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct SegmentFullType(
    pub SegmentSubtype,
    pub SegmentClass,
    pub Option<SegmentSubclass>,
);

impl SegmentFullType {
    pub fn has_subclass(&self) -> bool {
        self.2.is_some()
    }

    pub fn with_subclass(&self, subclass: SegmentSubclass) -> Self {
        Self(self.0.clone(), self.1.clone(), Some(subclass))
    }

    pub fn as_str(&self) -> String {
        match self.2.as_ref() {
            Some(subclass) => format!("{}-{}-{}", self.0, self.1, subclass),
            None => format!("{}-{}", self.0, self.1),
        }
    }
}

impl fmt::Display for SegmentFullType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_str())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SegmentUsing {
    AsCustomer,
    AtDestination,
    ToDeliver,
    ToFarm,
    ForForestry,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SegmentRecognized {
    AsPermitted,
    AsPrivate,
    AsDisabled,
    AsEmployee,
    AsStudent,
}

/// Travel mode for a segment.
/// See <https://docs.overturemaps.org/schema/concepts/by-theme/transportation/travel-modes/>
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SegmentMode {
    Vehicle,
    MotorVehicle,
    Car,
    Truck,
    Motorcycle,
    Foot,
    Bicycle,
    Bus,
    Hgv,
    Hov,
    Emergency,
}

impl SegmentMode {
    pub fn parent(&self) -> Option<SegmentMode> {
        match self {
            Self::Vehicle => None,
            Self::Foot => None,
            Self::Bicycle => Some(Self::Vehicle),
            Self::MotorVehicle => Some(Self::Vehicle),
            Self::Car => Some(Self::MotorVehicle),
            Self::Truck => Some(Self::MotorVehicle),
            Self::Motorcycle => Some(Self::MotorVehicle),
            Self::Bus => Some(Self::MotorVehicle),
            Self::Hgv => Some(Self::MotorVehicle),
            Self::Hov => Some(Self::MotorVehicle),
            Self::Emergency => Some(Self::MotorVehicle),
        }
    }
}

#[derive(Debug, Clone)]
pub enum DuringExpression {
    Osm(OpeningHoursExpression),
    Unexpected(String),
}

impl Serialize for DuringExpression {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            DuringExpression::Osm(expr) => serializer.serialize_str(&expr.to_string()),
            DuringExpression::Unexpected(s) => serializer.serialize_str(s),
        }
    }
}

impl<'de> Deserialize<'de> for DuringExpression {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match opening_hours_syntax::parse(&s) {
            Ok(expr) => Ok(DuringExpression::Osm(expr)),
            Err(_) => Ok(DuringExpression::Unexpected(s)),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SegmentVehicleDimension {
    AxleCount,
    Height,
    Length,
    Weight,
    Width,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SegmentVehicleComparator {
    GreaterThan,
    GreaterThanEqual,
    Equal,
    LessThan,
    LessThanEqual,
}

impl SegmentVehicleComparator {
    pub fn apply(&self, value: f64, restriction: f64) -> bool {
        match self {
            SegmentVehicleComparator::GreaterThan => value > restriction,
            SegmentVehicleComparator::GreaterThanEqual => value >= restriction,
            SegmentVehicleComparator::Equal => value == restriction,
            SegmentVehicleComparator::LessThan => value < restriction,
            SegmentVehicleComparator::LessThanEqual => value <= restriction,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SegmentUnit {
    Length(SegmentLengthUnit),
    Weight(SegmentWeightUnit),
}

impl SegmentUnit {
    pub fn as_str(&self) -> &'static str {
        match self {
            SegmentUnit::Length(SegmentLengthUnit::Inches) => "in",
            SegmentUnit::Length(SegmentLengthUnit::Feet) => "ft",
            SegmentUnit::Length(SegmentLengthUnit::Yard) => "yd",
            SegmentUnit::Length(SegmentLengthUnit::Mile) => "mi",
            SegmentUnit::Length(SegmentLengthUnit::Centimeter) => "cm",
            SegmentUnit::Length(SegmentLengthUnit::Meter) => "m",
            SegmentUnit::Length(SegmentLengthUnit::Kilometer) => "km",
            SegmentUnit::Weight(SegmentWeightUnit::Imperial(SegmentImperialWeightUnit::Ounce)) => {
                "oz"
            }
            SegmentUnit::Weight(SegmentWeightUnit::Imperial(SegmentImperialWeightUnit::Pound)) => {
                "lb"
            }
            SegmentUnit::Weight(SegmentWeightUnit::Imperial(SegmentImperialWeightUnit::Stone)) => {
                "st"
            }
            SegmentUnit::Weight(SegmentWeightUnit::Imperial(
                SegmentImperialWeightUnit::LongTon,
            )) => "lt",
            SegmentUnit::Weight(SegmentWeightUnit::Metric(SegmentMetricWeightUnit::Gram)) => "g",
            SegmentUnit::Weight(SegmentWeightUnit::Metric(SegmentMetricWeightUnit::Kilogram)) => {
                "kg"
            }
            SegmentUnit::Weight(SegmentWeightUnit::Metric(SegmentMetricWeightUnit::MetricTon)) => {
                "t"
            }
        }
    }
}

impl Serialize for SegmentUnit {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SegmentUnit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "in" => Ok(SegmentUnit::Length(SegmentLengthUnit::Inches)),
            "ft" => Ok(SegmentUnit::Length(SegmentLengthUnit::Feet)),
            "yd" => Ok(SegmentUnit::Length(SegmentLengthUnit::Yard)),
            "mi" => Ok(SegmentUnit::Length(SegmentLengthUnit::Mile)),
            "cm" => Ok(SegmentUnit::Length(SegmentLengthUnit::Centimeter)),
            "m" => Ok(SegmentUnit::Length(SegmentLengthUnit::Meter)),
            "km" => Ok(SegmentUnit::Length(SegmentLengthUnit::Kilometer)),
            "oz" => Ok(SegmentUnit::Weight(SegmentWeightUnit::Imperial(
                SegmentImperialWeightUnit::Ounce,
            ))),
            "lb" => Ok(SegmentUnit::Weight(SegmentWeightUnit::Imperial(
                SegmentImperialWeightUnit::Pound,
            ))),
            "st" => Ok(SegmentUnit::Weight(SegmentWeightUnit::Imperial(
                SegmentImperialWeightUnit::Stone,
            ))),
            "lt" => Ok(SegmentUnit::Weight(SegmentWeightUnit::Imperial(
                SegmentImperialWeightUnit::LongTon,
            ))),
            "g" => Ok(SegmentUnit::Weight(SegmentWeightUnit::Metric(
                SegmentMetricWeightUnit::Gram,
            ))),
            "kg" => Ok(SegmentUnit::Weight(SegmentWeightUnit::Metric(
                SegmentMetricWeightUnit::Kilogram,
            ))),
            "t" => Ok(SegmentUnit::Weight(SegmentWeightUnit::Metric(
                SegmentMetricWeightUnit::MetricTon,
            ))),
            "" => Ok(SegmentUnit::Length(SegmentLengthUnit::Meter)),
            other => Err(serde::de::Error::custom(format!(
                "unknown SegmentUnit '{other}'"
            ))),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum SegmentLengthUnit {
    #[serde(rename = "in")]
    Inches,
    #[serde(rename = "ft")]
    Feet,
    #[serde(rename = "yd")]
    Yard,
    #[serde(rename = "mi")]
    Mile,
    #[serde(rename = "cm")]
    Centimeter,
    #[serde(rename = "m")]
    Meter,
    #[serde(rename = "km")]
    Kilometer,
}

impl SegmentLengthUnit {
    pub fn to_uom(&self, value: f64) -> uom::si::f64::Length {
        match self {
            SegmentLengthUnit::Inches => uom::si::f64::Length::new::<uom::si::length::inch>(value),
            SegmentLengthUnit::Feet => uom::si::f64::Length::new::<uom::si::length::foot>(value),
            SegmentLengthUnit::Yard => uom::si::f64::Length::new::<uom::si::length::yard>(value),
            SegmentLengthUnit::Mile => uom::si::f64::Length::new::<uom::si::length::mile>(value),
            SegmentLengthUnit::Centimeter => {
                uom::si::f64::Length::new::<uom::si::length::centimeter>(value)
            }
            SegmentLengthUnit::Meter => uom::si::f64::Length::new::<uom::si::length::meter>(value),
            SegmentLengthUnit::Kilometer => {
                uom::si::f64::Length::new::<uom::si::length::kilometer>(value)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SegmentWeightUnit {
    Imperial(SegmentImperialWeightUnit),
    Metric(SegmentMetricWeightUnit),
}

impl SegmentWeightUnit {
    pub fn to_uom(&self, value: f64) -> uom::si::f64::Mass {
        use SegmentImperialWeightUnit as I;
        use SegmentMetricWeightUnit as M;
        use SegmentWeightUnit as SWU;

        match self {
            SWU::Imperial(I::Ounce) => uom::si::f64::Mass::new::<uom::si::mass::ounce>(value),
            SWU::Imperial(I::Pound) => uom::si::f64::Mass::new::<uom::si::mass::pound>(value),
            SWU::Imperial(I::Stone) => {
                uom::si::f64::Mass::new::<uom::si::mass::kilogram>(value * 6.350288)
            }
            SWU::Imperial(I::LongTon) => uom::si::f64::Mass::new::<uom::si::mass::ton_long>(value),
            SWU::Metric(M::Kilogram) => uom::si::f64::Mass::new::<uom::si::mass::kilogram>(value),
            SWU::Metric(M::Gram) => uom::si::f64::Mass::new::<uom::si::mass::gram>(value),
            SWU::Metric(M::MetricTon) => uom::si::f64::Mass::new::<uom::si::mass::ton>(value),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SegmentImperialWeightUnit {
    #[serde(rename = "oz")]
    Ounce,
    #[serde(rename = "lb")]
    Pound,
    #[serde(rename = "st")]
    Stone,
    #[serde(rename = "lt")]
    LongTon,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SegmentMetricWeightUnit {
    #[serde(rename = "g")]
    Gram,
    #[serde(rename = "kg")]
    Kilogram,
    #[serde(rename = "t")]
    MetricTon,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SegmentAccessType {
    Allowed,
    Denied,
    Designated,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SegmentAccessRestriction {
    pub access_type: SegmentAccessType,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub when: Option<SegmentAccessRestrictionWhen>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub vehicle: Option<String>,
}

impl SegmentAccessRestriction {
    pub fn contains_mode(&self, mode: &SegmentMode) -> bool {
        self.when
            .as_ref()
            .and_then(|w| w.mode.as_ref())
            .map(|m| m.contains(mode))
            .unwrap_or_default()
    }
}

fn default_none<T>() -> Option<T> {
    None
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SegmentAccessRestrictionWhen {
    #[serde(skip_serializing_if = "Option::is_none", default = "default_none")]
    pub during: Option<DuringExpression>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub heading: Option<SegmentHeading>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub using: Option<Vec<SegmentUsing>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub recognized: Option<Vec<SegmentRecognized>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub mode: Option<Vec<SegmentMode>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub vehicle: Option<Vec<SegmentAccessRestrictionWhenVehicle>>,
}

impl SegmentAccessRestrictionWhen {
    pub fn contains_mode(&self, mode: &SegmentMode) -> bool {
        self.mode
            .as_ref()
            .map(|m| m.contains(mode))
            .unwrap_or_default()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SegmentAccessRestrictionWhenVehicle {
    pub dimension: SegmentVehicleDimension,
    pub comparison: SegmentVehicleComparator,
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub unit: Option<SegmentUnit>,
}

impl SegmentAccessRestrictionWhenVehicle {
    pub fn is_valid(&self, when: &SegmentAccessRestrictionWhenVehicle) -> bool {
        use SegmentUnit as SU;

        if when.dimension != self.dimension {
            return false;
        }

        match (&self.unit, &when.unit) {
            (Some(SU::Length(this_unit)), Some(SU::Length(other_unit))) => {
                let this_value_f64 = this_unit.to_uom(self.value).get::<uom::si::length::meter>();
                let other_value_f64 = other_unit
                    .to_uom(when.value)
                    .get::<uom::si::length::meter>();
                self.comparison.apply(other_value_f64, this_value_f64)
            }
            (Some(SU::Weight(this_unit)), Some(SU::Weight(other_unit))) => {
                let this_value_f64 = this_unit
                    .to_uom(self.value)
                    .get::<uom::si::mass::kilogram>();
                let other_value_f64 = other_unit
                    .to_uom(when.value)
                    .get::<uom::si::mass::kilogram>();
                self.comparison.apply(other_value_f64, this_value_f64)
            }
            (Some(SU::Weight(_)), Some(SU::Length(_))) => false,
            (Some(SU::Length(_)), Some(SU::Weight(_))) => false,
            _ => self.comparison.apply(when.value, self.value),
        }
    }
}

/// Trait implemented by segment records (both OMF and OSM) to support shared
/// [`TravelModeFilter`] evaluation and mode-specific directional heading evaluation.
pub trait FilterableSegment {
    fn segment_subtype(&self) -> Option<&SegmentSubtype>;
    fn segment_class(&self) -> Option<&SegmentClass>;
    fn segment_subclass(&self) -> Option<&SegmentSubclass>;
    fn segment_access_restrictions(&self) -> Option<&[SegmentAccessRestriction]>;
}

impl<T: FilterableSegment + ?Sized> FilterableSegment for &T {
    fn segment_subtype(&self) -> Option<&SegmentSubtype> {
        (*self).segment_subtype()
    }

    fn segment_class(&self) -> Option<&SegmentClass> {
        (*self).segment_class()
    }

    fn segment_subclass(&self) -> Option<&SegmentSubclass> {
        (*self).segment_subclass()
    }

    fn segment_access_restrictions(&self) -> Option<&[SegmentAccessRestriction]> {
        (*self).segment_access_restrictions()
    }
}

/// Configures a predicate for testing whether a segment belongs to a specific travel mode.
#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(tag = "type")]
pub enum TravelModeFilter {
    #[serde(rename = "subtype")]
    MatchesSubtype { subtype: SegmentSubtype },
    #[serde(rename = "class")]
    MatchesClasses {
        classes: HashSet<SegmentClass>,
        behavior: MatchBehavior,
        allow_unset: bool,
    },
    #[serde(rename = "class_with_subclasses")]
    MatchesClassesWithSubclasses {
        classes: HashMap<SegmentClass, Vec<SegmentSubclass>>,
        behavior: MatchBehavior,
        allow_unset: bool,
    },
    #[serde(rename = "access_mode")]
    MatchesModeAccess { modes: Vec<SegmentMode> },
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MatchBehavior {
    Include,
    Exclude,
}

impl MatchBehavior {
    pub fn apply(&self, result: bool) -> bool {
        match self {
            MatchBehavior::Include => result,
            MatchBehavior::Exclude => !result,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ModeAccessState {
    DefaultAllowed,
    BlanketDenied,
    SpecificallyDenied,
    AllowedException,
}

#[derive(Clone, Debug)]
struct ModeAccessAccumulator {
    pub modes: Vec<SegmentMode>,
    pub state: ModeAccessState,
}

impl ModeAccessAccumulator {
    pub fn new(modes: &[SegmentMode]) -> Self {
        Self {
            modes: modes.to_vec(),
            state: ModeAccessState::DefaultAllowed,
        }
    }

    pub fn supports_mode(&self) -> bool {
        matches!(
            self.state,
            ModeAccessState::DefaultAllowed | ModeAccessState::AllowedException
        )
    }

    pub fn add_restriction(&mut self, r: &SegmentAccessRestriction) {
        let has_mode = r.when.as_ref().and_then(|x| {
            x.mode
                .as_ref()
                .map(|modes| modes.iter().any(|m| self.modes.contains(m)))
        });
        let heading = r.when.as_ref().and_then(|x| x.heading.clone());
        let mods = r
            .when
            .as_ref()
            .map(|x| x.recognized.is_some() || x.using.is_some());

        use SegmentAccessType as SAT;
        use SegmentHeading as SH;
        match (&r.access_type, has_mode, heading, mods) {
            // 1. Blanket Denial
            (SAT::Denied, None, None, None) if self.state == ModeAccessState::DefaultAllowed => {
                self.state = ModeAccessState::BlanketDenied;
            }
            // 2. Specific Denial overrides everything
            (SAT::Denied, Some(true), None | Some(SH::Forward), _) => {
                self.state = ModeAccessState::SpecificallyDenied;
            }
            // 3. Exception Allowed (if we were blanket denied, or just explicitly allowed)
            (SAT::Allowed | SAT::Designated, Some(true), None | Some(SH::Forward), None | Some(false))
                if self.state != ModeAccessState::SpecificallyDenied =>
            {
                self.state = ModeAccessState::AllowedException;
            }
            // 4. Conditional Exception (e.g. Employee only) -> deny access
            (SAT::Allowed | SAT::Designated, Some(true), None | Some(SH::Forward), Some(true))
                if self.state != ModeAccessState::SpecificallyDenied =>
            {
                self.state = ModeAccessState::SpecificallyDenied;
            }
            _ => {}
        }
    }
}

impl TravelModeFilter {
    /// Test whether a given segment matches this travel mode filter.
    pub fn matches_filter<S: FilterableSegment>(&self, segment: &S) -> bool {
        match self {
            TravelModeFilter::MatchesSubtype { subtype } => segment
                .segment_subtype()
                .map(|s| s == subtype)
                .unwrap_or_default(),

            TravelModeFilter::MatchesClasses {
                classes,
                behavior,
                allow_unset,
            } => segment
                .segment_class()
                .map(|c| behavior.apply(classes.contains(c)))
                .unwrap_or(*allow_unset),

            TravelModeFilter::MatchesClassesWithSubclasses {
                classes,
                behavior,
                allow_unset,
            } => match (segment.segment_class(), segment.segment_subclass()) {
                (Some(cl), None) => behavior.apply(classes.contains_key(cl)),
                (Some(cl), Some(sc)) => match classes.get(cl) {
                    None => *allow_unset,
                    Some(subclasses) => behavior.apply(subclasses.contains(sc)),
                },
                _ => *allow_unset,
            },

            TravelModeFilter::MatchesModeAccess { modes } => {
                let restrictions = segment.segment_access_restrictions().unwrap_or(&[]);
                let mut acc = ModeAccessAccumulator::new(modes);
                for r in restrictions {
                    acc.add_restriction(r);
                }
                acc.supports_mode()
            }
        }
    }

    fn ordering_value(&self) -> u64 {
        use TravelModeFilter as T;
        match self {
            T::MatchesSubtype { .. } => 0,
            T::MatchesClasses { .. } => 1,
            T::MatchesClassesWithSubclasses { .. } => 1,
            T::MatchesModeAccess { .. } => 2,
        }
    }
}

impl PartialEq for TravelModeFilter {
    fn eq(&self, other: &Self) -> bool {
        self.ordering_value().cmp(&other.ordering_value()).is_eq()
    }
}

impl PartialOrd for TravelModeFilter {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for TravelModeFilter {}

impl Ord for TravelModeFilter {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.ordering_value().cmp(&other.ordering_value())
    }
}

/// Evaluates the valid travel headings (`Forward`, `Backward`) on a segment for a given
/// `SegmentAccessRestrictionWhen` query and segment `access_restrictions` list.
pub fn evaluate_segment_headings(
    access_restrictions: Option<&[SegmentAccessRestriction]>,
    when: Option<&SegmentAccessRestrictionWhen>,
) -> Vec<SegmentHeading> {
    let no_restrictions = access_restrictions.map(|r| r.is_empty()).unwrap_or(true);
    if when.is_none() && (access_restrictions.is_none() || no_restrictions) {
        return vec![SegmentHeading::Forward, SegmentHeading::Backward];
    }

    let mut valid_headings = Vec::new();
    let when_heading = when.and_then(|w| w.heading.clone());
    let (test_fwd, test_bwd) = match when_heading {
        None => (true, true),
        Some(SegmentHeading::Forward) => (true, false),
        Some(SegmentHeading::Backward) => (false, true),
    };

    if test_fwd && is_heading_valid(SegmentHeading::Forward, when, access_restrictions) {
        valid_headings.push(SegmentHeading::Forward);
    }

    if test_bwd && is_heading_valid(SegmentHeading::Backward, when, access_restrictions) {
        valid_headings.push(SegmentHeading::Backward);
    }

    valid_headings
}

fn is_heading_valid(
    heading: SegmentHeading,
    when: Option<&SegmentAccessRestrictionWhen>,
    access_restrictions: Option<&[SegmentAccessRestriction]>,
) -> bool {
    use SegmentAccessType as SAT;

    let Some(restrictions) = access_restrictions else {
        return true;
    };

    let is_denied = |r: &&SegmentAccessRestriction| r.access_type == SAT::Denied;

    let (heading_specific, general): (Vec<_>, Vec<_>) = restrictions
        .iter()
        .filter(|r| restriction_applies_to(r, &heading, when))
        .partition(|r| r.when.as_ref().and_then(|w| w.heading.as_ref()).is_some());

    let (heading_denied, heading_allowed): (Vec<_>, Vec<_>) =
        heading_specific.into_iter().partition(is_denied);
    let (general_denied, general_allowed): (Vec<_>, Vec<_>) =
        general.into_iter().partition(is_denied);
    let has_heading_denied = !heading_denied.is_empty();
    let has_heading_allowed = !heading_allowed.is_empty();
    let no_general_denied = general_denied.is_empty();
    let has_general_allowed = !general_allowed.is_empty();

    if has_heading_denied {
        has_heading_allowed
    } else {
        has_heading_allowed || has_general_allowed || no_general_denied
    }
}

fn restriction_applies_to(
    restriction: &SegmentAccessRestriction,
    heading: &SegmentHeading,
    when: Option<&SegmentAccessRestrictionWhen>,
) -> bool {
    let restriction_when = restriction.when.as_ref();

    let heading_matches = restriction_when
        .and_then(|w| w.heading.as_ref())
        .map(|h| h == heading)
        .unwrap_or(true);

    if !heading_matches {
        return false;
    }

    if let Some(when) = when {
        when_is_compatible(when, restriction_when)
    } else {
        restriction_when.is_none()
            || restriction_when.is_some_and(|rw| {
                rw.mode.is_none() && rw.using.is_none() && rw.recognized.is_none()
            })
    }
}

fn when_is_compatible(
    when: &SegmentAccessRestrictionWhen,
    segment_restrictions: Option<&SegmentAccessRestrictionWhen>,
) -> bool {
    let Some(restrictions) = segment_restrictions else {
        return true;
    };

    if let Some(restriction_modes) = &restrictions.mode {
        if let Some(when_modes) = &when.mode {
            if !when_modes.iter().any(|m| restriction_modes.contains(m)) {
                return false;
            }
        } else {
            return false;
        }
    }

    if let Some(restriction_using) = &restrictions.using {
        if let Some(when_using) = &when.using {
            if !when_using.iter().all(|u| restriction_using.contains(u)) {
                return false;
            }
        } else {
            return false;
        }
    }

    if let Some(restriction_recognized) = &restrictions.recognized {
        if let Some(when_recognized) = &when.recognized {
            if !when_recognized
                .iter()
                .all(|r| restriction_recognized.contains(r))
            {
                return false;
            }
        } else {
            return false;
        }
    }

    if let Some(restrictions_vehicle) = &restrictions.vehicle {
        let all_restrictions_apply_to_all_vehicles = restrictions_vehicle.iter().all(|r_vehicle| {
            if let Some(when_vehicles) = &when.vehicle {
                when_vehicles
                    .iter()
                    .all(|w_vehicle| r_vehicle.is_valid(w_vehicle))
            } else {
                false
            }
        });

        if !all_restrictions_apply_to_all_vehicles {
            return false;
        }
    }

    true
}

/// Edge list filter and island detection configuration for a single travel mode.
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct NetworkEdgeListConfiguration {
    pub mode: String,
    pub filter: Vec<TravelModeFilter>,
    pub island_algorithm_config: Option<IslandDetectionAlgorithm>,
}

impl NetworkEdgeListConfiguration {
    /// Default walk-mode edge list configuration.
    pub fn default_walk() -> Self {
        Self {
            mode: "walk".to_string(),
            filter: vec![
                TravelModeFilter::MatchesSubtype {
                    subtype: SegmentSubtype::Road,
                },
                TravelModeFilter::MatchesClasses {
                    classes: HashSet::from([
                        SegmentClass::Secondary,
                        SegmentClass::Tertiary,
                        SegmentClass::Residential,
                        SegmentClass::LivingStreet,
                        SegmentClass::Unclassified,
                        SegmentClass::Service,
                        SegmentClass::Pedestrian,
                        SegmentClass::Footway,
                        SegmentClass::Steps,
                        SegmentClass::Path,
                        SegmentClass::Track,
                        SegmentClass::Bridleway,
                        SegmentClass::Unknown,
                    ]),
                    behavior: MatchBehavior::Include,
                    allow_unset: true,
                },
                TravelModeFilter::MatchesModeAccess {
                    modes: vec![SegmentMode::Foot],
                },
            ],
            island_algorithm_config: None,
        }
    }

    /// Default bike-mode edge list configuration.
    pub fn default_bike() -> Self {
        Self {
            mode: "bike".to_string(),
            filter: vec![
                TravelModeFilter::MatchesSubtype {
                    subtype: SegmentSubtype::Road,
                },
                TravelModeFilter::MatchesClasses {
                    classes: HashSet::from([
                        SegmentClass::Primary,
                        SegmentClass::Secondary,
                        SegmentClass::Tertiary,
                        SegmentClass::Residential,
                        SegmentClass::LivingStreet,
                        SegmentClass::Unclassified,
                        SegmentClass::Service,
                        SegmentClass::Pedestrian,
                        SegmentClass::Path,
                        SegmentClass::Track,
                        SegmentClass::Cycleway,
                        SegmentClass::Bridleway,
                        SegmentClass::Unknown,
                    ]),
                    behavior: MatchBehavior::Include,
                    allow_unset: true,
                },
                TravelModeFilter::MatchesModeAccess {
                    modes: vec![SegmentMode::Bicycle],
                },
            ],
            island_algorithm_config: None,
        }
    }

    /// Default drive-mode edge list configuration.
    pub fn default_drive() -> Self {
        Self {
            mode: "drive".to_string(),
            filter: vec![
                TravelModeFilter::MatchesSubtype {
                    subtype: SegmentSubtype::Road,
                },
                TravelModeFilter::MatchesClasses {
                    classes: HashSet::from([
                        SegmentClass::Motorway,
                        SegmentClass::Trunk,
                        SegmentClass::Primary,
                        SegmentClass::Secondary,
                        SegmentClass::Tertiary,
                        SegmentClass::Residential,
                        SegmentClass::LivingStreet,
                        SegmentClass::Unclassified,
                        SegmentClass::Service,
                        SegmentClass::Unknown,
                    ]),
                    behavior: MatchBehavior::Include,
                    allow_unset: true,
                },
                TravelModeFilter::MatchesModeAccess {
                    modes: vec![
                        SegmentMode::MotorVehicle,
                        SegmentMode::Car,
                        SegmentMode::Truck,
                        SegmentMode::Motorcycle,
                    ],
                },
            ],
            island_algorithm_config: None,
        }
    }

    /// Built-in preset configuration for `"walk"`, `"bike"`, or `"drive"`.
    pub fn default_for_mode(mode: &str) -> Result<Self, String> {
        match mode.trim().to_ascii_lowercase().as_str() {
            "walk" | "foot" | "pedestrian" => Ok(Self::default_walk()),
            "bike" | "bicycle" | "cycling" => Ok(Self::default_bike()),
            "drive" | "car" | "motor_vehicle" | "auto" => Ok(Self::default_drive()),
            other => Err(format!(
                "unsupported travel mode preset '{other}'; expected one of: walk, bike, drive"
            )),
        }
    }

    /// Default multimodal (`["walk", "bike", "drive"]`) edge list configurations.
    pub fn default_multimodal() -> Vec<Self> {
        vec![
            Self::default_walk(),
            Self::default_bike(),
            Self::default_drive(),
        ]
    }
}

impl From<&NetworkEdgeListConfiguration> for SegmentAccessRestrictionWhen {
    fn from(value: &NetworkEdgeListConfiguration) -> Self {
        let user_modes_opt = value.filter.iter().find_map(|f| match f {
            TravelModeFilter::MatchesModeAccess { modes } => Some(modes.clone()),
            _ => None,
        });
        let mut result = SegmentAccessRestrictionWhen::default();
        if let Some(modes) = user_modes_opt {
            result.mode = Some(modes);
        }
        result
    }
}
