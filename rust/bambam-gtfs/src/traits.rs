//! Core traits for `bambam-gtfs` records and spatial/temporal entities.

use crate::primitives::{BoundingBox, Coord, GtfsTime, TimeWindow};
use std::hash::Hash;
use std::sync::Arc;

/// Trait implemented by GTFS records that have a strongly-typed unique identifier.
pub trait Identifiable {
    /// The strongly-typed identifier newtype (e.g. `StopId`, `RouteId`, `TripId`).
    type Id: Eq + Hash + Clone;

    /// Returns a reference to this entity's identifier.
    fn id(&self) -> &Self::Id;
}

impl<T: Identifiable> Identifiable for Arc<T> {
    type Id = T::Id;

    #[inline]
    fn id(&self) -> &Self::Id {
        self.as_ref().id()
    }
}

/// Trait implemented by spatial GTFS entities (`Stop`, `FlexLocation`, `LocationGroup`, `StopPlace`, `ShapePolyline`).
///
/// Allows route planners and multimodal A* search to query representative coordinates and
/// bounding boxes uniformly across fixed stops and flexible service zones.
pub trait Spatial {
    /// Returns a representative coordinate (exact stop coordinate or polygon/group centroid), if available.
    fn representative_coord(&self) -> Option<Coord>;

    /// Returns the axis-aligned geographic bounding box covering this entity, if available.
    fn bounding_box(&self) -> Option<BoundingBox>;
}

impl<T: Spatial> Spatial for Arc<T> {
    #[inline]
    fn representative_coord(&self) -> Option<Coord> {
        self.as_ref().representative_coord()
    }

    #[inline]
    fn bounding_box(&self) -> Option<BoundingBox> {
        self.as_ref().bounding_box()
    }
}

/// Trait providing unified temporal access to a stop-time event.
pub trait TemporalStopTime {
    /// Returns the scheduled arrival time at the stop, if fixed or interpolated.
    fn arrival_time(&self) -> Option<GtfsTime>;

    /// Returns the scheduled departure time from the stop, if fixed or interpolated.
    fn departure_time(&self) -> Option<GtfsTime>;

    /// Returns the flexible pickup/drop-off window (`[start, end]`), if this is a demand-responsive stop time.
    fn flex_window(&self) -> Option<TimeWindow>;

    /// Returns the earliest time a rider could board or alight at this stop time.
    fn earliest_time(&self) -> Option<GtfsTime> {
        self.arrival_time()
            .or_else(|| self.departure_time())
            .or_else(|| self.flex_window().map(|w| w.start))
    }

    /// Returns the latest time a rider could board or alight at this stop time.
    fn latest_time(&self) -> Option<GtfsTime> {
        self.departure_time()
            .or_else(|| self.arrival_time())
            .or_else(|| self.flex_window().map(|w| w.end))
    }
}
