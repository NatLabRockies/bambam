//! Phase 3: Route-planning timetables (`RaptorTimetable` and `EdgeTimetable`) and spatial/shape traversal structures.

macro_rules! impl_rkyv_snapshot {
    ($ty:ident, $archived:ident) => {
        impl $ty {
            /// Serializes this timetable into a compact `rkyv` byte vector.
            pub fn to_rkyv_bytes(&self) -> Result<Vec<u8>, $crate::routing::RoutingError> {
                let aligned = rkyv::to_bytes::<rkyv::rancor::Error>(self)
                    .map_err(|e| $crate::routing::RoutingError::Snapshot(e.to_string()))?;
                Ok(aligned.into_vec())
            }

            /// Validates and returns a zero-copy archived reference directly from a
            /// 16-byte-aligned `rkyv` byte slice without allocating or copying the vectors.
            pub fn access_rkyv_bytes(
                bytes: &[u8],
            ) -> Result<&$archived, $crate::routing::RoutingError> {
                rkyv::access::<$archived, rkyv::rancor::Error>(bytes)
                    .map_err(|e| $crate::routing::RoutingError::Snapshot(e.to_string()))
            }

            /// Deserializes a timetable from an `rkyv` byte slice (reading directly when `bytes`
            /// is already 16-byte aligned, or copying into an aligned buffer otherwise).
            pub fn from_rkyv_bytes(bytes: &[u8]) -> Result<Self, $crate::routing::RoutingError> {
                if (bytes.as_ptr() as usize) % 16 == 0 {
                    rkyv::from_bytes::<Self, rkyv::rancor::Error>(bytes)
                        .map_err(|e| $crate::routing::RoutingError::Snapshot(e.to_string()))
                } else {
                    let mut aligned = rkyv::util::AlignedVec::<16>::with_capacity(bytes.len());
                    aligned.extend_from_slice(bytes);
                    rkyv::from_bytes::<Self, rkyv::rancor::Error>(&aligned)
                        .map_err(|e| $crate::routing::RoutingError::Snapshot(e.to_string()))
                }
            }

            /// Writes this timetable as a binary `rkyv` snapshot to `writer`.
            pub fn write_rkyv<W: std::io::Write>(
                &self,
                mut writer: W,
            ) -> Result<(), $crate::routing::RoutingError> {
                let aligned = rkyv::to_bytes::<rkyv::rancor::Error>(self)
                    .map_err(|e| $crate::routing::RoutingError::Snapshot(e.to_string()))?;
                writer
                    .write_all(&aligned)
                    .map_err(|e| $crate::routing::RoutingError::Snapshot(e.to_string()))
            }

            /// Reads a timetable from a binary `rkyv` snapshot `reader`.
            pub fn read_rkyv<R: std::io::Read>(
                mut reader: R,
            ) -> Result<Self, $crate::routing::RoutingError> {
                let mut bytes = Vec::new();
                reader
                    .read_to_end(&mut bytes)
                    .map_err(|e| $crate::routing::RoutingError::Snapshot(e.to_string()))?;
                Self::from_rkyv_bytes(&bytes)
            }

            /// Saves this timetable to a file on disk in `rkyv` format.
            pub fn save_rkyv(
                &self,
                path: impl AsRef<std::path::Path>,
            ) -> Result<(), $crate::routing::RoutingError> {
                let file = std::fs::File::create(path)
                    .map_err(|e| $crate::routing::RoutingError::Snapshot(e.to_string()))?;
                let writer = std::io::BufWriter::new(file);
                self.write_rkyv(writer)
            }

            /// Loads a timetable from an `rkyv` snapshot file on disk.
            pub fn load_rkyv(
                path: impl AsRef<std::path::Path>,
            ) -> Result<Self, $crate::routing::RoutingError> {
                let file = std::fs::File::open(path)
                    .map_err(|e| $crate::routing::RoutingError::Snapshot(e.to_string()))?;
                let reader = std::io::BufReader::new(file);
                Self::read_rkyv(reader)
            }
        }
    };
}

mod builder;
mod compact_stop_time;
mod departure;
mod edge_timetable;
mod error;
mod feed_metadata;
mod lineage;
mod pattern;
mod routable_trip;
mod spatial_index;
mod timetable;

pub use builder::RaptorTimetableBuilder;
pub use compact_stop_time::{CompactStopTime, RoutableStopTiming};
pub use departure::{ActiveServiceWindow, BoardableDeparture};
pub use edge_timetable::{
    ArchivedEdgeTimetable, EdgeDeparture, EdgeRouteSchedule, EdgeTimetable, EdgeTimetableBuilder,
};
pub use error::RoutingError;
pub use feed_metadata::FeedMetadata;
pub use lineage::{
    FullyQualifiedPlaceId, FullyQualifiedRouteId, FullyQualifiedTripId, PlaceIdKind,
};
pub use pattern::{IndexedTransfer, PatternStopProfile, PlacePatternOccurrence, RoutePattern};
pub use routable_trip::RoutableTrip;
pub use spatial_index::{PlaceSpatialEntry, RkyvSpatialIndex};
pub use timetable::{ArchivedRaptorTimetable, RaptorTimetable};
