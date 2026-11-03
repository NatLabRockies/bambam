//! Validated `Trip`, `StopTime`, `StopTimeTiming`, and `BoardingPolicy` domain types.

use crate::enums::{
    Availability, BikesAllowedType, ContinuousPickupDropOff, DirectionType, PickupDropOffType,
    TimepointType,
};
use crate::ids::{BlockId, RouteId, ServiceId, ShapeId, TripId};
use crate::model::place::StopPlace;
use crate::model::shape::ShapeLinearRef;
use crate::primitives::{GtfsTime, TimeWindow};
use crate::traits::{Identifiable, TemporalStopTime};

#[cfg(feature = "gtfs-flex")]
use crate::model::flex::{BookingRule, FlexDurationParams};
#[cfg(feature = "gtfs-flex")]
use std::sync::Arc;

/// Type-safe representation of *when* a vehicle visits a [`StopTime`].
///
/// Enforces the GTFS and GTFS-Flex invariant that fixed arrival/departure times
/// and flexible pickup/drop-off windows are mutually exclusive.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub enum StopTimeTiming {
    /// Both arrival and departure times are explicitly scheduled (`arrival <= departure`).
    Scheduled {
        /// Scheduled arrival time.
        arrival: GtfsTime,
        /// Scheduled departure time.
        departure: GtfsTime,
    },
    /// Intermediate stop where one or both times are omitted in raw GTFS and require interpolation.
    Uninterpolated {
        /// Optional arrival time.
        arrival: Option<GtfsTime>,
        /// Optional departure time.
        departure: Option<GtfsTime>,
    },
    /// Demand-responsive pickup/drop-off time window (`[start, end]`, GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    FlexWindow(TimeWindow),
}

/// Boarding or alighting policy at a [`StopTime`], combining standard `PickupDropOffType`,
/// `ContinuousPickupDropOff`, and optional GTFS-Flex `BookingRule` references.
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct BoardingPolicy {
    /// Standard pickup/drop-off method (`0` = regular, `1` = none, `2` = phone agency, `3` = coordinate with driver).
    pub method: PickupDropOffType,
    /// Continuous pickup/drop-off rule along the segment starting at this stop time.
    pub continuous: ContinuousPickupDropOff,
    /// Resolved booking rule from `booking_rules.txt` (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    pub booking_rule: Option<Arc<BookingRule>>,
}

impl BoardingPolicy {
    /// Returns `true` if riders are permitted to board/alight (either regularly or via coordination/booking).
    #[inline]
    pub fn is_allowed(&self) -> bool {
        !matches!(self.method, PickupDropOffType::NotAvailable)
    }
}

/// A relationally validated stop-time event along a [`Trip`].
#[derive(
    Clone, Debug, PartialEq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct StopTime {
    /// Monotonically increasing sequence number along the trip.
    pub stop_sequence: u32,
    /// Resolved target place (`Stop`, `FlexZone`, or `LocationGroup`).
    pub place: StopPlace,
    /// Validated timing (`Scheduled`, `Uninterpolated`, or `FlexWindow`).
    pub timing: StopTimeTiming,
    /// Pickup policy at this stop time.
    pub pickup: BoardingPolicy,
    /// Drop-off policy at this stop time.
    pub drop_off: BoardingPolicy,
    /// Optional signage headsign override from this stop onward.
    pub stop_headsign: Option<String>,
    /// Whether times are exact or approximate.
    pub timepoint: TimepointType,
    /// Resolved linear reference along the trip's [`ShapePolyline`](crate::model::ShapePolyline),
    /// enabling sub-segment linestring reconstruction between any pair of stops on the trip.
    pub shape_ref: Option<ShapeLinearRef>,
    /// Optional GTFS-Flex travel duration scaling and offset parameters.
    #[cfg(feature = "gtfs-flex")]
    pub flex_duration: FlexDurationParams,
}

impl StopTime {
    /// Returns `true` if this stop time uses a GTFS-Flex window, references a flexible zone/group,
    /// or has an associated booking rule.
    pub fn is_flexible(&self) -> bool {
        #[cfg(feature = "gtfs-flex")]
        {
            if matches!(self.timing, StopTimeTiming::FlexWindow(_))
                || matches!(
                    self.place,
                    StopPlace::FlexZone(_) | StopPlace::LocationGroup(_)
                )
                || self.pickup.booking_rule.is_some()
                || self.drop_off.booking_rule.is_some()
            {
                return true;
            }
        }
        false
    }
}

impl TemporalStopTime for StopTime {
    fn arrival_time(&self) -> Option<GtfsTime> {
        match self.timing {
            StopTimeTiming::Scheduled { arrival, .. } => Some(arrival),
            StopTimeTiming::Uninterpolated {
                arrival, departure, ..
            } => arrival.or(departure),
            #[cfg(feature = "gtfs-flex")]
            StopTimeTiming::FlexWindow(_) => None,
        }
    }

    fn departure_time(&self) -> Option<GtfsTime> {
        match self.timing {
            StopTimeTiming::Scheduled { departure, .. } => Some(departure),
            StopTimeTiming::Uninterpolated {
                arrival, departure, ..
            } => departure.or(arrival),
            #[cfg(feature = "gtfs-flex")]
            StopTimeTiming::FlexWindow(_) => None,
        }
    }

    fn flex_window(&self) -> Option<TimeWindow> {
        match self.timing {
            #[cfg(feature = "gtfs-flex")]
            StopTimeTiming::FlexWindow(w) => Some(w),
            _ => None,
        }
    }
}

/// Validated frequency headway window (`frequencies.txt`).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct FrequencyWindow {
    /// Time window during which vehicles depart at `headway_secs` intervals.
    pub window: TimeWindow,
    /// Headway in seconds between consecutive vehicle departures.
    pub headway_secs: u32,
    /// Whether departures are exact or headway-based.
    pub exact_times: TimepointType,
}

/// A validated transit trip (`trips.txt`) with ordered [`StopTime`]s and resolved shape linear references.
#[derive(
    Clone, Debug, PartialEq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct Trip {
    /// Unique trip identifier.
    pub id: TripId,
    /// Route served by this trip.
    pub route_id: RouteId,
    /// Service calendar schedule governing when this trip operates.
    pub service_id: ServiceId,
    /// Optional shape identifier (`shapes.txt`).
    pub shape_id: Option<ShapeId>,
    /// Optional trip headsign.
    pub trip_headsign: Option<String>,
    /// Optional trip short name.
    pub trip_short_name: Option<String>,
    /// Optional travel direction (`Outbound` or `Inbound`).
    pub direction_id: Option<DirectionType>,
    /// Optional block identifier.
    pub block_id: Option<BlockId>,
    /// Wheelchair accessibility.
    pub wheelchair_accessible: Availability,
    /// Bike policy.
    pub bikes_allowed: BikesAllowedType,
    /// Ordered sequence of stop times along the trip.
    pub stop_times: Vec<StopTime>,
    /// Optional headway frequency windows.
    pub frequencies: Vec<FrequencyWindow>,
}

impl Identifiable for Trip {
    type Id = TripId;

    #[inline]
    fn id(&self) -> &Self::Id {
        &self.id
    }
}
