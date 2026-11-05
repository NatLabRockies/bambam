//! Edge-oriented Compressed Sparse Row (CSR) timetable (`EdgeTimetable`, `EdgeRouteSchedule`, `EdgeDeparture`)
//! for time-dependent edge-based multimodal graph search (`routee-compass`).
//!
//! ## Data Layout
//! Unlike [`crate::routing::RaptorTimetable`] (which groups stop times by full [`crate::routing::RoutePattern`]
//! sequences), [`EdgeTimetable`] indexes schedules directly by graph `edge_id` (`0..num_edges`) using
//! three flat contiguous arrays:
//! 1. `edge_offsets: Vec<u32>` (`edge_id -> route_schedules[start..end]`),
//! 2. `route_schedules: Vec<EdgeRouteSchedule>` (`(route_label, departures[start..end])`), and
//! 3. `departures: Vec<EdgeDeparture>` (sorted ascending by `(src_departure_time, dst_arrival_time)`).
//!
//! ## Finding the Next Departure on an Edge
//! When the router evaluates a transit `edge_id` at `current_datetime` with the rider's current
//! `route_id` state ([`EdgeTimetable::get_next_departure`]):
//! 1. **$O(1)$ CSR Slice Lookup:** `edge_offsets[edge_id]..edge_offsets[edge_id + 1]` yields the slice of
//!    [`EdgeRouteSchedule`]s operating along that edge.
//! 2. **$O(\log D)$ Binary Search + Early Exit:** For each [`EdgeRouteSchedule`], `partition_point`
//!    binary-searches its contiguous [`EdgeDeparture`] slice for the first departure with
//!    `src_departure_time >= current_datetime`. It then scans forward to find the departure with the
//!    earliest `dst_arrival_time`, terminating as soon as `dep.src_departure_time >= best.dst_arrival_time`
//!    (since non-negative travel times guarantee no later departure can arrive earlier).
//! 3. **Unboarded vs. Boarded State Handling:**
//!    - **Unboarded or Transferring (`current_route_id != Some(schedule.route_label)`):** The rider is
//!      entering from the road network (or switching routes) and must walk from the graph's source vertex
//!      to the physical GTFS stop (`boarding_delay = dep.src_walk_distance / walk_speed`). A candidate
//!      departure is only boardable if `dep.src_departure_time >= current_datetime + boarding_delay`.
//!      The resulting time between `current_datetime` and `dep.src_departure_time` (walking to the stop
//!      plus waiting at the stop platform) is recorded as **transit boarding time**.
//!    - **Already Boarded (`current_route_id == Some(schedule.route_label)`):** The rider is already
//!      on board the vehicle from the preceding transit edge, so `boarding_delay` is zero and any
//!      departure with `dep.src_departure_time >= current_datetime` is eligible. Any elapsed time between
//!      `current_datetime` (the vehicle's arrival at this stop) and `dep.src_departure_time` represents
//!      in-vehicle **scheduled dwell time** at the stop rather than boarding wait time.

use crate::primitives::{RkyvLengthMeters, RkyvNaiveDateTime};
use crate::routing::RoutingError;
use chrono::{Duration, NaiveDateTime};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::ops::Range;
use std::path::Path;
use uom::si::f64::{Length, Time, Velocity};
use uom::si::length::meter;
use uom::si::time::second;
use uom::si::velocity::meter_per_second;

/// A single scheduled transit departure along a directed graph edge, including
/// the walking distances between the road network vertices and the physical GTFS stops.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct EdgeDeparture {
    /// Scheduled or interpolated departure timestamp from the source transit stop.
    #[rkyv(with = RkyvNaiveDateTime)]
    pub src_departure_time: NaiveDateTime,
    /// Scheduled or interpolated arrival timestamp at the destination transit stop.
    #[rkyv(with = RkyvNaiveDateTime)]
    pub dst_arrival_time: NaiveDateTime,
    /// Walking distance from the source road graph vertex to the boarding transit stop.
    #[rkyv(with = RkyvLengthMeters)]
    pub src_walk_distance: Length,
    /// Walking distance from the alighting transit stop to the destination road graph vertex.
    #[rkyv(with = RkyvLengthMeters)]
    pub dst_walk_distance: Length,
}

impl EdgeDeparture {
    /// Creates a new [`EdgeDeparture`] with explicit `src_walk_distance` and `dst_walk_distance`.
    pub fn new(
        src_departure_time: NaiveDateTime,
        dst_arrival_time: NaiveDateTime,
        src_walk_distance: Length,
        dst_walk_distance: Length,
    ) -> Self {
        Self {
            src_departure_time,
            dst_arrival_time,
            src_walk_distance,
            dst_walk_distance,
        }
    }

    /// Creates an [`EdgeDeparture`] with zero vertex-to-stop walking distance.
    pub fn without_walk_distance(
        src_departure_time: NaiveDateTime,
        dst_arrival_time: NaiveDateTime,
    ) -> Self {
        Self {
            src_departure_time,
            dst_arrival_time,
            src_walk_distance: Length::new::<meter>(0.0),
            dst_walk_distance: Length::new::<meter>(0.0),
        }
    }

    /// Returns the in-vehicle travel time (`dst_arrival_time - src_departure_time`) as [`uom::si::f64::Time`].
    pub fn travel_time(&self) -> Result<Time, RoutingError> {
        if self.dst_arrival_time < self.src_departure_time {
            return Err(RoutingError::NegativeEdgeTravelTime {
                src_departure_time: self
                    .src_departure_time
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string(),
                dst_arrival_time: self
                    .dst_arrival_time
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string(),
            });
        }
        let delta = self
            .dst_arrival_time
            .signed_duration_since(self.src_departure_time);
        let secs = delta.num_milliseconds() as f64 / 1000.0;
        Ok(Time::new::<second>(secs))
    }

    /// Computes the boarding walking delay (`src_walk_distance / walk_speed`) required to walk
    /// from the source road vertex to the boarding transit stop.
    ///
    /// Returns zero if `walk_speed <= 0.0` or `src_walk_distance <= 0.0`.
    pub fn boarding_walk_delay(&self, walk_speed: Velocity) -> Time {
        let speed_mps = walk_speed.get::<meter_per_second>();
        let dist_m = self.src_walk_distance.get::<meter>();
        if speed_mps > 0.0 && dist_m > 0.0 {
            self.src_walk_distance / walk_speed
        } else {
            Time::new::<second>(0.0)
        }
    }

    /// Computes the alighting walking delay (`dst_walk_distance / walk_speed`) required to walk
    /// from the alighting transit stop to the destination road vertex.
    ///
    /// Returns zero if `walk_speed <= 0.0` or `dst_walk_distance <= 0.0`.
    pub fn alighting_walk_delay(&self, walk_speed: Velocity) -> Time {
        let speed_mps = walk_speed.get::<meter_per_second>();
        let dist_m = self.dst_walk_distance.get::<meter>();
        if speed_mps > 0.0 && dist_m > 0.0 {
            self.dst_walk_distance / walk_speed
        } else {
            Time::new::<second>(0.0)
        }
    }
}

/// Contiguous CSR schedule slice for a single `route_label` on a specific transit `edge_id`.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct EdgeRouteSchedule {
    /// Categorical route label (`i64`) corresponding to the route's fully-qualified ID.
    pub route_label: i64,
    /// Start index (inclusive) into [`EdgeTimetable::departures`].
    pub departures_start: u32,
    /// End index (exclusive) into [`EdgeTimetable::departures`].
    pub departures_end: u32,
}

impl EdgeRouteSchedule {
    /// Returns the index range `departures_start..departures_end` into [`EdgeTimetable::departures`].
    pub const fn departures_range(&self) -> Range<usize> {
        (self.departures_start as usize)..(self.departures_end as usize)
    }
}

/// Flat 3-array CSR timetable indexed by graph `edge_id` (`0..num_edges`) for $O(1)$ edge lookup
/// and $O(\log D)$ binary search over sorted departures.
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct EdgeTimetable {
    /// CSR offset table indexed by `edge_id` (`0..num_edges`), with length `num_edges + 1`
    /// (or empty when `num_edges == 0`).
    ///
    /// Edge `e` owns `route_schedules[edge_offsets[e] as usize .. edge_offsets[e + 1] as usize]`.
    pub edge_offsets: Vec<u32>,
    /// Flat CSR arena of [`EdgeRouteSchedule`]s across all edges.
    pub route_schedules: Vec<EdgeRouteSchedule>,
    /// Flat CSR arena of [`EdgeDeparture`]s across all `(edge_id, route_label)` pairs, sorted
    /// ascending by `(src_departure_time, dst_arrival_time)` within each [`EdgeRouteSchedule`].
    pub departures: Vec<EdgeDeparture>,
    /// Optional catalog of unique fully-qualified route IDs (`fq_route_ids`).
    pub fq_route_ids: Vec<String>,
}

impl EdgeTimetable {
    /// Creates an [`EdgeTimetableBuilder`] for assembling an [`EdgeTimetable`].
    pub fn builder() -> EdgeTimetableBuilder {
        EdgeTimetableBuilder::default()
    }

    /// Number of dense graph edges (`0..num_edges`) indexed by this timetable.
    pub fn num_edges(&self) -> usize {
        self.edge_offsets.len().saturating_sub(1)
    }

    /// Returns the slice of [`EdgeRouteSchedule`]s serving `edge_id` in $O(1)$ time,
    /// or `None` if `edge_id >= self.num_edges()`.
    pub fn edge_route_schedules(&self, edge_id: usize) -> Option<&[EdgeRouteSchedule]> {
        if edge_id >= self.num_edges() {
            return None;
        }
        let start = self.edge_offsets[edge_id] as usize;
        let end = self.edge_offsets[edge_id + 1] as usize;
        self.route_schedules.get(start..end)
    }

    /// Returns the slice of sorted [`EdgeDeparture`]s for `schedule` in $O(1)$ time.
    pub fn schedule_departures(&self, schedule: &EdgeRouteSchedule) -> &[EdgeDeparture] {
        self.departures
            .get(schedule.departures_range())
            .unwrap_or(&[])
    }

    /// Searches all route schedules on `edge_id` to find the departure that arrives soonest
    /// at the destination stop when starting from the source vertex at `current_datetime`.
    ///
    /// - When `current_route_id == Some(schedule.route_label)`, the rider is already aboard
    ///   that route, so no boarding walk delay is required (`boarding_delay == 0`).
    /// - When boarding a new route (`current_route_id != Some(schedule.route_label)`), if
    ///   `walk_speed` is provided, the rider must walk `dep.src_walk_distance` at `walk_speed`
    ///   before they can board (`src_departure_time >= current_datetime + boarding_delay`).
    ///
    /// Returns `Ok(Some((route_label, best_departure, boarding_delay)))` or `Ok(None)` if no
    /// future departure exists on `edge_id`.
    pub fn get_next_departure(
        &self,
        edge_id: usize,
        current_datetime: &NaiveDateTime,
        current_route_id: Option<i64>,
        walk_speed: Option<Velocity>,
    ) -> Result<Option<(i64, EdgeDeparture, Time)>, RoutingError> {
        let schedules =
            self.edge_route_schedules(edge_id)
                .ok_or(RoutingError::InvalidEdgeId {
                    edge_id,
                    num_edges: self.num_edges(),
                })?;

        let mut best: Option<(i64, EdgeDeparture, Time)> = None;

        for schedule in schedules {
            let deps = self.schedule_departures(schedule);
            if deps.is_empty() {
                continue;
            }
            let is_same_route = current_route_id == Some(schedule.route_label);
            let effective_walk_speed = if is_same_route { None } else { walk_speed };

            if let Some((dep, boarding_delay)) =
                find_best_route_departure(*current_datetime, deps, effective_walk_speed)?
            {
                match &best {
                    None => {
                        best = Some((schedule.route_label, dep, boarding_delay));
                    }
                    Some((_, prev_dep, _)) => {
                        if dep.dst_arrival_time < prev_dep.dst_arrival_time {
                            best = Some((schedule.route_label, dep, boarding_delay));
                        }
                    }
                }
            }
        }

        Ok(best)
    }

    // --- Binary Snapshot Serialization (`rkyv`) ---

    /// Serializes this [`EdgeTimetable`] into a compact `rkyv` byte vector.
    pub fn to_rkyv_bytes(&self) -> Result<Vec<u8>, RoutingError> {
        let aligned = rkyv::to_bytes::<rkyv::rancor::Error>(self)
            .map_err(|e| RoutingError::Snapshot(e.to_string()))?;
        Ok(aligned.into_vec())
    }

    /// Validates and returns a zero-copy [`ArchivedEdgeTimetable`] reference directly from a
    /// 16-byte-aligned `rkyv` byte slice without allocating or copying the CSR vectors.
    pub fn access_rkyv_bytes(bytes: &[u8]) -> Result<&ArchivedEdgeTimetable, RoutingError> {
        rkyv::access::<ArchivedEdgeTimetable, rkyv::rancor::Error>(bytes)
            .map_err(|e| RoutingError::Snapshot(e.to_string()))
    }

    /// Deserializes an [`EdgeTimetable`] from an `rkyv` byte slice.
    pub fn from_rkyv_bytes(bytes: &[u8]) -> Result<Self, RoutingError> {
        let mut aligned = rkyv::util::AlignedVec::<16>::with_capacity(bytes.len());
        aligned.extend_from_slice(bytes);
        rkyv::from_bytes::<Self, rkyv::rancor::Error>(&aligned)
            .map_err(|e| RoutingError::Snapshot(e.to_string()))
    }

    /// Writes this [`EdgeTimetable`] as a binary `rkyv` snapshot to `writer`.
    pub fn write_rkyv<W: Write>(&self, mut writer: W) -> Result<(), RoutingError> {
        let aligned = rkyv::to_bytes::<rkyv::rancor::Error>(self)
            .map_err(|e| RoutingError::Snapshot(e.to_string()))?;
        writer
            .write_all(&aligned)
            .map_err(|e| RoutingError::Snapshot(e.to_string()))
    }

    /// Reads an [`EdgeTimetable`] from a binary `rkyv` snapshot `reader`.
    pub fn read_rkyv<R: Read>(mut reader: R) -> Result<Self, RoutingError> {
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(|e| RoutingError::Snapshot(e.to_string()))?;
        Self::from_rkyv_bytes(&bytes)
    }

    /// Saves this [`EdgeTimetable`] to a file on disk in `rkyv` format.
    pub fn save_rkyv(&self, path: impl AsRef<Path>) -> Result<(), RoutingError> {
        let file = File::create(path).map_err(|e| RoutingError::Snapshot(e.to_string()))?;
        let writer = BufWriter::new(file);
        self.write_rkyv(writer)
    }

    /// Loads an [`EdgeTimetable`] from an `rkyv` snapshot file on disk.
    pub fn load_rkyv(path: impl AsRef<Path>) -> Result<Self, RoutingError> {
        let file = File::open(path).map_err(|e| RoutingError::Snapshot(e.to_string()))?;
        let reader = BufReader::new(file);
        Self::read_rkyv(reader)
    }
}

fn find_best_route_departure(
    current_datetime: NaiveDateTime,
    deps: &[EdgeDeparture],
    walk_speed: Option<Velocity>,
) -> Result<Option<(EdgeDeparture, Time)>, RoutingError> {
    // Binary search for the first departure with src_departure_time >= current_datetime
    // (since boarding_delay >= 0, no departure prior to current_datetime can ever be boarded).
    let start_idx = deps.partition_point(|d| d.src_departure_time < current_datetime);
    let mut best: Option<(EdgeDeparture, Time)> = None;

    for dep in &deps[start_idx..] {
        if dep.dst_arrival_time < dep.src_departure_time {
            return Err(RoutingError::NegativeEdgeTravelTime {
                src_departure_time: dep
                    .src_departure_time
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string(),
                dst_arrival_time: dep.dst_arrival_time.format("%Y-%m-%d %H:%M:%S").to_string(),
            });
        }

        // Early exit: since departures are sorted ascending by src_departure_time and
        // dst_arrival_time >= src_departure_time, once src_departure_time >= best.dst_arrival_time,
        // no subsequent departure can arrive earlier than `best`.
        if let Some((ref best_dep, _)) = best {
            if dep.src_departure_time >= best_dep.dst_arrival_time {
                break;
            }
        }

        let boarding_delay = match walk_speed {
            Some(speed) => dep.boarding_walk_delay(speed),
            None => Time::new::<second>(0.0),
        };

        let delay_ms = (boarding_delay.get::<second>() * 1000.0).round() as i64;
        let ready_datetime = current_datetime
            .checked_add_signed(Duration::milliseconds(delay_ms))
            .unwrap_or(NaiveDateTime::MAX);

        if dep.src_departure_time < ready_datetime {
            continue;
        }

        match &best {
            None => {
                best = Some((*dep, boarding_delay));
            }
            Some((best_dep, _)) => {
                if dep.dst_arrival_time < best_dep.dst_arrival_time {
                    best = Some((*dep, boarding_delay));
                }
            }
        }
    }

    Ok(best)
}

/// Incremental builder that compiles `(edge_id, route_label, EdgeDeparture)` records into
/// a dense, sorted 3-array CSR [`EdgeTimetable`].
#[derive(Default, Debug, Clone)]
pub struct EdgeTimetableBuilder {
    edges: BTreeMap<usize, BTreeMap<i64, Vec<EdgeDeparture>>>,
    fq_route_ids: Vec<String>,
}

impl EdgeTimetableBuilder {
    /// Creates a new empty [`EdgeTimetableBuilder`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the catalog of unique fully-qualified route IDs (`fq_route_ids`).
    pub fn with_fq_route_ids(mut self, fq_route_ids: Vec<String>) -> Self {
        self.fq_route_ids = fq_route_ids;
        self
    }

    /// Ensures `edge_id` is registered in the builder even if all its departures are filtered out.
    pub fn ensure_edge(&mut self, edge_id: usize) {
        self.edges.entry(edge_id).or_default();
    }

    /// Inserts a departure for `(edge_id, route_label)`.
    pub fn add_departure(&mut self, edge_id: usize, route_label: i64, departure: EdgeDeparture) {
        self.edges
            .entry(edge_id)
            .or_default()
            .entry(route_label)
            .or_default()
            .push(departure);
    }

    /// Builds the contiguous CSR [`EdgeTimetable`], verifying that edge IDs form a dense
    /// `0..num_edges` sequence and sorting + deduplicating departures within each `(edge_id, route_label)`.
    pub fn build(self) -> Result<EdgeTimetable, RoutingError> {
        let num_edges = self.edges.len();
        if num_edges == 0 {
            return Ok(EdgeTimetable {
                edge_offsets: Vec::new(),
                route_schedules: Vec::new(),
                departures: Vec::new(),
                fq_route_ids: self.fq_route_ids,
            });
        }

        let mut edge_offsets = Vec::with_capacity(num_edges + 1);
        let mut route_schedules = Vec::new();
        let mut departures = Vec::new();

        let mut edges_map = self.edges;
        for edge_id in 0..num_edges {
            let route_map = edges_map
                .remove(&edge_id)
                .ok_or(RoutingError::InvalidEdgeId { edge_id, num_edges })?;

            edge_offsets.push(route_schedules.len() as u32);

            for (route_label, mut deps) in route_map {
                deps.sort_by(|a, b| {
                    a.src_departure_time
                        .cmp(&b.src_departure_time)
                        .then_with(|| a.dst_arrival_time.cmp(&b.dst_arrival_time))
                });
                deps.dedup();

                let departures_start = departures.len() as u32;
                departures.extend(deps);
                let departures_end = departures.len() as u32;

                route_schedules.push(EdgeRouteSchedule {
                    route_label,
                    departures_start,
                    departures_end,
                });
            }
        }
        edge_offsets.push(route_schedules.len() as u32);

        Ok(EdgeTimetable {
            edge_offsets,
            route_schedules,
            departures,
            fq_route_ids: self.fq_route_ids,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    #[test]
    fn test_edge_timetable_csr_lookup_walk_delay_and_overtaking() {
        let mut builder =
            EdgeTimetable::builder().with_fq_route_ids(vec!["route_a".into(), "route_b".into()]);

        // Edge 0, Route 0: boarding walk distance = 120m.
        // At walk_speed = 1.0 m/s, boarding_delay = 120s (2 minutes).
        // Departure 1 at 08:01:00 (arrives 08:10:00) -> missed if boarding from road at 08:00:00,
        //                                               but caught if already on Route 0!
        // Departure 2 at 08:03:00 (arrives 08:15:00) -> local bus
        // Departure 3 at 08:04:00 (arrives 08:12:00) -> express bus that overtakes Departure 2!
        builder.add_departure(
            0,
            0,
            EdgeDeparture::new(
                dt("2025-06-15 08:01:00"),
                dt("2025-06-15 08:10:00"),
                Length::new::<meter>(120.0),
                Length::new::<meter>(50.0),
            ),
        );
        builder.add_departure(
            0,
            0,
            EdgeDeparture::new(
                dt("2025-06-15 08:03:00"),
                dt("2025-06-15 08:15:00"),
                Length::new::<meter>(120.0),
                Length::new::<meter>(50.0),
            ),
        );
        builder.add_departure(
            0,
            0,
            EdgeDeparture::new(
                dt("2025-06-15 08:04:00"),
                dt("2025-06-15 08:12:00"),
                Length::new::<meter>(120.0),
                Length::new::<meter>(50.0),
            ),
        );

        let timetable = builder.build().unwrap();
        assert_eq!(timetable.num_edges(), 1);
        assert_eq!(timetable.edge_offsets, vec![0, 1]);
        assert_eq!(timetable.route_schedules.len(), 1);
        assert_eq!(timetable.departures.len(), 3);

        let query_time = dt("2025-06-15 08:00:00");
        let walk_speed = Velocity::new::<meter_per_second>(1.0);

        // 1. Rider is NOT currently on Route 0 (`current_route_id = None`):
        //    - Needs 120s to walk 120m -> reaches stop at 08:02:00 (misses 08:01:00 departure).
        //    - Between 08:03:00 (arr 08:15) and 08:04:00 (arr 08:12), picks 08:04:00 express!
        let (r_label, dep, delay) = timetable
            .get_next_departure(0, &query_time, None, Some(walk_speed))
            .unwrap()
            .unwrap();
        assert_eq!(r_label, 0);
        assert_eq!(dep.src_departure_time, dt("2025-06-15 08:04:00"));
        assert_eq!(dep.dst_arrival_time, dt("2025-06-15 08:12:00"));
        assert!((delay.get::<second>() - 120.0).abs() < 1e-6);
        assert!((dep.travel_time().unwrap().get::<second>() - 480.0).abs() < 1e-6);

        // 2. Rider IS already on Route 0 (`current_route_id = Some(0)`):
        //    - Boarding walk delay is 0s -> catches the 08:01:00 departure (arrives 08:10:00)!
        let (r_label_cont, dep_cont, delay_cont) = timetable
            .get_next_departure(0, &query_time, Some(0), Some(walk_speed))
            .unwrap()
            .unwrap();
        assert_eq!(r_label_cont, 0);
        assert_eq!(dep_cont.src_departure_time, dt("2025-06-15 08:01:00"));
        assert_eq!(dep_cont.dst_arrival_time, dt("2025-06-15 08:10:00"));
        assert_eq!(delay_cont.get::<second>(), 0.0);
    }

    #[test]
    fn test_edge_timetable_rkyv_roundtrip_and_dense_validation() {
        let mut builder = EdgeTimetable::builder().with_fq_route_ids(vec!["r0".into()]);
        builder.add_departure(
            0,
            0,
            EdgeDeparture::without_walk_distance(
                dt("2025-06-15 09:00:00"),
                dt("2025-06-15 09:10:00"),
            ),
        );
        builder.ensure_edge(1); // Edge 1 has no departures but keeps edge IDs dense
        let original = builder.build().unwrap();
        assert_eq!(original.num_edges(), 2);

        let bytes = original.to_rkyv_bytes().unwrap();
        let archived = EdgeTimetable::access_rkyv_bytes(&bytes).unwrap();
        assert_eq!(archived.edge_offsets.len(), 3);
        let restored = EdgeTimetable::from_rkyv_bytes(&bytes).unwrap();
        assert_eq!(restored, original);

        // Missing edge 0 when edge 1 is inserted fails dense validation
        let mut sparse_builder = EdgeTimetable::builder();
        sparse_builder.ensure_edge(1);
        assert!(matches!(
            sparse_builder.build(),
            Err(RoutingError::InvalidEdgeId {
                edge_id: 0,
                num_edges: 1
            })
        ));
    }
}
