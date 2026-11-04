//! Phase 1 raw CSV wire structures for standard GTFS schedule tables.

use crate::enums::{
    Availability, BikesAllowedType, ContinuousPickupDropOff, DirectionType, Exception,
    LocationType, PathwayDirectionType, PathwayMode, PickupDropOffType, RouteType, TimepointType,
    TransferType,
};
use crate::ids::*;
use crate::primitives::*;
use chrono::NaiveDate;
use rgb::RGB8;

/// Raw row from `agency.txt`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawAgency {
    /// Unique agency identifier (optional when feed has a single agency).
    #[serde(rename = "agency_id")]
    pub id: Option<AgencyId>,
    /// Full name of the transit agency.
    #[serde(rename = "agency_name")]
    pub name: String,
    /// URL of the transit agency.
    #[serde(rename = "agency_url")]
    pub url: String,
    /// IANA timezone where the transit agency is located (`agency_timezone`).
    #[serde(
        rename = "agency_timezone",
        deserialize_with = "deserialize_timezone",
        serialize_with = "serialize_timezone"
    )]
    pub timezone: Tz,
    /// Primary language used by this transit agency.
    #[serde(rename = "agency_lang")]
    pub lang: Option<String>,
    /// Voice telephone number for the specified agency.
    #[serde(rename = "agency_phone")]
    pub phone: Option<String>,
    /// URL of a web page that allows a rider to purchase tickets online.
    #[serde(rename = "agency_fare_url")]
    pub fare_url: Option<String>,
    /// Email address actively monitored by the agency’s customer service department.
    #[serde(rename = "agency_email")]
    pub email: Option<String>,
}

impl Default for RawAgency {
    fn default() -> Self {
        Self {
            id: None,
            name: String::new(),
            url: String::new(),
            timezone: Tz::UTC,
            lang: None,
            phone: None,
            fare_url: None,
            email: None,
        }
    }
}

/// Raw row from `stops.txt`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RawStop {
    /// Unique identifier for the stop, station, or node.
    #[serde(rename = "stop_id")]
    pub id: StopId,
    /// Short text or a number that identifies the location for riders.
    #[serde(rename = "stop_code")]
    pub code: Option<String>,
    /// Name of the location.
    #[serde(rename = "stop_name")]
    pub name: Option<String>,
    /// Description of the location.
    #[serde(default, rename = "stop_desc")]
    pub description: Option<String>,
    /// Type of the location (stop/platform, station, entrance, generic node, boarding area).
    #[serde(default)]
    pub location_type: LocationType,
    /// Parent station identifier if this stop is inside a station.
    pub parent_station: Option<StopId>,
    /// Fare zone identifier for this stop.
    pub zone_id: Option<ZoneId>,
    /// URL of a web page about the location.
    #[serde(rename = "stop_url")]
    pub url: Option<String>,
    /// Longitude of the stop in WGS84 decimal degrees.
    #[serde(
        deserialize_with = "de_with_optional_float",
        serialize_with = "serialize_float_as_str",
        rename = "stop_lon",
        default
    )]
    pub longitude: Option<f64>,
    /// Latitude of the stop in WGS84 decimal degrees.
    #[serde(
        deserialize_with = "de_with_optional_float",
        serialize_with = "serialize_float_as_str",
        rename = "stop_lat",
        default
    )]
    pub latitude: Option<f64>,
    /// Optional IANA timezone override for the location (`stop_timezone`).
    #[serde(
        rename = "stop_timezone",
        deserialize_with = "deserialize_option_timezone",
        serialize_with = "serialize_option_timezone",
        default
    )]
    pub timezone: Option<Tz>,
    /// Indicates whether wheelchair boardings are possible from the location.
    #[serde(default)]
    pub wheelchair_boarding: Availability,
    /// Level of the location.
    pub level_id: Option<LevelId>,
    /// Platform identifier for a platform stop.
    pub platform_code: Option<String>,
    /// Text-to-speech readable version of `stop_name`.
    #[serde(rename = "tts_stop_name")]
    pub tts_name: Option<String>,
}

/// Raw row from `routes.txt`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RawRoute {
    /// Unique identifier for the route.
    #[serde(rename = "route_id")]
    pub id: RouteId,
    /// Short name of a route (e.g. "32" or "Green").
    #[serde(rename = "route_short_name", default)]
    pub short_name: Option<String>,
    /// Full name of a route.
    #[serde(rename = "route_long_name", default)]
    pub long_name: Option<String>,
    /// Description of a route.
    #[serde(rename = "route_desc")]
    pub desc: Option<String>,
    /// Mode of transportation used on the route.
    pub route_type: RouteType,
    /// URL of a web page about the particular route.
    #[serde(rename = "route_url")]
    pub url: Option<String>,
    /// Agency for the specified route.
    pub agency_id: Option<AgencyId>,
    /// Order in which routes should be presented to customers.
    #[serde(rename = "route_sort_order")]
    pub order: Option<u32>,
    /// Route color matching public-facing material.
    #[serde(
        deserialize_with = "deserialize_optional_color",
        serialize_with = "serialize_optional_color",
        rename = "route_color",
        default
    )]
    pub color: Option<RGB8>,
    /// Legible color to use for text drawn against `route_color`.
    #[serde(
        deserialize_with = "deserialize_optional_color",
        serialize_with = "serialize_optional_color",
        rename = "route_text_color",
        default
    )]
    pub text_color: Option<RGB8>,
    /// Indicates whether a rider can board the transit vehicle anywhere along the travel path.
    #[serde(default)]
    pub continuous_pickup: ContinuousPickupDropOff,
    /// Indicates whether a rider can alight from the transit vehicle anywhere along the travel path.
    #[serde(default)]
    pub continuous_drop_off: ContinuousPickupDropOff,
}

/// Raw row from `trips.txt`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RawTrip {
    /// Unique identifier for the trip.
    #[serde(rename = "trip_id")]
    pub id: TripId,
    /// Service calendar identifier (`calendar.txt` / `calendar_dates.txt`).
    pub service_id: ServiceId,
    /// Route identifier (`routes.txt`).
    pub route_id: RouteId,
    /// Shape identifier (`shapes.txt`).
    pub shape_id: Option<ShapeId>,
    /// Text that appears on signage identifying the trip's destination to riders.
    pub trip_headsign: Option<String>,
    /// Public-facing text used to identify the trip to riders.
    pub trip_short_name: Option<String>,
    /// Indicates the direction of travel for a trip.
    pub direction_id: Option<DirectionType>,
    /// Identifies the block to which the trip belongs.
    pub block_id: Option<BlockId>,
    /// Indicates wheelchair accessibility for the trip.
    #[serde(default)]
    pub wheelchair_accessible: Availability,
    /// Indicates whether bikes are allowed on this trip.
    #[serde(default)]
    pub bikes_allowed: BikesAllowedType,
}

/// Raw row from `stop_times.txt` (including optional GTFS-Flex columns when `gtfs-flex` is enabled).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RawStopTime {
    /// Trip to which this stop time belongs.
    pub trip_id: TripId,
    /// Arrival time at the stop.
    #[serde(
        deserialize_with = "deserialize_optional_time",
        serialize_with = "serialize_optional_time",
        default
    )]
    pub arrival_time: Option<GtfsTime>,
    /// Departure time from the stop.
    #[serde(
        deserialize_with = "deserialize_optional_time",
        serialize_with = "serialize_optional_time",
        default
    )]
    pub departure_time: Option<GtfsTime>,
    /// Stop identifier (`stops.txt`). May be empty when GTFS-Flex `location_id` or `location_group_id` is used.
    #[serde(default)]
    pub stop_id: Option<StopId>,
    /// Order of stops for a particular trip.
    pub stop_sequence: u32,
    /// Text that appears on signage identifying the trip's destination at this stop.
    pub stop_headsign: Option<String>,
    /// Indicates pickup method.
    #[serde(default)]
    pub pickup_type: PickupDropOffType,
    /// Indicates drop-off method.
    #[serde(default)]
    pub drop_off_type: PickupDropOffType,
    /// Indicates continuous pickup behavior from this stop time onward.
    #[serde(default)]
    pub continuous_pickup: ContinuousPickupDropOff,
    /// Indicates continuous drop-off behavior from this stop time onward.
    #[serde(default)]
    pub continuous_drop_off: ContinuousPickupDropOff,
    /// Actual distance traveled along the associated shape from the first stop.
    #[serde(
        deserialize_with = "de_with_optional_float",
        serialize_with = "serialize_float_as_str",
        default
    )]
    pub shape_dist_traveled: Option<f64>,
    /// Indicates whether arrival and departure times are exact or approximate.
    #[serde(default)]
    pub timepoint: TimepointType,

    // --- GTFS-Flex extension columns ---
    /// Identifier referencing a GeoJSON location from `locations.geojson` (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    #[serde(default)]
    pub location_id: Option<LocationId>,
    /// Identifier referencing a location group from `location_groups.txt` (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    #[serde(default)]
    pub location_group_id: Option<LocationGroupId>,
    /// Time on-demand service becomes available (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    #[serde(
        deserialize_with = "deserialize_optional_time",
        serialize_with = "serialize_optional_time",
        default
    )]
    pub start_pickup_drop_off_window: Option<GtfsTime>,
    /// Time on-demand service ends (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    #[serde(
        deserialize_with = "deserialize_optional_time",
        serialize_with = "serialize_optional_time",
        default
    )]
    pub end_pickup_drop_off_window: Option<GtfsTime>,
    /// Booking rule identifier for pickup (`booking_rules.txt`, GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    #[serde(default)]
    pub pickup_booking_rule_id: Option<BookingRuleId>,
    /// Booking rule identifier for drop-off (`booking_rules.txt`, GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    #[serde(default)]
    pub drop_off_booking_rule_id: Option<BookingRuleId>,
    /// Factor for estimating mean travel duration (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    #[serde(
        deserialize_with = "de_with_optional_float",
        serialize_with = "serialize_float_as_str",
        default
    )]
    pub mean_duration_factor: Option<f64>,
    /// Offset in minutes for estimating mean travel duration (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    #[serde(
        deserialize_with = "de_with_optional_float",
        serialize_with = "serialize_float_as_str",
        default
    )]
    pub mean_duration_offset: Option<f64>,
    /// Factor for 95th percentile travel duration (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    #[serde(
        deserialize_with = "de_with_optional_float",
        serialize_with = "serialize_float_as_str",
        default
    )]
    pub safe_duration_factor: Option<f64>,
    /// Offset in minutes for 95th percentile travel duration (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    #[serde(
        deserialize_with = "de_with_optional_float",
        serialize_with = "serialize_float_as_str",
        default
    )]
    pub safe_duration_offset: Option<f64>,
}

/// Raw row from `calendar.txt`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RawCalendar {
    /// Service calendar identifier.
    #[serde(rename = "service_id")]
    pub id: ServiceId,
    /// Service operates on Mondays.
    #[serde(
        deserialize_with = "deserialize_bool",
        serialize_with = "serialize_bool"
    )]
    pub monday: bool,
    /// Service operates on Tuesdays.
    #[serde(
        deserialize_with = "deserialize_bool",
        serialize_with = "serialize_bool"
    )]
    pub tuesday: bool,
    /// Service operates on Wednesdays.
    #[serde(
        deserialize_with = "deserialize_bool",
        serialize_with = "serialize_bool"
    )]
    pub wednesday: bool,
    /// Service operates on Thursdays.
    #[serde(
        deserialize_with = "deserialize_bool",
        serialize_with = "serialize_bool"
    )]
    pub thursday: bool,
    /// Service operates on Fridays.
    #[serde(
        deserialize_with = "deserialize_bool",
        serialize_with = "serialize_bool"
    )]
    pub friday: bool,
    /// Service operates on Saturdays.
    #[serde(
        deserialize_with = "deserialize_bool",
        serialize_with = "serialize_bool"
    )]
    pub saturday: bool,
    /// Service operates on Sundays.
    #[serde(
        deserialize_with = "deserialize_bool",
        serialize_with = "serialize_bool"
    )]
    pub sunday: bool,
    /// Start service date (inclusive).
    #[serde(
        deserialize_with = "deserialize_date",
        serialize_with = "serialize_date"
    )]
    pub start_date: NaiveDate,
    /// End service date (inclusive).
    #[serde(
        deserialize_with = "deserialize_date",
        serialize_with = "serialize_date"
    )]
    pub end_date: NaiveDate,
}

/// Raw row from `calendar_dates.txt`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RawCalendarDate {
    /// Service identifier modified on this date.
    pub service_id: ServiceId,
    /// Date when service is added or removed.
    #[serde(
        deserialize_with = "deserialize_date",
        serialize_with = "serialize_date"
    )]
    pub date: NaiveDate,
    /// Whether service is added (`1`) or removed (`2`).
    pub exception_type: Exception,
}

/// Raw point row from `shapes.txt`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RawShapePoint {
    /// Shape identifier.
    #[serde(rename = "shape_id")]
    pub id: ShapeId,
    /// Latitude of the shape vertex in WGS84 decimal degrees.
    #[serde(rename = "shape_pt_lat", default)]
    pub latitude: f64,
    /// Longitude of the shape vertex in WGS84 decimal degrees.
    #[serde(rename = "shape_pt_lon", default)]
    pub longitude: f64,
    /// Sequence order of the point along the shape.
    #[serde(rename = "shape_pt_sequence")]
    pub sequence: u32,
    /// Distance traveled along the shape from the first shape point.
    #[serde(
        deserialize_with = "de_with_optional_float",
        serialize_with = "serialize_float_as_str",
        rename = "shape_dist_traveled",
        default
    )]
    pub dist_traveled: Option<f64>,
}

/// Raw row from `frequencies.txt`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawFrequency {
    /// Trip identifier.
    pub trip_id: TripId,
    /// Time at which the first vehicle departs from the first stop of the trip.
    pub start_time: GtfsTime,
    /// Time at which service changes to a different headway (or ceases) at the first stop.
    pub end_time: GtfsTime,
    /// Time in seconds between departures from the same stop (headway).
    pub headway_secs: u32,
    /// Indicates whether frequency-based trips are exactly scheduled or headway-based.
    #[serde(default)]
    pub exact_times: TimepointType,
}

/// Raw row from `transfers.txt`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawTransfer {
    /// Stop ID where a connection between routes begins.
    pub from_stop_id: StopId,
    /// Stop ID where a connection between routes ends.
    pub to_stop_id: StopId,
    /// Type of connection for the specified `(from_stop_id, to_stop_id)` pair.
    #[serde(default)]
    pub transfer_type: TransferType,
    /// Amount of time in seconds that must be available to permit a transfer.
    pub min_transfer_time: Option<u32>,
}

/// Raw row from `pathways.txt`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RawPathway {
    /// Unique pathway identifier.
    #[serde(rename = "pathway_id")]
    pub id: String,
    /// Origin stop/node ID.
    pub from_stop_id: StopId,
    /// Destination stop/node ID.
    pub to_stop_id: StopId,
    /// Pathway mode.
    #[serde(rename = "pathway_mode")]
    pub mode: PathwayMode,
    /// Directionality of the pathway.
    pub is_bidirectional: PathwayDirectionType,
    /// Horizontal length in meters.
    pub length: Option<f32>,
    /// Average walk traversal time in seconds.
    pub traversal_time: Option<u32>,
}

/// Raw row from `levels.txt`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RawLevel {
    /// Unique identifier of the level.
    #[serde(rename = "level_id")]
    pub id: LevelId,
    /// Numeric index of the level (`0` = ground level).
    pub level_index: f64,
    /// Optional elevation name.
    pub level_name: Option<String>,
}

/// Raw row from `feed_info.txt`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawFeedInfo {
    /// Full name of the organization that publishes the dataset.
    #[serde(rename = "feed_publisher_name")]
    pub name: String,
    /// URL of the feed publishing organization's website.
    #[serde(rename = "feed_publisher_url")]
    pub url: String,
    /// Default language used for the text in this dataset.
    #[serde(rename = "feed_lang")]
    pub lang: String,
    /// Start date for which the dataset provides reliable schedule information.
    #[serde(
        deserialize_with = "deserialize_option_date",
        serialize_with = "serialize_option_date",
        rename = "feed_start_date",
        default
    )]
    pub start_date: Option<NaiveDate>,
    /// End date for which the dataset provides reliable schedule information.
    #[serde(
        deserialize_with = "deserialize_option_date",
        serialize_with = "serialize_option_date",
        rename = "feed_end_date",
        default
    )]
    pub end_date: Option<NaiveDate>,
    /// String that indicates the current version of their GTFS dataset.
    #[serde(rename = "feed_version")]
    pub version: Option<String>,
}

/// Raw row from `stop_areas.txt`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawStopArea {
    /// Area identifier.
    pub area_id: AreaId,
    /// Stop or GeoJSON location identifier (`stops.txt` or `locations.geojson`).
    pub stop_id: String,
}

// --- Self-describing GtfsTable implementations for standard schedule files ---

crate::impl_gtfs_csv_table!(RawAgency, "agency.txt", mandatory = true);
crate::impl_gtfs_csv_table!(RawStop, "stops.txt", mandatory = true);
crate::impl_gtfs_csv_table!(RawRoute, "routes.txt", mandatory = true);
crate::impl_gtfs_csv_table!(RawTrip, "trips.txt", mandatory = true);
crate::impl_gtfs_csv_table!(RawStopTime, "stop_times.txt", mandatory = true);
crate::impl_gtfs_csv_table!(RawCalendar, "calendar.txt", mandatory = false);
crate::impl_gtfs_csv_table!(RawCalendarDate, "calendar_dates.txt", mandatory = false);
crate::impl_gtfs_csv_table!(RawShapePoint, "shapes.txt", mandatory = false);
crate::impl_gtfs_csv_table!(RawFrequency, "frequencies.txt", mandatory = false);
crate::impl_gtfs_csv_table!(RawTransfer, "transfers.txt", mandatory = false);
crate::impl_gtfs_csv_table!(RawPathway, "pathways.txt", mandatory = false);
crate::impl_gtfs_csv_table!(RawLevel, "levels.txt", mandatory = false);
crate::impl_gtfs_csv_table!(RawFeedInfo, "feed_info.txt", mandatory = false);
crate::impl_gtfs_csv_table!(RawStopArea, "stop_areas.txt", mandatory = false);

/// Composable [`TableBundle`](crate::raw::TableBundle) containing all standard GTFS Schedule tables.
#[derive(Clone, Debug, Default)]
pub struct ScheduleTables {
    /// Rows from `agency.txt` (mandatory).
    pub agencies: Vec<RawAgency>,
    /// Rows from `stops.txt` (mandatory).
    pub stops: Vec<RawStop>,
    /// Rows from `routes.txt` (mandatory).
    pub routes: Vec<RawRoute>,
    /// Rows from `trips.txt` (mandatory).
    pub trips: Vec<RawTrip>,
    /// Rows from `stop_times.txt` (mandatory).
    pub stop_times: Vec<RawStopTime>,
    /// Rows from `calendar.txt` (optional).
    pub calendar: Option<Vec<RawCalendar>>,
    /// Rows from `calendar_dates.txt` (optional).
    pub calendar_dates: Option<Vec<RawCalendarDate>>,
    /// Rows from `shapes.txt` (optional).
    pub shapes: Option<Vec<RawShapePoint>>,
    /// Rows from `frequencies.txt` (optional).
    pub frequencies: Option<Vec<RawFrequency>>,
    /// Rows from `transfers.txt` (optional).
    pub transfers: Option<Vec<RawTransfer>>,
    /// Rows from `pathways.txt` (optional).
    pub pathways: Option<Vec<RawPathway>>,
    /// Rows from `levels.txt` (optional).
    pub levels: Option<Vec<RawLevel>>,
    /// Rows from `feed_info.txt` (optional).
    pub feed_info: Option<Vec<RawFeedInfo>>,
    /// Rows from `stop_areas.txt` (optional).
    pub stop_areas: Option<Vec<RawStopArea>>,
}

impl crate::raw::TableBundle for ScheduleTables {
    fn from_archive<R: std::io::Read + std::io::Seek>(
        archive: &mut crate::raw::GtfsArchive<R>,
    ) -> Result<Self, crate::raw::RawReadError> {
        Ok(Self {
            agencies: archive.load()?,
            stops: archive.load()?,
            routes: archive.load()?,
            trips: archive.load()?,
            stop_times: archive.load()?,
            calendar: archive.load()?,
            calendar_dates: archive.load()?,
            shapes: archive.load()?,
            frequencies: archive.load()?,
            transfers: archive.load()?,
            pathways: archive.load()?,
            levels: archive.load()?,
            feed_info: archive.load()?,
            stop_areas: archive.load()?,
        })
    }
}
