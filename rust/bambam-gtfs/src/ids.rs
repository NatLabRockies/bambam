//! Strongly-typed identifier newtypes for GTFS entities and dense routing indices.

use std::borrow::Borrow;
use std::fmt;
use std::ops::Deref;

/// Macro that defines a strongly-typed string-backed GTFS identifier newtype
/// along with common conversions (`new`, `as_str`, `AsRef<str>`, `Deref`, `Borrow<str>`,
/// `From`, `Display`, and `Serde` transparent serialization).
#[macro_export]
macro_rules! define_gtfs_id {
    ($(#[$meta:meta])* $vis:vis struct $name:ident;) => {
        $(#[$meta])*
        #[derive(
            Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
            rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
        )]
        #[rkyv(derive(PartialEq, Eq, PartialOrd, Ord, Hash))]
        #[serde(transparent)]
        $vis struct $name(pub String);

        impl $name {
            /// Creates a new identifier from any string-like value.
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }

            /// Returns the underlying string slice.
            #[inline]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Returns true if the identifier string is empty.
            #[inline]
            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }
        }

        impl Deref for $name {
            type Target = str;

            #[inline]
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            #[inline]
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl Borrow<str> for $name {
            #[inline]
            fn borrow(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for $name {
            #[inline]
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            #[inline]
            fn from(s: &str) -> Self {
                Self(s.to_owned())
            }
        }

        impl From<$name> for String {
            #[inline]
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

/// Macro that defines a strongly-typed contiguous `u32` index for Phase 3 routing structures.
#[macro_export]
macro_rules! define_gtfs_index {
    ($(#[$meta:meta])* $vis:vis struct $name:ident;) => {
        $(#[$meta])*
        #[derive(
            Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize,
            Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
        )]
        #[rkyv(derive(PartialEq, Eq, PartialOrd, Ord, Hash))]
        #[serde(transparent)]
        $vis struct $name(pub u32);

        impl $name {
            /// Creates a new typed index from a `u32`.
            #[inline]
            pub const fn new(idx: u32) -> Self {
                Self(idx)
            }

            /// Creates a new typed index from a `usize`.
            #[inline]
            pub const fn from_usize(idx: usize) -> Self {
                Self(idx as u32)
            }

            /// Returns the index as a `usize` for slice indexing.
            #[inline]
            pub const fn index(self) -> usize {
                self.0 as usize
            }

            /// Returns the underlying `u32` value.
            #[inline]
            pub const fn as_u32(self) -> u32 {
                self.0
            }
        }

        impl From<u32> for $name {
            #[inline]
            fn from(idx: u32) -> Self {
                Self(idx)
            }
        }

        impl From<usize> for $name {
            #[inline]
            fn from(idx: usize) -> Self {
                Self(idx as u32)
            }
        }

        impl From<$name> for usize {
            #[inline]
            fn from(idx: $name) -> Self {
                idx.0 as usize
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

define_gtfs_id! {
    /// Unique identifier for a GTFS archive / feed in a multi-feed timetable (e.g. `"mdb-412"` or `"rtd-denver"`).
    pub struct FeedId;
}

define_gtfs_id! {
    /// Unique identifier for an transit agency (`agency.txt: agency_id`).
    pub struct AgencyId;
}

define_gtfs_id! {
    /// Unique identifier for a physical stop, station, or node (`stops.txt: stop_id`).
    pub struct StopId;
}

define_gtfs_id! {
    /// Unique identifier for a transit route (`routes.txt: route_id`).
    pub struct RouteId;
}

define_gtfs_id! {
    /// Unique identifier for a transit trip (`trips.txt: trip_id`).
    pub struct TripId;
}

define_gtfs_id! {
    /// Unique identifier for a service calendar schedule (`calendar.txt` / `calendar_dates.txt: service_id`).
    pub struct ServiceId;
}

define_gtfs_id! {
    /// Unique identifier for a vehicle trajectory shape (`shapes.txt: shape_id`).
    pub struct ShapeId;
}

define_gtfs_id! {
    /// Unique identifier for a station level (`levels.txt: level_id`).
    pub struct LevelId;
}

define_gtfs_id! {
    /// Unique identifier for a fare zone (`stops.txt` / `locations.geojson: zone_id`).
    pub struct ZoneId;
}

define_gtfs_id! {
    /// Unique identifier for a stop area (`areas.txt` / `stop_areas.txt: area_id`).
    pub struct AreaId;
}

define_gtfs_id! {
    /// Unique identifier for a vehicle block (`trips.txt: block_id`).
    pub struct BlockId;
}

#[cfg(feature = "gtfs-flex")]
define_gtfs_id! {
    /// Unique identifier for a GTFS-Flex GeoJSON zone (`locations.geojson: id`).
    pub struct LocationId;
}

#[cfg(feature = "gtfs-flex")]
define_gtfs_id! {
    /// Unique identifier for a GTFS-Flex location group (`location_groups.txt: location_group_id`).
    pub struct LocationGroupId;
}

#[cfg(feature = "gtfs-flex")]
define_gtfs_id! {
    /// Unique identifier for a GTFS-Flex booking rule (`booking_rules.txt: booking_rule_id`).
    pub struct BookingRuleId;
}

// --- Contiguous indices for Phase 3 (RAPTOR / Multimodal Graph Search) ---

define_gtfs_index! {
    /// Contiguous index for a GTFS feed/archive (`0..num_feeds`) in a multi-feed `RoutingTimetable`.
    pub struct FeedIdx;
}

define_gtfs_index! {
    /// Contiguous index for a routable place (`Stop`, `FlexZone`, or `LocationGroup`) in a `RoutingTimetable`.
    pub struct PlaceIdx;
}

define_gtfs_index! {
    /// Contiguous index for a `Route` in a `RoutingTimetable`.
    pub struct RouteIdx;
}

define_gtfs_index! {
    /// Contiguous index for a `RoutePattern` (unique stop/place sequence and pickup/drop-off profile).
    pub struct PatternIdx;
}

define_gtfs_index! {
    /// Contiguous index for a `Trip` in a `RoutingTimetable`.
    pub struct TripIdx;
}

define_gtfs_index! {
    /// Contiguous index for a `ServiceCalendar` in a `RoutingTimetable`.
    pub struct ServiceIdx;
}

define_gtfs_index! {
    /// Contiguous index for a `ShapePolyline` in a `RoutingTimetable`.
    pub struct ShapeIdx;
}

/// A GTFS identifier scoped to a specific archive ([`FeedId`]), preventing cross-feed ID collisions
/// when merging hundreds or thousands of GTFS archives into a single [`RoutingTimetable`](crate::routing::RoutingTimetable).
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ScopedId<Id> {
    /// Archive / feed identifier.
    pub feed_id: FeedId,
    /// Feed-local entity identifier (`StopId`, `RouteId`, `TripId`, etc.).
    pub id: Id,
}

impl<Id> ScopedId<Id> {
    /// Creates a new feed-scoped identifier.
    pub fn new(feed_id: impl Into<FeedId>, id: impl Into<Id>) -> Self {
        Self {
            feed_id: feed_id.into(),
            id: id.into(),
        }
    }
}

impl<Id: fmt::Display> fmt::Display for ScopedId<Id> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.feed_id, self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_newtypes_and_indices() {
        let stop_id = StopId::new("stop_1");
        assert_eq!(stop_id.as_str(), "stop_1");
        assert_eq!(&*stop_id, "stop_1");
        assert_eq!(stop_id.as_ref(), "stop_1");
        assert!(!stop_id.is_empty());
        assert!(StopId::default().is_empty());
        assert_eq!(format!("{stop_id}"), "stop_1");

        let from_str: StopId = "stop_2".into();
        let into_string: String = from_str.clone().into();
        assert_eq!(into_string, "stop_2");

        let place_idx = PlaceIdx::from_usize(42);
        assert_eq!(place_idx.index(), 42);
        assert_eq!(place_idx.as_u32(), 42);
        assert_eq!(usize::from(place_idx), 42);
        assert_eq!(PlaceIdx::from(42u32), place_idx);
        assert_eq!(format!("{place_idx}"), "42");

        let scoped: ScopedId<StopId> = ScopedId::new("feed_a", StopId::new("stop_1"));
        assert_eq!(scoped.feed_id.as_str(), "feed_a");
        assert_eq!(scoped.id.as_str(), "stop_1");
        assert_eq!(format!("{scoped}"), "feed_a:stop_1");
    }
}
