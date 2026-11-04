//! Route mode enumerations (`RouteType`, `ExtendedRouteType`).

use serde::de::{Deserialize, Deserializer};
use serde::ser::{Serialize, Serializer};

/// `rkyv::with` wrapper that serializes `ExtendedRouteType` as its `i16` discriminant.
pub struct RkyvExtendedRouteType;

impl rkyv::with::ArchiveWith<ExtendedRouteType> for RkyvExtendedRouteType {
    type Archived = rkyv::rend::i16_le;
    type Resolver = ();

    #[inline]
    fn resolve_with(
        field: &ExtendedRouteType,
        resolver: Self::Resolver,
        out: rkyv::Place<Self::Archived>,
    ) {
        let code = *field as i16;
        rkyv::Archive::resolve(&code, resolver, out);
    }
}

impl<S: rkyv::rancor::Fallible + ?Sized> rkyv::with::SerializeWith<ExtendedRouteType, S>
    for RkyvExtendedRouteType
{
    #[inline]
    fn serialize_with(
        field: &ExtendedRouteType,
        serializer: &mut S,
    ) -> Result<Self::Resolver, S::Error> {
        let code = *field as i16;
        rkyv::Serialize::serialize(&code, serializer)
    }
}

impl<D: rkyv::rancor::Fallible + ?Sized>
    rkyv::with::DeserializeWith<rkyv::rend::i16_le, ExtendedRouteType, D>
    for RkyvExtendedRouteType
{
    #[inline]
    fn deserialize_with(
        field: &rkyv::rend::i16_le,
        _deserializer: &mut D,
    ) -> Result<ExtendedRouteType, D::Error> {
        Ok(ExtendedRouteType::try_from(field.to_native()).unwrap_or(ExtendedRouteType::Bus))
    }
}

#[derive(Debug, Clone, derive_more::TryFrom, Copy, PartialEq, Eq, Hash)]
#[repr(i16)]
#[try_from(repr)]
/// Google Transit extended route types [https://developers.google.com/transit/gtfs/reference/extended-route-types](https://developers.google.com/transit/gtfs/reference/extended-route-types)
pub enum ExtendedRouteType {
    /// Railway Service.
    Railway = 100,
    /// High Speed Rail Service. Examples: TGV (FR), ICE (DE), Eurostar (GB).
    HighSpeedRail = 101,
    /// Long Distance Trains. Examples: InterCity/EuroCity.
    LongDistanceTrains = 102,
    /// Inter Regional Rail Service. Examples: InterRegio (DE), Cross County Rail (GB).
    InterRegionalRail = 103,
    /// Car Transport Rail Service.
    CarTransportRail = 104,
    /// Sleeper Rail Service. Example: GNER Sleeper (GB).
    SleeperRail = 105,
    /// Regional Rail Service. Examples: TER (FR), Regionalzug (DE).
    RegionalRail = 106,
    /// Tourist Railway Service. Example: Romney, Hythe & Dymchurch (GB).
    TouristRailway = 107,
    /// Rail Shuttle (Within Complex). Examples: Gatwick Shuttle (GB), Sky Line (DE).
    RailShuttleWithinComplex = 108,
    /// Suburban Railway. Examples: S-Bahn (DE), RER (FR), S-tog (Kopenhagen).
    SuburbanRailway = 109,
    /// Replacement Rail Service.
    ReplacementRail = 110,
    /// Special Rail Service.
    SpecialRail = 111,
    /// Lorry Transport Rail Service.
    LorryTransportRail = 112,
    /// All Rail Services.
    OtherRail = 113,
    /// Cross-Country Rail Service.
    CrossCountryRail = 114,
    /// Vehicle Transport Rail Service.
    VehicleTransportRail = 115,
    /// Rack and Pinion Railway. Examples: Rochers de Naye (CH), Dolderbahn (CH).
    RackAndPinionRailway = 116,
    /// Additional Rail Service.
    AdditionalRail = 117,

    /// Coach Service.
    Coach = 200,
    /// International Coach Service. Examples: EuroLine, Touring.
    InternationalCoach = 201,
    /// National Coach Service. Example: National Express (GB).
    NationalCoach = 202,
    /// Shuttle Coach Service. Examples: Roissy Bus (FR), Reading-Heathrow (GB).
    ShuttleCoach = 203,
    /// Regional Coach Service.
    RegionalCoach = 204,
    /// Special Coach Service.
    SpecialCoach = 205,
    /// Sightseeing Coach Service.
    SightseeingCoach = 206,
    /// Tourist Coach Service.
    TouristCoach = 207,
    /// Commuter Coach Service.
    CommuterCoach = 208,
    /// All Coach Services.
    OtherCoach = 209,

    /// Urban Railway Service.
    UrbanRailway = 400,
    /// Metro Service. Example: Métro de Paris.
    Metro = 401,
    /// Underground Service. Examples: London Underground, U-Bahn.
    Underground = 402,
    /// Urban Railway Service.
    UrbanRailwayServiceDetail = 403,
    /// All Urban Railway Services.
    OtherUrbanRailway = 404,
    /// Monorail.
    Monorail = 405,

    /// Bus Service.
    Bus = 700,
    /// Regional Bus Service. Example: Eastbourne-Maidstone (GB).
    RegionalBus = 701,
    /// Express Bus Service. Example: X19 Wokingham-Heathrow (GB).
    ExpressBus = 702,
    /// Stopping Bus Service. Example: 38 London: Clapton Pond-Victoria (GB).
    StoppingBus = 703,
    /// Local Bus Service.
    LocalBus = 704,
    /// Night Bus Service. Example: N prefixed buses in London (GB).
    NightBus = 705,
    /// Post Bus Service. Example: Maidstone P4 (GB).
    PostBus = 706,
    /// Special Needs Bus.
    SpecialNeedsBus = 707,
    /// Mobility Bus Service.
    MobilityBus = 708,
    /// Mobility Bus for Registered Disabled.
    MobilityBusForRegisteredDisabled = 709,
    /// Sightseeing Bus.
    SightseeingBus = 710,
    /// Shuttle Bus. Example: 747 Heathrow-Gatwick Airport Service (GB).
    ShuttleBus = 711,
    /// School Bus.
    SchoolBus = 712,
    /// School and Public Service Bus.
    SchoolAndPublicServiceBus = 713,
    /// Rail Replacement Bus Service.
    RailReplacementBus = 714,
    /// Demand and Response Bus Service.
    DemandAndResponseBus = 715,
    /// All Bus Services.
    OtherBus = 716,

    /// Trolleybus Service.
    Trolleybus = 800,

    /// Tram Service.
    Tram = 900,
    /// City Tram Service.
    CityTram = 901,
    /// Local Tram Service. Examples: Munich (DE), Brussels (BE), Croydon (GB).
    LocalTram = 902,
    /// Regional Tram Service.
    RegionalTram = 903,
    /// Sightseeing Tram Service. Example: Blackpool Seafront (GB).
    SightseeingTram = 904,
    /// Shuttle Tram Service.
    ShuttleTram = 905,
    /// All Tram Services.
    OtherTram = 906,

    /// Water Transport Service.
    WaterTransport = 1000,
    /// Air Service.
    Air = 1100,
    /// Ferry Service.
    Ferry = 1200,

    /// Aerial Lift Service. Examples: Telefèric de Montjuïc (ES), Saleve (CH), Roosevelt Island Tramway (US).
    AerialLift = 1300,
    /// Telecabin Service.
    Telecabin = 1301,
    /// Cable Car Service.
    CableCar = 1302,
    /// Elevator Service.
    Elevator = 1303,
    /// Chair Lift Service.
    ChairLift = 1304,
    /// Drag Lift Service.
    DragLift = 1305,
    /// Small Telecabin Service.
    SmallTelecabin = 1306,
    /// All Telecabin Services.
    OtherTelecabin = 1307,

    /// Funicular Service. Example: Rigiblick (Zürich, CH).
    Funicular = 1400,

    /// Taxi Service.
    Taxi = 1500,
    /// Communal Taxi Service. Examples: Marshrutka (RU), dolmuş (TR).
    CommunalTaxi = 1501,
    /// Water Taxi Service.
    WaterTaxi = 1502,
    /// Rail Taxi Service.
    RailTaxi = 1503,
    /// Bike Taxi Service.
    BikeTaxi = 1504,
    /// Licensed Taxi Service.
    LicensedTaxi = 1505,
    /// Private Hire Service Vehicle.
    PrivateHireServiceVehicle = 1506,
    /// All Taxi Services.
    OtherTaxi = 1507,

    /// Miscellaneous Service.
    MiscellaneousService = 1700,
    /// Horse-drawn Carriage.
    HorseDrawnCarriage = 1702,
}

/// Describes the kind of Route. See <https://gtfs.org/reference/static/#routestxt> `route_type`
///
/// Some route types are extended GTFS (<https://developers.google.com/transit/gtfs/reference/extended-route-types>)
#[derive(
    Debug,
    Default,
    Copy,
    Clone,
    PartialEq,
    Eq,
    Hash,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum RouteType {
    /// Tram, Streetcar, Light rail. Any light rail or street level system within a metropolitan area
    Tramway,
    /// Subway, Metro. Any underground rail system within a metropolitan area.
    Subway,
    /// Used for intercity or long-distance travel
    Rail,
    /// Used for short- and long-distance bus routes
    #[default]
    Bus,
    /// Used for short- and long-distance boat service
    Ferry,
    /// Used for street-level rail cars where the cable runs beneath the vehicle, e.g., cable car in San Francisco
    CableCar,
    /// Aerial lift, suspended cable car (e.g., gondola lift, aerial tramway). Cable transport where cabins, cars, gondolas or open chairs are suspended by means of one or more cables
    Gondola,
    /// Any rail system designed for steep inclines
    Funicular,
    /// Google Transit extended route types <https://developers.google.com/transit/gtfs/reference/extended-route-types>
    Extended(#[rkyv(with = RkyvExtendedRouteType)] ExtendedRouteType),
    /// Any other value
    Other(i16),
}

impl ExtendedRouteType {
    /// Convert extended route types to standard route types if possible. All route types that can not be represented in standard route types at all are kept as-is.
    /// This loses precision.
    pub fn to_standard_route_type(&self) -> RouteType {
        let int = *self as i16;
        let hundreds = int / 100;
        match hundreds {
            1 => RouteType::Rail,
            4 => RouteType::Subway,
            2 | 7 | 8 => RouteType::Bus,
            9 => RouteType::Tramway,
            10 | 12 => RouteType::Ferry,
            13 => RouteType::Gondola,
            14 => RouteType::Funicular,
            _ => RouteType::Extended(*self),
        }
    }
}

impl<'de> Deserialize<'de> for RouteType {
    fn deserialize<D>(deserializer: D) -> Result<RouteType, D::Error>
    where
        D: Deserializer<'de>,
    {
        let i = i16::deserialize(deserializer)?;
        if let Ok(ert) = ExtendedRouteType::try_from(i) {
            return Ok(RouteType::Extended(ert));
        }

        Ok(match i {
            0 => RouteType::Tramway,
            1 => RouteType::Subway,
            2 => RouteType::Rail,
            3 => RouteType::Bus,
            4 => RouteType::Ferry,
            5 => RouteType::CableCar,
            6 => RouteType::Gondola,
            7 => RouteType::Funicular,
            _ => RouteType::Other(i),
        })
    }
}

impl Serialize for RouteType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i16(match self {
            RouteType::Tramway => 0,
            RouteType::Subway => 1,
            RouteType::Rail => 2,
            RouteType::Bus => 3,
            RouteType::Ferry => 4,
            RouteType::CableCar => 5,
            RouteType::Gondola => 6,
            RouteType::Funicular => 7,
            RouteType::Extended(ert) => *ert as i16,
            RouteType::Other(i) => *i,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct RouteRow {
        route_type: RouteType,
    }

    #[test]
    fn test_route_type_serde_and_standard_mapping() {
        let cases = [
            (0, RouteType::Tramway),
            (1, RouteType::Subway),
            (2, RouteType::Rail),
            (3, RouteType::Bus),
            (4, RouteType::Ferry),
            (5, RouteType::CableCar),
            (6, RouteType::Gondola),
            (7, RouteType::Funicular),
            (100, RouteType::Extended(ExtendedRouteType::Railway)),
            (401, RouteType::Extended(ExtendedRouteType::Metro)),
            (700, RouteType::Extended(ExtendedRouteType::Bus)),
            (
                715,
                RouteType::Extended(ExtendedRouteType::DemandAndResponseBus),
            ),
            (999, RouteType::Other(999)),
        ];

        for (code, expected) in cases {
            let csv_str = format!("route_type\n{code}\n");
            let row: RouteRow = csv::Reader::from_reader(csv_str.as_bytes())
                .deserialize()
                .next()
                .unwrap()
                .unwrap();
            assert_eq!(row.route_type, expected);
        }

        // ExtendedRouteType::to_standard_route_type mapping
        assert_eq!(
            ExtendedRouteType::HighSpeedRail.to_standard_route_type(),
            RouteType::Rail
        );
        assert_eq!(
            ExtendedRouteType::NationalCoach.to_standard_route_type(),
            RouteType::Bus
        );
        assert_eq!(
            ExtendedRouteType::Metro.to_standard_route_type(),
            RouteType::Subway
        );
        assert_eq!(
            ExtendedRouteType::DemandAndResponseBus.to_standard_route_type(),
            RouteType::Bus
        );
        assert_eq!(
            ExtendedRouteType::Trolleybus.to_standard_route_type(),
            RouteType::Bus
        );
        assert_eq!(
            ExtendedRouteType::CityTram.to_standard_route_type(),
            RouteType::Tramway
        );
        assert_eq!(
            ExtendedRouteType::WaterTransport.to_standard_route_type(),
            RouteType::Ferry
        );
        assert_eq!(
            ExtendedRouteType::Ferry.to_standard_route_type(),
            RouteType::Ferry
        );
        assert_eq!(
            ExtendedRouteType::Telecabin.to_standard_route_type(),
            RouteType::Gondola
        );
        assert_eq!(
            ExtendedRouteType::Funicular.to_standard_route_type(),
            RouteType::Funicular
        );
        assert_eq!(
            ExtendedRouteType::Air.to_standard_route_type(),
            RouteType::Extended(ExtendedRouteType::Air)
        );
        assert_eq!(
            ExtendedRouteType::Taxi.to_standard_route_type(),
            RouteType::Extended(ExtendedRouteType::Taxi)
        );
    }

    #[test]
    fn test_rkyv_extended_route_type_roundtrip() {
        let variants = [
            RouteType::Bus,
            RouteType::Extended(ExtendedRouteType::HighSpeedRail),
            RouteType::Extended(ExtendedRouteType::DemandAndResponseBus),
            RouteType::Extended(ExtendedRouteType::HorseDrawnCarriage),
            RouteType::Other(1999),
        ];
        for v in variants {
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&v).unwrap();
            let restored: RouteType =
                rkyv::from_bytes::<RouteType, rkyv::rancor::Error>(&bytes).unwrap();
            assert_eq!(restored, v);
        }
    }
}
