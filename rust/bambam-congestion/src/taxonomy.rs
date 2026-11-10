use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The 6 operating speed time-of-day periods modeled in BAMBAM congestion calibration and imputation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TimePeriod {
    /// Weekday Morning Peak (`wd-am`): `Day: Weekday = 1`, `Hour: AM Peak = 1`, `Hour: PM Peak = 0`
    WdAm,
    /// Weekday Afternoon / Midday (`wd-af`): `Day: Weekday = 1`, `Hour: AM Peak = 0`, `Hour: PM Peak = 0`
    WdAf,
    /// Weekday Evening Peak (`wd-pm`): `Day: Weekday = 1`, `Hour: AM Peak = 0`, `Hour: PM Peak = 1`
    WdPm,
    /// Weekend Morning (`we-am`): `Day: Weekday = 0`, `Hour: AM Peak = 1`, `Hour: PM Peak = 0`
    WeAm,
    /// Weekend Afternoon (`we-af`): `Day: Weekday = 0`, `Hour: AM Peak = 0`, `Hour: PM Peak = 0`
    WeAf,
    /// Weekend Evening (`we-pm`): `Day: Weekday = 0`, `Hour: AM Peak = 0`, `Hour: PM Peak = 1`
    WePm,
}

impl TimePeriod {
    pub const ALL: [TimePeriod; 6] = [
        Self::WdAm,
        Self::WdAf,
        Self::WdPm,
        Self::WeAm,
        Self::WeAf,
        Self::WePm,
    ];

    /// Canonical kebab-case identifier (`wd-am`, `wd-af`, `wd-pm`, `we-am`, `we-af`, `we-pm`).
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::WdAm => "wd-am",
            Self::WdAf => "wd-af",
            Self::WdPm => "wd-pm",
            Self::WeAm => "we-am",
            Self::WeAf => "we-af",
            Self::WePm => "we-pm",
        }
    }

    /// Returns `(day_weekday, hour_am_peak, hour_pm_peak)` indicator flags for XGBoost.
    pub const fn time_flags(&self) -> (f32, f32, f32) {
        match self {
            Self::WdAm => (1.0, 1.0, 0.0),
            Self::WdAf => (1.0, 0.0, 0.0),
            Self::WdPm => (1.0, 0.0, 1.0),
            Self::WeAm => (0.0, 1.0, 0.0),
            Self::WeAf => (0.0, 0.0, 0.0),
            Self::WePm => (0.0, 0.0, 1.0),
        }
    }

    /// Standard RouteE Compass enumerated speed filename for this time period.
    pub fn enumerated_speed_filename(&self) -> String {
        format!("edges-speeds-{}-mph-enumerated.txt.gz", self.as_str())
    }
}

impl fmt::Display for TimePeriod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TimePeriod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "wd-am" => Ok(Self::WdAm),
            "wd-af" => Ok(Self::WdAf),
            "wd-pm" => Ok(Self::WdPm),
            "we-am" => Ok(Self::WeAm),
            "we-af" => Ok(Self::WeAf),
            "we-pm" => Ok(Self::WePm),
            other => Err(format!(
                "unknown time period '{other}', expected one of: wd-am, wd-af, wd-pm, we-am, we-af, we-pm"
            )),
        }
    }
}

/// The 9 consolidated road functional classes used in the congestion calibration and imputation models.
///
/// Matches the `prepare_data_for_model` / `prepare_data_for_imputation` categorization in
/// `calibration_utils.py` and `models.py`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CongestionRoadClass {
    Residential,
    Motorway,
    Trunk,
    Primary,
    Secondary,
    Tertiary,
    Service,
    /// `motorway_link` and `trunk_link`
    Link,
    /// `primary_link`, `secondary_link`, `tertiary_link`, `unclassified`, `living_street`, `track`, `road`, etc.
    Other,
}

impl CongestionRoadClass {
    /// Parses a road class from either an OSM highway tag (`"motorway"`, `"motorway_link"`, etc.)
    /// or a `bambam-network` `SegmentFullType` string (`"road-motorway"`, `"road-motorway-link"`,
    /// `"road-service-driveway"`, etc.).
    pub fn from_class_str(raw: &str) -> Self {
        let normalized = normalize_highway_tag(raw);
        match normalized.as_str() {
            "motorway" => Self::Motorway,
            "trunk" => Self::Trunk,
            "motorway_link" | "trunk_link" => Self::Link,
            "primary" => Self::Primary,
            "secondary" => Self::Secondary,
            "tertiary" => Self::Tertiary,
            "residential" => Self::Residential,
            "service" => Self::Service,
            _ => Self::Other,
        }
    }

    /// Returns `true` if this road class is a high-functional class that uses TomTom calibration
    /// and XGBoost speed prediction (`Motorway`, `Trunk`, `Link`, `Primary`, `Secondary`, `Tertiary`).
    ///
    /// Low-functional classes (`Residential`, `Service`, `Other`) use imputed speed limits (`sl-med-imp`).
    pub const fn is_high_functional(&self) -> bool {
        matches!(
            self,
            Self::Motorway
                | Self::Trunk
                | Self::Link
                | Self::Primary
                | Self::Secondary
                | Self::Tertiary
        )
    }

    /// Returns `true` if this road class is `Link` (`motorway_link` or `trunk_link`).
    ///
    /// In both `models.py` and `predict_speeds_using_models.py`, `Road Class: Link` masks
    /// `Speed Limit` to `NaN` so it is always evaluated by the `without_speedlimit` model.
    pub const fn is_link(&self) -> bool {
        matches!(self, Self::Link)
    }

    /// Returns the 8 one-hot road class indicator flags in the exact column order expected by XGBoost:
    /// `[Motorway, Trunk, Primary, Secondary, Tertiary, Service, Link, Other]`.
    /// (`Residential` is the reference category with all 8 flags set to `0.0`).
    pub const fn one_hot_8(&self) -> [f32; 8] {
        match self {
            Self::Motorway => [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            Self::Trunk => [0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            Self::Primary => [0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            Self::Secondary => [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0],
            Self::Tertiary => [0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0],
            Self::Service => [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            Self::Link => [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            Self::Other => [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            Self::Residential => [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        }
    }

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Residential => "residential",
            Self::Motorway => "motorway",
            Self::Trunk => "trunk",
            Self::Primary => "primary",
            Self::Secondary => "secondary",
            Self::Tertiary => "tertiary",
            Self::Service => "service",
            Self::Link => "link",
            Self::Other => "other",
        }
    }
}

/// Normalizes a `bambam-network` `SegmentFullType` string (e.g. `"road-motorway-link"`,
/// `"road-service-driveway"`, `"road-primary"`) or raw OSM highway string (e.g. `"motorway_link"`)
/// into the canonical 15 drive highway strings used for grouping `sl-med-imp` median speeds
/// and max speed limits per highway type.
pub fn normalize_highway_tag(raw: &str) -> String {
    let s = raw.trim().to_ascii_lowercase();
    let stripped = s.strip_prefix("road-").unwrap_or(&s);

    // Handle "<class>-link" from SegmentFullType (e.g. "motorway-link" -> "motorway_link")
    if let Some(base) = stripped.strip_suffix("-link") {
        let link_name = format!("{}_link", base.replace('-', "_"));
        return link_name;
    }

    // Handle service subclasses like "service-driveway", "service-parking_aisle", "service-alley"
    if stripped.starts_with("service-") {
        return "service".to_string();
    }

    // Also handle Python-style list/composite strings if present (matching `prepare_osm_network`)
    const DRIVE_HIGHWAYS: [&str; 15] = [
        "motorway_link",
        "motorway",
        "trunk_link",
        "trunk",
        "primary_link",
        "primary",
        "secondary_link",
        "secondary",
        "tertiary_link",
        "tertiary",
        "residential",
        "service",
        "track",
        "road",
        "unclassified",
    ];
    let underscored = stripped.replace('-', "_");
    if DRIVE_HIGHWAYS.contains(&underscored.as_str()) {
        return underscored;
    }
    for key in DRIVE_HIGHWAYS {
        if underscored.contains(key) {
            return key.to_string();
        }
    }
    underscored
}

/// Static (time-independent) tabular features for a single road edge.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EdgeStaticFeatures {
    /// Posted speed limit in mph (`None` if missing or if `road_class == CongestionRoadClass::Link`).
    pub model_speed_limit_mph: Option<f32>,
    /// Imputed speed limit (`sl-med-imp`, floored at `15.0 mph`), used as fallback for low-functional roads.
    pub imputed_speed_limit_mph: f32,
    /// Population density within 0.5-mile buffer (`1,000 persons / sq mi`).
    pub pop_density_1000_per_sq_mi: f32,
    /// Employment density within 0.5-mile buffer (`1,000 jobs / sq mi`).
    pub emp_density_1000_per_sq_mi: f32,
    /// Consolidated road class.
    pub road_class: CongestionRoadClass,
    /// Sum of 5 MEP opportunity category densities within 0.5-mile buffer (`opportunities / sq mi`).
    pub opp_density_per_sq_mi: f32,
    /// Sum of traffic signal, crossing, and stop sign densities within 0.5-mile buffer (`controls / sq mi`).
    pub traffic_control_density_per_sq_mi: f32,
}

impl EdgeStaticFeatures {
    /// Builds the 16-feature vector for `model_with_speedlimit` for the given `TimePeriod`:
    ///
    /// `[Speed Limit, Population Density, Employment Density, Day: Weekday, Hour: AM Peak, Hour: PM Peak,
    ///   Motorway, Trunk, Primary, Secondary, Tertiary, Service, Link, Other,
    ///   Opp Density, Traffic Signal/Crossing/Traffic Stop Density]`
    pub fn to_features_with_speedlimit(
        &self,
        period: TimePeriod,
        speed_limit_mph: f32,
    ) -> [f32; 16] {
        let (day_wd, hr_am, hr_pm) = period.time_flags();
        let rc = self.road_class.one_hot_8();
        [
            speed_limit_mph,
            self.pop_density_1000_per_sq_mi,
            self.emp_density_1000_per_sq_mi,
            day_wd,
            hr_am,
            hr_pm,
            rc[0],
            rc[1],
            rc[2],
            rc[3],
            rc[4],
            rc[5],
            rc[6],
            rc[7],
            self.opp_density_per_sq_mi,
            self.traffic_control_density_per_sq_mi,
        ]
    }

    /// Builds the 15-feature vector for `model_without_speedlimit` for the given `TimePeriod`:
    ///
    /// `[Population Density, Employment Density, Day: Weekday, Hour: AM Peak, Hour: PM Peak,
    ///   Motorway, Trunk, Primary, Secondary, Tertiary, Service, Link, Other,
    ///   Opp Density, Traffic Signal/Crossing/Traffic Stop Density]`
    pub fn to_features_without_speedlimit(&self, period: TimePeriod) -> [f32; 15] {
        let (day_wd, hr_am, hr_pm) = period.time_flags();
        let rc = self.road_class.one_hot_8();
        [
            self.pop_density_1000_per_sq_mi,
            self.emp_density_1000_per_sq_mi,
            day_wd,
            hr_am,
            hr_pm,
            rc[0],
            rc[1],
            rc[2],
            rc[3],
            rc[4],
            rc[5],
            rc[6],
            rc[7],
            self.opp_density_per_sq_mi,
            self.traffic_control_density_per_sq_mi,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_and_classify_road_classes() {
        assert_eq!(normalize_highway_tag("road-motorway-link"), "motorway_link");
        assert_eq!(
            CongestionRoadClass::from_class_str("road-motorway-link"),
            CongestionRoadClass::Link
        );
        assert_eq!(
            CongestionRoadClass::from_class_str("trunk_link"),
            CongestionRoadClass::Link
        );
        // Note: in calibration_utils.py, primary_link/secondary_link/tertiary_link map to Other
        assert_eq!(
            CongestionRoadClass::from_class_str("road-primary-link"),
            CongestionRoadClass::Other
        );
        assert_eq!(
            CongestionRoadClass::from_class_str("road-service-driveway"),
            CongestionRoadClass::Service
        );
        assert_eq!(
            CongestionRoadClass::from_class_str("road-primary"),
            CongestionRoadClass::Primary
        );
        assert!(CongestionRoadClass::Motorway.is_high_functional());
        assert!(CongestionRoadClass::Link.is_high_functional());
        assert!(!CongestionRoadClass::Residential.is_high_functional());
        assert!(!CongestionRoadClass::Service.is_high_functional());
        assert!(!CongestionRoadClass::Other.is_high_functional());
    }
}
