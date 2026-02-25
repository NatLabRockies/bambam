use std::{fmt::Display, str::FromStr};

use serde::{Deserialize, Serialize};

/// model for OSM Highway keys. see <https://wiki.openstreetmap.org/wiki/Key:highway#Highway> for details.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Highway {
    // 7 main tags
    Motorway,
    Trunk,
    Primary,
    Secondary,
    Tertiary,
    Unclassified,
    Residential,

    // link roads
    MotorwayLink,
    TrunkLink,
    PrimaryLink,
    SecondaryLink,
    TertiaryLink,

    // special road types (elided except for those expected here: <https://github.nlr.gov/choehne/UrbanMEP/blob/UrbanMEP-SA/scripts/osm_utils.py>)
    LivingStreet,
    Service,
    Pedestrian,
    Track,
    BusGuideway,

    Road,
    Busway,
    Footway,
    Bridleway,
    Steps,
    Corridor,
    Path,
    Sidewalk,
    Cycleway,
    Elevator,
    Trailhead,

    Escape,
    Raceway,
    ViaFerrata,
    Proposed,
    Construction,
    BusStop,
    Crossing,
    CyclistWaitingAid,

    EmergencyBay,
    EmergencyAccessPoint,
    GiveWay,
    Ladder,
    Milestone,
    MiniRoundabout,
    MotorwayJunction,
    PassingPlace,
    Platform,
    RestArea,
    Services,
    SpeedCamera,
    SpeedDisplay,
    Stop,
    StreetLamp,
    TollGantry,
    TrafficMirror,
    TrafficSignals,

    TurningCircle,
    TurningLoop,
    Other(String),
}

impl Highway {
    /// the hierarchical representation of this highway.
    ///
    /// this is interpreted:
    ///   - the top 7 set via this link: <https://wiki.openstreetmap.org/wiki/Key:highway#Highway>
    ///   - their associated "*_link" types carry the same position
    ///   - anything else is an "8", as ranking them against each other is ambiguous, but they are
    ///     all clearly ranked as lower-priority from the top 7.
    pub fn hierarchy(&self) -> u64 {
        match self {
            Highway::Motorway => 1,
            Highway::Trunk => 2,
            Highway::Primary => 3,
            Highway::Secondary => 4,
            Highway::Tertiary => 5,
            Highway::Unclassified => 6,
            Highway::Residential => 7,
            Highway::MotorwayLink => 1,
            Highway::TrunkLink => 2,
            Highway::PrimaryLink => 3,
            Highway::SecondaryLink => 4,
            Highway::TertiaryLink => 5,
            Highway::LivingStreet => 8,
            Highway::Service => 8,
            Highway::Pedestrian => 8,
            Highway::Track => 8,
            Highway::BusGuideway => 8,
            Highway::Road => 8,
            Highway::Busway => 8,
            Highway::Footway => 8,
            Highway::Bridleway => 8,
            Highway::Steps => 8,
            Highway::Corridor => 8,
            Highway::Path => 8,
            Highway::Sidewalk => 8,
            Highway::Cycleway => 8,
            Highway::Elevator => 8,
            Highway::Trailhead => 8,
            Highway::Escape => 9,
            Highway::Raceway => 9,
            Highway::ViaFerrata => 9,
            Highway::Proposed => 9,
            Highway::Construction => 9,
            Highway::BusStop => 9,
            Highway::Crossing => 9,
            Highway::CyclistWaitingAid => 9,
            Highway::EmergencyBay => 9,
            Highway::EmergencyAccessPoint => 9,
            Highway::GiveWay => 9,
            Highway::Ladder => 9,
            Highway::Milestone => 9,
            Highway::MiniRoundabout => 9,
            Highway::MotorwayJunction => 9,
            Highway::PassingPlace => 9,
            Highway::Platform => 9,
            Highway::RestArea => 9,
            Highway::Services => 9,
            Highway::SpeedCamera => 9,
            Highway::SpeedDisplay => 9,
            Highway::Stop => 9,
            Highway::StreetLamp => 9,
            Highway::TollGantry => 9,
            Highway::TrafficMirror => 9,
            Highway::TrafficSignals => 9,
            Highway::TurningCircle => 9,
            Highway::TurningLoop => 9,
            Highway::Other(_) => 10,
        }
    }
}

impl Display for Highway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Highway::Motorway => write!(f, "motorway"),
            Highway::Trunk => write!(f, "trunk"),
            Highway::Primary => write!(f, "primary"),
            Highway::Secondary => write!(f, "secondary"),
            Highway::Tertiary => write!(f, "tertiary"),
            Highway::Unclassified => write!(f, "unclassified"),
            Highway::Residential => write!(f, "residential"),
            Highway::MotorwayLink => write!(f, "motorwaylink"),
            Highway::TrunkLink => write!(f, "trunklink"),
            Highway::PrimaryLink => write!(f, "primarylink"),
            Highway::SecondaryLink => write!(f, "secondarylink"),
            Highway::TertiaryLink => write!(f, "tertiarylink"),
            Highway::LivingStreet => write!(f, "livingstreet"),
            Highway::Service => write!(f, "service"),
            Highway::Pedestrian => write!(f, "pedestrian"),
            Highway::Track => write!(f, "track"),
            Highway::BusGuideway => write!(f, "busguideway"),
            Highway::Road => write!(f, "road"),
            Highway::Busway => write!(f, "busway"),
            Highway::Footway => write!(f, "footway"),
            Highway::Bridleway => write!(f, "bridleway"),
            Highway::Steps => write!(f, "steps"),
            Highway::Corridor => write!(f, "corridor"),
            Highway::Path => write!(f, "path"),
            Highway::Sidewalk => write!(f, "sidewalk"),
            Highway::Cycleway => write!(f, "cycleway"),
            Highway::Elevator => write!(f, "elevator"),
            Highway::Trailhead => write!(f, "trailhead"),
            Highway::Escape => write!(f, "escape"),
            Highway::Raceway => write!(f, "raceway"),
            Highway::ViaFerrata => write!(f, "via_ferrata"),
            Highway::Proposed => write!(f, "proposed"),
            Highway::Construction => write!(f, "construction"),
            Highway::BusStop => write!(f, "bus_stop"),
            Highway::Crossing => write!(f, "crossing"),
            Highway::CyclistWaitingAid => write!(f, "cyclist_waiting_aid"),
            Highway::EmergencyBay => write!(f, "emergency_bay"),
            Highway::EmergencyAccessPoint => write!(f, "emergency_access_point"),
            Highway::GiveWay => write!(f, "give_way"),
            Highway::Ladder => write!(f, "ladder"),
            Highway::Milestone => write!(f, "milestone"),
            Highway::MiniRoundabout => write!(f, "mini_roundabout"),
            Highway::MotorwayJunction => write!(f, "motorway_junction"),
            Highway::PassingPlace => write!(f, "passing_place"),
            Highway::Platform => write!(f, "platform"),
            Highway::RestArea => write!(f, "rest_area"),
            Highway::Services => write!(f, "services"),
            Highway::SpeedCamera => write!(f, "speed_camera"),
            Highway::SpeedDisplay => write!(f, "speed_display"),
            Highway::Stop => write!(f, "stop"),
            Highway::StreetLamp => write!(f, "street_lamp"),
            Highway::TollGantry => write!(f, "toll_gantry"),
            Highway::TrafficMirror => write!(f, "traffic_mirror"),
            Highway::TrafficSignals => write!(f, "traffic_signals"),
            Highway::TurningCircle => write!(f, "turning_circle"),
            Highway::TurningLoop => write!(f, "turning_loop"),
            Highway::Other(tag) => write!(f, "{tag}"),
        }
    }
}

impl FromStr for Highway {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // removed underscores to match against data errors where, in some
        // cases, users submitted entries without underscore or with spaces instead
        match s.trim().to_lowercase().replace("_", "").as_str() {
            "motorway" => Ok(Highway::Motorway),
            "trunk" => Ok(Highway::Trunk),
            "primary" => Ok(Highway::Primary),
            "secondary" => Ok(Highway::Secondary),
            "tertiary" => Ok(Highway::Tertiary),
            "unclassified" => Ok(Highway::Unclassified),
            "residential" => Ok(Highway::Residential),
            "motorwaylink" => Ok(Highway::MotorwayLink),
            "trunklink" => Ok(Highway::TrunkLink),
            "primarylink" => Ok(Highway::PrimaryLink),
            "secondarylink" => Ok(Highway::SecondaryLink),
            "tertiarylink" => Ok(Highway::TertiaryLink),
            "livingstreet" => Ok(Highway::LivingStreet),
            "service" => Ok(Highway::Service),
            "pedestrian" => Ok(Highway::Pedestrian),
            "track" => Ok(Highway::Track),
            "busguideway" => Ok(Highway::BusGuideway),
            "road" => Ok(Highway::Road),
            "busway" => Ok(Highway::Busway),
            "footway" => Ok(Highway::Footway),
            "bridleway" => Ok(Highway::Bridleway),
            "steps" => Ok(Highway::Steps),
            "corridor" => Ok(Highway::Corridor),
            "path" => Ok(Highway::Path),
            "sidewalk" => Ok(Highway::Sidewalk),
            "cycleway" => Ok(Highway::Cycleway),
            "elevator" => Ok(Highway::Elevator),
            "trailhead" => Ok(Highway::Trailhead),
            "escape" => Ok(Highway::Escape),
            "raceway" => Ok(Highway::Raceway),
            "viaferrata" => Ok(Highway::ViaFerrata),
            "proposed" => Ok(Highway::Proposed),
            "construction" => Ok(Highway::Construction),
            "busstop" => Ok(Highway::BusStop),
            "crossing" => Ok(Highway::Crossing),
            "cyclistwaitingaid" => Ok(Highway::CyclistWaitingAid),
            "emergencybay" => Ok(Highway::EmergencyBay),
            "emergencyaccesspoint" => Ok(Highway::EmergencyAccessPoint),
            "giveway" => Ok(Highway::GiveWay),
            "ladder" => Ok(Highway::Ladder),
            "milestone" => Ok(Highway::Milestone),
            "miniroundabout" => Ok(Highway::MiniRoundabout),
            "motorwayjunction" => Ok(Highway::MotorwayJunction),
            "passingplace" => Ok(Highway::PassingPlace),
            "platform" => Ok(Highway::Platform),
            "restarea" => Ok(Highway::RestArea),
            "services" => Ok(Highway::Services),
            "speedcamera" => Ok(Highway::SpeedCamera),
            "speeddisplay" => Ok(Highway::SpeedDisplay),
            "stop" => Ok(Highway::Stop),
            "streetlamp" => Ok(Highway::StreetLamp),
            "tollgantry" => Ok(Highway::TollGantry),
            "trafficmirror" => Ok(Highway::TrafficMirror),
            "trafficsignals" => Ok(Highway::TrafficSignals),
            "turningcircle" => Ok(Highway::TurningCircle),
            "turningloop" => Ok(Highway::TurningLoop),
            other => Ok(Highway::Other(other.to_string())),
        }
    }
}

impl Ord for Highway {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.hierarchy().cmp(&other.hierarchy())
    }
}
