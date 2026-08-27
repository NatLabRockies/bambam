use itertools::Itertools;
use serde::{Deserialize, Serialize};

pub type VehicleTypeId = String;

/// geofencing_zones "rules" object that contains logical rules.
#[derive(Default, Clone, Debug)]
pub struct ZoneConstraints {
    /// Is the ride allowed to start in this zone?
    pub ride_start_allowed: Option<bool>,
    /// Is the ride allowed to end in this zone?
    pub ride_end_allowed: Option<bool>,
    /// Is the ride allowed to travel through this zone?
    pub ride_through_allowed: Option<bool>,
    /// What is the maximum speed allowed, in kilometers per hour?
    pub maximum_speed_kph: Option<i32>,
    /// Can vehicles only be parked at stations defined in [station_information] within this geofence zone?
    pub station_parking: Option<bool>,
    /// Array of IDs of vehicle types for which any restrictions SHOULD be applied.
    /// If vehicle type IDs are not specified, then restrictions apply to all vehicle types.
    pub vehicle_type_ids: Option<Vec<VehicleTypeId>>,
}

impl ZoneConstraints {
    /// default behavior if no Rules are encountered for a feature.
    pub fn allow_all() -> Self {
        Self {
            ride_start_allowed: Some(true),
            ride_end_allowed: Some(true),
            ride_through_allowed: Some(true),
            maximum_speed_kph: Some(i32::MAX),
            station_parking: Some(true),
            vehicle_type_ids: None,
        }
    }

    /// converts v2_3 Rules into ZoneConstraints for either a zone or for a global ruleset.
    pub fn from_v2_3(
        rules: Option<&Vec<gbfs_types::v2_3::files::geofencing_zones::Rule>>,
    ) -> Vec<ZoneConstraints> {
        match rules {
            Some(rs) => rs.iter().map(|r| r.into()).collect_vec(),
            None => vec![],
        }
    }

    /// converts v3_0 Rules into ZoneConstraints for either a zone or for a global ruleset.
    pub fn from_v3_0(
        rules: Option<&Vec<gbfs_types::v3_0::files::geofencing_zones::Rule>>,
    ) -> Vec<ZoneConstraints> {
        match rules {
            Some(rs) => rs.iter().map(|r| r.into()).collect_vec(),
            None => vec![],
        }
    }

    /// from a list of constraints, merge according to the rules of precedence found at
    /// <https://github.com/MobilityData/gbfs/blob/master/gbfs.md#geofencing-rule-precedence>.
    ///
    /// here, we ensure that:
    ///   - When multiple rules in the same array apply to a particular vehicle type, per the
    /// semantics of the vehicle_type_ids field, then the earlier rule (in order of the JSON file)
    /// takes precedence for that vehicle type.
    ///   - When a polygon and the global_rules field define rules that apply to a particular
    /// vehicle type, then the rules from the polygon take precedence for that vehicle type
    /// in the area of the polygon.
    pub fn merge_constraints(
        global_constraints: &[ZoneConstraints],
        constraints: &[ZoneConstraints],
        for_vehicle_type: Option<&VehicleTypeId>,
    ) -> Option<ZoneConstraints> {
        let iter: Box<dyn Iterator<Item = &ZoneConstraints>> =
            match (global_constraints, constraints) {
                ([], []) => return None,
                (glob, []) => Box::new(glob.iter()),
                ([], zone) => Box::new(zone.iter()),
                (glob, zone) => Box::new(zone.iter().chain(glob.iter())),
            };

        let mut accumulator = Self::default();
        for c in iter {
            if matches_accumulator(c, for_vehicle_type) {
                accumulator.append(c);
            }
        }
        Some(accumulator)
    }

    /// appends the values of another set of constraints onto this one.
    ///
    /// per documentation:
    ///
    /// > When multiple rules in the same array apply to a particular vehicle type,
    /// > per the semantics of the vehicle_type_ids field, then the earlier rule
    /// > (in order of the JSON file) takes precedence for that vehicle type.
    ///
    /// see <https://github.com/MobilityData/gbfs/blob/master/gbfs.md#geofencing-rule-precedence>
    fn append(&mut self, other: &ZoneConstraints) {
        self.maximum_speed_kph =
            merge_no_overwrite(self.maximum_speed_kph, other.maximum_speed_kph);
        self.ride_start_allowed =
            merge_no_overwrite(self.ride_start_allowed, other.ride_start_allowed);
        self.ride_end_allowed = merge_no_overwrite(self.ride_end_allowed, other.ride_end_allowed);
        self.ride_through_allowed =
            merge_no_overwrite(self.ride_through_allowed, other.ride_through_allowed);
        self.station_parking = merge_no_overwrite(self.station_parking, other.station_parking);
    }
}

/// the subset of VehicleType related to route planning. ignored fields are commented out.
/// taken from gbfs_types::v3_0::files::vehicle_types.
#[allow(unused)]
#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VehicleTypeConstraints {
    /// Unique identifier of a vehicle type.
    pub vehicle_type_id: VehicleTypeId,
    // /// The vehicle's general form factor.
    // pub form_factor: Option<String>,
    // /// The number of riders (driver included) the vehicle can legally accommodate.
    // pub rider_capacity: Option<u8>,
    // /// Cargo volume available in the vehicle, expressed in liters. For cars, it corresponds to the space between the boot floor, including the storage under the hatch, to the rear shelf in the trunk.
    // pub cargo_volume_capacity: Option<i32>,
    // /// The capacity of the vehicle cargo space (excluding passengers), expressed in kilograms.
    // pub cargo_load_capacity: Option<f64>,
    // /// The primary propulsion type of the vehicle.
    // pub propulsion_type: Option<String>,
    // // /// Vehicle air quality certificate. Official anti-pollution certificate, based on the information on the vehicle's registration certificate, attesting to its level of pollutant emissions based on a defined standard. In Europe, for example, it is the European emission standard. The aim of this measure is to encourage the use of the least polluting vehicles by allowing them to drive during pollution peaks or in low emission zones.
    // // pub eco_labels: Option<Vec<EcoLabel>>,
    // /// This represents the furthest distance in meters that the vehicle can travel without recharging or refueling when it has the maximum amount of energy potential (for example, a full battery or full tank of gas).
    pub max_range_meters: Option<f64>,
    /// The public name of this vehicle type.
    pub name: Option<Vec<String>>,
    // /// Description of accessories available in the vehicle. These accessories are part of the vehicle and are not supposed to change frequently.
    // pub vehicle_accessories: Option<Vec<String>>,
    // // /// Maximum quantity of CO2, in grams, emitted per kilometer, according to the [WLTP](https://en.wikipedia.org/wiki/Worldwide_Harmonised_Light_Vehicles_Test_Procedure).
    // // pub g_CO2_km: Option<i64>,
    // /// URL to an image that would assist the user in identifying the vehicle (for example, an image of the vehicle or a logo).
    // pub vehicle_image: Option<String>,
    // /// The name of the vehicle manufacturer.
    // pub make: Option<Vec<String>>,
    // /// The name of the vehicle model.
    // pub model: Option<Vec<String>>,
    // /// The color of the vehicle.
    // pub color: Option<String>,
    // /// Customer-readable description of the vehicle type outlining special features or how-tos.
    // pub description: Option<Vec<String>>,
    // /// Number of wheels this vehicle type has.
    // pub wheel_count: Option<u8>,
    /// The maximum speed in kilometers per hour this vehicle is permitted to reach in accordance with local permit and regulations.
    pub max_permitted_speed: Option<u8>,
    // /// The rated power of the motor for this vehicle type in watts.
    // pub rated_power: Option<u32>,
    // /// Maximum time in minutes that a vehicle can be reserved before a rental begins.
    // /// If default_reserve_time is set to 0, the vehicle type cannot be reserved.
    // pub default_reserve_time: Option<u32>,
    // /// The conditions for returning the vehicle at the end of the rental.
    // pub return_constraint: Option<String>,
    // pub vehicle_assets: Option<VehicleAsset>,
    // /// A plan_id, as defined in system_pricing_plans.json, that identifies a default pricing plan for this vehicle to be used by trip planning applications for purposes of calculating the cost of a single trip using this vehicle type.
    // /// This default pricing plan is superseded by `pricing_plan_id` when `pricing_plan_id` is defined in `vehicle_status.json`.
    // pub default_pricing_plan_id: Option<PricingPlanID>,
    // /// All pricing plan IDs that are applied to this vehicle type.
    // pub pricing_plan_ids: Option<Vec<PricingPlanID>>,
}

impl From<&gbfs_types::v2_3::files::geofencing_zones::Rule> for ZoneConstraints {
    fn from(value: &gbfs_types::v2_3::files::geofencing_zones::Rule) -> Self {
        Self {
            vehicle_type_ids: value.vehicle_type_id.clone(),
            ride_start_allowed: Some(value.ride_allowed),
            ride_end_allowed: Some(value.ride_allowed),
            ride_through_allowed: Some(value.ride_through_allowed),
            maximum_speed_kph: value.maximum_speed_kph,
            station_parking: value.station_parking,
        }
    }
}

impl From<&gbfs_types::v3_0::files::geofencing_zones::Rule> for ZoneConstraints {
    fn from(value: &gbfs_types::v3_0::files::geofencing_zones::Rule) -> Self {
        Self {
            vehicle_type_ids: value.vehicle_type_ids.clone(),
            ride_start_allowed: Some(value.ride_start_allowed),
            ride_end_allowed: Some(value.ride_end_allowed),
            ride_through_allowed: Some(value.ride_through_allowed),
            maximum_speed_kph: value.maximum_speed_kph,
            station_parking: value.station_parking,
        }
    }
}

impl From<&super::gbfs_v2_2::types::GeofenceRules> for ZoneConstraints {
    fn from(value: &super::gbfs_v2_2::types::GeofenceRules) -> Self {
        Self {
            vehicle_type_ids: value.vehicle_type_id.clone(),
            ride_start_allowed: Some(value.ride_allowed),
            ride_end_allowed: Some(value.ride_allowed),
            ride_through_allowed: Some(value.ride_through_allowed),
            maximum_speed_kph: value.maximum_speed_kph,
            station_parking: None,
        }
    }
}

/// helper for testing if the accumulator's vehicle type argument matches the constraint set.
fn matches_accumulator(
    constraint: &ZoneConstraints,
    for_vehicle_type: Option<&VehicleTypeId>,
) -> bool {
    match (for_vehicle_type, &constraint.vehicle_type_ids) {
        (None, None) => true,
        (None, Some(_)) => false,
        (Some(_), None) => false,
        (Some(acc_type), Some(c_types)) => c_types.contains(acc_type),
    }
}

/// per documentation:
///
/// > When multiple rules in the same array apply to a particular vehicle type,
/// > per the semantics of the vehicle_type_ids field, then the earlier rule
/// > (in order of the JSON file) takes precedence for that vehicle type.
///
/// see <https://github.com/MobilityData/gbfs/blob/master/gbfs.md#geofencing-rule-precedence>s
fn merge_no_overwrite<T>(lhs: Option<T>, rhs: Option<T>) -> Option<T> {
    match (lhs, rhs) {
        (None, None) => None,
        (None, Some(r)) => Some(r),
        (Some(l), None) => Some(l),
        (Some(l), Some(_)) => Some(l),
    }
}
