use serde::{Deserialize, Serialize};

pub type VehicleTypeId = String;

/// geofencing rules object
pub struct FeatureRules {
    /// Array of IDs of vehicle types for which any restrictions SHOULD be applied.
    /// If vehicle type IDs are not specified, then restrictions apply to all vehicle types.
    pub vehicle_type_ids: Option<Vec<VehicleTypeId>>,
    /// Is the ride allowed to start in this zone?
    pub ride_start_allowed: bool,
    /// Is the ride allowed to end in this zone?
    pub ride_end_allowed: bool,
    /// Is the ride allowed to travel through this zone?
    pub ride_through_allowed: bool,
    /// What is the maximum speed allowed, in kilometers per hour?
    ///
    /// If there is no maximum speed to observe, this can be omitted.
    pub maximum_speed_kph: Option<i32>,
    /// Can vehicles only be parked at stations defined in [station_information] within this geofence zone?
    pub station_parking: Option<bool>,
}

#[serde_with::skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone)]
/// Vehicle that is currently deployed in the field
pub struct VehicleType {
    /// Unique identifier of a vehicle type.
    pub vehicle_type_id: VehicleTypeId,
    /// The vehicle's general form factor.
    pub form_factor: Option<String>,
    /// The number of riders (driver included) the vehicle can legally accommodate.
    pub rider_capacity: Option<u8>,
    /// Cargo volume available in the vehicle, expressed in liters. For cars, it corresponds to the space between the boot floor, including the storage under the hatch, to the rear shelf in the trunk.
    pub cargo_volume_capacity: Option<i32>,
    /// The capacity of the vehicle cargo space (excluding passengers), expressed in kilograms.
    pub cargo_load_capacity: Option<f64>,
    /// The primary propulsion type of the vehicle.
    pub propulsion_type: Option<String>,
    // /// Vehicle air quality certificate. Official anti-pollution certificate, based on the information on the vehicle's registration certificate, attesting to its level of pollutant emissions based on a defined standard. In Europe, for example, it is the European emission standard. The aim of this measure is to encourage the use of the least polluting vehicles by allowing them to drive during pollution peaks or in low emission zones.
    // pub eco_labels: Option<Vec<EcoLabel>>,
    /// This represents the furthest distance in meters that the vehicle can travel without recharging or refueling when it has the maximum amount of energy potential (for example, a full battery or full tank of gas).
    pub max_range_meters: Option<f64>,
    /// The public name of this vehicle type.
    pub name: Option<Vec<String>>,
    /// Description of accessories available in the vehicle. These accessories are part of the vehicle and are not supposed to change frequently.
    pub vehicle_accessories: Option<Vec<String>>,
    // /// Maximum quantity of CO2, in grams, emitted per kilometer, according to the [WLTP](https://en.wikipedia.org/wiki/Worldwide_Harmonised_Light_Vehicles_Test_Procedure).
    // pub g_CO2_km: Option<i64>,
    /// URL to an image that would assist the user in identifying the vehicle (for example, an image of the vehicle or a logo).
    pub vehicle_image: Option<String>,
    /// The name of the vehicle manufacturer.
    pub make: Option<Vec<String>>,
    /// The name of the vehicle model.
    pub model: Option<Vec<String>>,
    /// The color of the vehicle.
    pub color: Option<String>,
    /// Customer-readable description of the vehicle type outlining special features or how-tos.
    pub description: Option<Vec<String>>,
    /// Number of wheels this vehicle type has.
    pub wheel_count: Option<u8>,
    /// The maximum speed in kilometers per hour this vehicle is permitted to reach in accordance with local permit and regulations.
    pub max_permitted_speed: Option<u8>,
    /// The rated power of the motor for this vehicle type in watts.
    pub rated_power: Option<u32>,
    /// Maximum time in minutes that a vehicle can be reserved before a rental begins.
    /// If default_reserve_time is set to 0, the vehicle type cannot be reserved.
    pub default_reserve_time: Option<u32>,
    /// The conditions for returning the vehicle at the end of the rental.
    pub return_constraint: Option<String>,
    // pub vehicle_assets: Option<VehicleAsset>,
    // /// A plan_id, as defined in system_pricing_plans.json, that identifies a default pricing plan for this vehicle to be used by trip planning applications for purposes of calculating the cost of a single trip using this vehicle type.
    // /// This default pricing plan is superseded by `pricing_plan_id` when `pricing_plan_id` is defined in `vehicle_status.json`.
    // pub default_pricing_plan_id: Option<PricingPlanID>,
    // /// All pricing plan IDs that are applied to this vehicle type.
    // pub pricing_plan_ids: Option<Vec<PricingPlanID>>,
}
