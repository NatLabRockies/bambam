//! Multi-feed [`RaptorTimetableBuilder`] for compiling [`ValidatedFeed`]s into a contiguous [`RaptorTimetable`].

use crate::enums::TransferType;
use crate::ids::*;
use crate::model::{
    BoardingPolicy, Route, ServiceCalendar, ShapePolyline, StopPlace, StopTime, StopTimeTiming,
    Trip, ValidatedFeed, ValidationPolicy, ValidationWarning,
};
use crate::primitives::GtfsTime;
use crate::routing::compact_stop_time::NO_FLEX_DURATION_IDX;
use crate::routing::spatial_index::build_place_spatial_index;
use crate::routing::{
    CompactStopTime, FeedMetadata, IndexedTransfer, PatternStopProfile, PlacePatternOccurrence,
    RaptorTimetable, RoutableTrip, RoutePattern, RoutingError,
};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::Arc;

#[cfg(feature = "gtfs-flex")]
use crate::model::FlexDurationParams;

#[derive(Clone, PartialEq, Eq, Hash)]
struct PatternKey {
    feed_idx: FeedIdx,
    route_idx: RouteIdx,
    places: Vec<PlaceIdx>,
    pickup_methods: Vec<i16>,
    drop_off_methods: Vec<i16>,
}

fn pickup_code(p: &BoardingPolicy) -> i16 {
    match p.method {
        crate::enums::PickupDropOffType::Regular => 0,
        crate::enums::PickupDropOffType::NotAvailable => 1,
        crate::enums::PickupDropOffType::ArrangeByPhone => 2,
        crate::enums::PickupDropOffType::CoordinateWithDriver => 3,
        crate::enums::PickupDropOffType::Unknown(c) => c,
    }
}

/// Multi-feed builder that incrementally compiles and merges one or more [`ValidatedFeed`]s
/// into a single contiguous [`RaptorTimetable`].
///
/// Designed for HPC workflows where each GTFS archive is parsed and validated into a
/// [`ValidatedFeed`], appended via [`RaptorTimetableBuilder::add_feed`], and immediately dropped
/// so peak memory never exceeds the compact CSR arena.
#[derive(Default)]
pub struct RaptorTimetableBuilder {
    timetable: RaptorTimetable,
    pattern_lookup: HashMap<PatternKey, PatternIdx>,
    validation_policy: ValidationPolicy,
    materialize_frequency_trips: bool,
    #[cfg(feature = "gtfs-flex")]
    flex_duration_lookup: HashMap<[u64; 4], u16>,
}

impl RaptorTimetableBuilder {
    /// Creates an empty [`RaptorTimetableBuilder`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the [`ValidationPolicy`] used when interpolating and packing trips in Phase 3.
    ///
    /// When set to [`ValidationPolicy::DropInvalidTripsWithWarning`], trips with 0 or 1 stops,
    /// missing boundary timestamps (`UninterpolatableTripBoundary`), or decreasing stop times
    /// (`DecreasingStopTimes`) are dropped and recorded on [`FeedMetadata::warnings`] and
    /// [`FeedMetadata::dropped_counts`] instead of aborting the build.
    pub fn with_validation_policy(mut self, policy: ValidationPolicy) -> Self {
        self.validation_policy = policy;
        self
    }

    /// Configures whether headway-based `frequencies.txt` trips should be materialized into concrete
    /// [`RoutableTrip`] instances (with `CompactStopTime` entries shifted for each headway departure
    /// and `frequencies` cleared on the expanded trips).
    ///
    /// Note that [`RaptorTimetable::earliest_boardable_trip`] supports both materialized and
    /// unexpanded frequency trips automatically.
    pub fn with_materialize_frequencies(mut self, materialize: bool) -> Self {
        self.materialize_frequency_trips = materialize;
        self
    }

    /// Appends a [`ValidatedFeed`] under `feed_id` into the contiguous [`RaptorTimetable`] arena,
    /// returning its assigned [`FeedIdx`].
    pub fn add_feed(
        &mut self,
        feed_id: impl Into<FeedId>,
        mut feed: ValidatedFeed,
    ) -> Result<FeedIdx, RoutingError> {
        let feed_id = feed_id.into();
        let feed_idx = FeedIdx::from_usize(self.timetable.feeds.len());
        self.timetable
            .feed_id_to_idx
            .insert(feed_id.clone(), feed_idx);

        let places_start = self.timetable.places.len() as u32;
        let routes_start = self.timetable.routes.len() as u32;
        let patterns_start = self.timetable.patterns.len() as u32;
        let trips_start = self.timetable.trips.len() as u32;
        let services_start = self.timetable.services.len() as u32;
        let shapes_start = self.timetable.shapes.len() as u32;

        let mut local_stop_to_place: HashMap<StopId, PlaceIdx> =
            HashMap::with_capacity(feed.stops.len());
        #[cfg(feature = "gtfs-flex")]
        let mut local_loc_to_place: HashMap<LocationId, PlaceIdx> =
            HashMap::with_capacity(feed.locations.len());
        #[cfg(feature = "gtfs-flex")]
        let mut local_grp_to_place: HashMap<LocationGroupId, PlaceIdx> =
            HashMap::with_capacity(feed.location_groups.len());

        // 1. Append Places (Stops, FlexLocations, LocationGroups)
        let mut sorted_stops: Vec<_> = feed.stops.into_values().collect();
        sorted_stops.sort_by(|a, b| a.id.cmp(&b.id));
        for stop in sorted_stops {
            let idx = PlaceIdx::from_usize(self.timetable.places.len());
            local_stop_to_place.insert(stop.id.clone(), idx);
            self.timetable
                .scoped_stop_to_place
                .insert((feed_idx, stop.id.clone()), idx);
            self.timetable.places.push(StopPlace::Stop(stop));
            self.timetable.place_feed.push(feed_idx);
            self.timetable.place_to_patterns.push(Vec::new());
            self.timetable.place_to_transfers.push(Vec::new());
            self.timetable.group_to_member_places.push(Vec::new());
        }

        #[cfg(feature = "gtfs-flex")]
        {
            let mut sorted_locs: Vec<_> = feed.locations.into_values().collect();
            sorted_locs.sort_by(|a, b| a.id.cmp(&b.id));
            for loc in sorted_locs {
                let idx = PlaceIdx::from_usize(self.timetable.places.len());
                local_loc_to_place.insert(loc.id.clone(), idx);
                self.timetable
                    .scoped_location_to_place
                    .insert((feed_idx, loc.id.clone()), idx);
                self.timetable.places.push(StopPlace::FlexZone(loc));
                self.timetable.place_feed.push(feed_idx);
                self.timetable.place_to_patterns.push(Vec::new());
                self.timetable.place_to_transfers.push(Vec::new());
                self.timetable.group_to_member_places.push(Vec::new());
            }

            let mut sorted_groups: Vec<_> = feed.location_groups.into_values().collect();
            sorted_groups.sort_by(|a, b| a.id.cmp(&b.id));
            for grp in sorted_groups {
                let idx = PlaceIdx::from_usize(self.timetable.places.len());
                local_grp_to_place.insert(grp.id.clone(), idx);
                self.timetable
                    .scoped_location_group_to_place
                    .insert((feed_idx, grp.id.clone()), idx);

                let mut member_indices = Vec::with_capacity(grp.members.len());
                for m in &grp.members {
                    match m {
                        crate::model::PlaceMember::Stop(s) => {
                            if let Some(&pidx) = local_stop_to_place.get(&s.id) {
                                member_indices.push(pidx);
                            }
                        }
                        crate::model::PlaceMember::FlexZone(z) => {
                            if let Some(&pidx) = local_loc_to_place.get(&z.id) {
                                member_indices.push(pidx);
                            }
                        }
                    }
                }

                self.timetable.places.push(StopPlace::LocationGroup(grp));
                self.timetable.place_feed.push(feed_idx);
                self.timetable.place_to_patterns.push(Vec::new());
                self.timetable.place_to_transfers.push(Vec::new());
                self.timetable.group_to_member_places.push(member_indices);
            }
        }

        let places_end = self.timetable.places.len() as u32;

        // 2. Append Transfers & Pathways for this feed's places
        for place_u32 in places_start..places_end {
            let i = place_u32 as usize;
            let from_place = PlaceIdx::new(place_u32);
            if let Some(stop) = self.timetable.places[i].as_stop() {
                let mut edges = Vec::with_capacity(stop.transfers.len() + stop.pathways.len());
                for tr in &stop.transfers {
                    if let Some(&to_place) = local_stop_to_place.get(&tr.to_stop_id) {
                        edges.push(IndexedTransfer {
                            from_place,
                            to_place,
                            transfer_type: tr.transfer_type,
                            min_time_seconds: tr.min_transfer_time,
                            distance_meters: None,
                        });
                    }
                }
                for pw in &stop.pathways {
                    if let Some(&to_place) = local_stop_to_place.get(&pw.to_stop_id) {
                        edges.push(IndexedTransfer {
                            from_place,
                            to_place,
                            transfer_type: TransferType::MinTime,
                            min_time_seconds: pw.traversal_time,
                            distance_meters: pw.length,
                        });
                    }
                }
                self.timetable.place_to_transfers[i] = edges;
            }
        }

        // 3. Append Routes
        let mut sorted_routes: Vec<Route> = feed.routes.into_values().collect();
        sorted_routes.sort_by(|a, b| a.id.cmp(&b.id));
        let mut local_route_to_idx: HashMap<RouteId, RouteIdx> =
            HashMap::with_capacity(sorted_routes.len());
        for route in sorted_routes {
            let ridx = RouteIdx::from_usize(self.timetable.routes.len());
            local_route_to_idx.insert(route.id.clone(), ridx);
            self.timetable
                .scoped_route_to_idx
                .insert((feed_idx, route.id.clone()), ridx);
            self.timetable.routes.push(route);
            self.timetable.route_feed.push(feed_idx);
            self.timetable.route_to_patterns.push(Vec::new());
        }
        let routes_end = self.timetable.routes.len() as u32;

        // 4. Append Services
        let mut sorted_services: Vec<ServiceCalendar> = feed.services.into_values().collect();
        sorted_services.sort_by(|a, b| a.id.cmp(&b.id));
        let mut local_service_to_idx: HashMap<ServiceId, ServiceIdx> =
            HashMap::with_capacity(sorted_services.len());
        for svc in sorted_services {
            let sidx = ServiceIdx::from_usize(self.timetable.services.len());
            local_service_to_idx.insert(svc.id.clone(), sidx);
            self.timetable
                .scoped_service_to_idx
                .insert((feed_idx, svc.id.clone()), sidx);
            self.timetable.services.push(svc);
            self.timetable.service_feed.push(feed_idx);
        }

        // 5. Append Shapes
        let mut sorted_shapes: Vec<Arc<ShapePolyline>> = feed.shapes.into_values().collect();
        sorted_shapes.sort_by(|a, b| a.id.cmp(&b.id));
        let mut local_shape_to_idx: HashMap<ShapeId, ShapeIdx> =
            HashMap::with_capacity(sorted_shapes.len());
        for shp in sorted_shapes {
            let shidx = ShapeIdx::from_usize(self.timetable.shapes.len());
            local_shape_to_idx.insert(shp.id.clone(), shidx);
            self.timetable
                .scoped_shape_to_idx
                .insert((feed_idx, shp.id.clone()), shidx);
            self.timetable.shapes.push(shp);
            self.timetable.shape_feed.push(feed_idx);
        }
        let shapes_end = self.timetable.shapes.len() as u32;

        // 6. Interpolate Trips & Append to Flat CSR StopTimes Arena
        let mut raw_trips: Vec<Trip> = feed.trips.into_values().collect();
        raw_trips.sort_by(|a, b| a.id.cmp(&b.id));

        for mut trip in raw_trips {
            if self.validation_policy == ValidationPolicy::DropInvalidTripsWithWarning {
                if trip.stop_times.is_empty() {
                    feed.dropped_counts.empty_trips += 1;
                    feed.warnings.push(ValidationWarning::DroppedTrip {
                        trip_id: trip.id.to_string(),
                        reason: "trip has 0 stop_times rows".to_owned(),
                    });
                    continue;
                }
                if trip.stop_times.len() == 1 {
                    feed.dropped_counts.single_stop_trips += 1;
                    feed.warnings.push(ValidationWarning::DroppedTrip {
                        trip_id: trip.id.to_string(),
                        reason: "trip has only 1 stop_times row".to_owned(),
                    });
                    continue;
                }
            }

            let service_idx = match local_service_to_idx.get(&trip.service_id).copied() {
                Some(idx) => idx,
                None => {
                    let sidx = ServiceIdx::from_usize(self.timetable.services.len());
                    local_service_to_idx.insert(trip.service_id.clone(), sidx);
                    self.timetable
                        .scoped_service_to_idx
                        .insert((feed_idx, trip.service_id.clone()), sidx);
                    self.timetable
                        .services
                        .push(ServiceCalendar::new(trip.service_id.clone()));
                    self.timetable.service_feed.push(feed_idx);
                    sidx
                }
            };

            let route_idx = local_route_to_idx[&trip.route_id];
            let shape_idx = trip
                .shape_id
                .as_ref()
                .and_then(|sid| local_shape_to_idx.get(sid).copied());

            let (place_indices, compact_sts) = match self.interpolate_and_pack_stop_times(
                &trip.id,
                &mut trip.stop_times,
                &local_stop_to_place,
                #[cfg(feature = "gtfs-flex")]
                &local_loc_to_place,
                #[cfg(feature = "gtfs-flex")]
                &local_grp_to_place,
            ) {
                Ok(res) => res,
                Err(err) => match self.validation_policy {
                    ValidationPolicy::Strict => return Err(err),
                    ValidationPolicy::DropInvalidTripsWithWarning => {
                        feed.dropped_counts.malformed_trips += 1;
                        feed.warnings.push(ValidationWarning::DroppedTrip {
                            trip_id: trip.id.to_string(),
                            reason: err.to_string(),
                        });
                        continue;
                    }
                },
            };

            let key = PatternKey {
                feed_idx,
                route_idx,
                places: place_indices.clone(),
                pickup_methods: trip
                    .stop_times
                    .iter()
                    .map(|s| pickup_code(&s.pickup))
                    .collect(),
                drop_off_methods: trip
                    .stop_times
                    .iter()
                    .map(|s| pickup_code(&s.drop_off))
                    .collect(),
            };

            let pattern_idx = match self.pattern_lookup.get(&key).copied() {
                Some(pidx) => pidx,
                None => {
                    let pidx = PatternIdx::from_usize(self.timetable.patterns.len());
                    let stops_profile: Vec<PatternStopProfile> = trip
                        .stop_times
                        .iter()
                        .zip(place_indices.iter())
                        .map(|(st, &place_idx)| PatternStopProfile {
                            place_idx,
                            pickup: st.pickup.clone(),
                            drop_off: st.drop_off.clone(),
                        })
                        .collect();

                    for (offset, prof) in stops_profile.iter().enumerate() {
                        let occ = PlacePatternOccurrence {
                            pattern_idx: pidx,
                            stop_offset_in_pattern: offset as u16,
                        };
                        self.timetable.place_to_patterns[prof.place_idx.index()].push(occ);
                        let member_places =
                            self.timetable.group_to_member_places[prof.place_idx.index()].clone();
                        for member_pidx in member_places {
                            self.timetable.place_to_patterns[member_pidx.index()].push(occ);
                        }
                    }

                    self.timetable.route_to_patterns[route_idx.index()].push(pidx);
                    self.timetable.patterns.push(RoutePattern {
                        idx: pidx,
                        feed_idx,
                        route_idx,
                        stops: stops_profile,
                        trips: Vec::new(),
                    });
                    self.pattern_lookup.insert(key, pidx);
                    pidx
                }
            };

            if self.materialize_frequency_trips
                && !trip.frequencies.is_empty()
                && !compact_sts.is_empty()
                && compact_sts.iter().all(|st| !st.is_flex_window())
            {
                let t0_secs = compact_sts[0].departure_or_end_secs as i64;
                let mut instance_counter = 0usize;
                for freq in &trip.frequencies {
                    if freq.headway_secs == 0 {
                        continue;
                    }
                    let mut dep0 = freq.window.start.0 as i64;
                    let end0 = freq.window.end.0 as i64;
                    let headway = freq.headway_secs as i64;
                    while dep0 < end0 || (instance_counter == 0 && dep0 == end0) {
                        let offset_secs = dep0 - t0_secs;
                        let shifted_sts: Vec<CompactStopTime> = compact_sts
                            .iter()
                            .map(|st| CompactStopTime {
                                arrival_or_start_secs: GtfsTime(st.arrival_or_start_secs)
                                    .add_signed_seconds(offset_secs)
                                    .0,
                                departure_or_end_secs: GtfsTime(st.departure_or_end_secs)
                                    .add_signed_seconds(offset_secs)
                                    .0,
                                ..*st
                            })
                            .collect();

                        let stop_times_offset = self.timetable.stop_times.len() as u32;
                        let stop_count = shifted_sts.len() as u16;
                        self.timetable.stop_times.extend(shifted_sts);

                        let trip_idx = TripIdx::from_usize(self.timetable.trips.len());
                        let expanded_id = if instance_counter == 0 {
                            trip.id.clone()
                        } else {
                            TripId::new(format!("{}#freq_{}", trip.id, instance_counter))
                        };
                        self.timetable
                            .scoped_trip_to_idx
                            .insert((feed_idx, expanded_id.clone()), trip_idx);
                        self.timetable.patterns[pattern_idx.index()]
                            .trips
                            .push(trip_idx);

                        self.timetable.trips.push(RoutableTrip {
                            idx: trip_idx,
                            feed_idx,
                            id: expanded_id,
                            route_idx,
                            pattern_idx,
                            service_idx,
                            shape_idx,
                            headsign: trip.trip_headsign.clone(),
                            stop_times_offset,
                            stop_count,
                            frequencies: Vec::new(),
                        });

                        instance_counter += 1;
                        dep0 += headway;
                    }
                }
            } else {
                let stop_times_offset = self.timetable.stop_times.len() as u32;
                let stop_count = compact_sts.len() as u16;
                self.timetable.stop_times.extend(compact_sts);

                let trip_idx = TripIdx::from_usize(self.timetable.trips.len());
                self.timetable
                    .scoped_trip_to_idx
                    .insert((feed_idx, trip.id.clone()), trip_idx);
                self.timetable.patterns[pattern_idx.index()]
                    .trips
                    .push(trip_idx);

                self.timetable.trips.push(RoutableTrip {
                    idx: trip_idx,
                    feed_idx,
                    id: trip.id,
                    route_idx,
                    pattern_idx,
                    service_idx,
                    shape_idx,
                    headsign: trip.trip_headsign,
                    stop_times_offset,
                    stop_count,
                    frequencies: trip.frequencies,
                });
            }
        }

        let patterns_end = self.timetable.patterns.len() as u32;
        let trips_end = self.timetable.trips.len() as u32;
        let services_end = self.timetable.services.len() as u32;

        // Sort trips within each newly created pattern by earliest time at the first stop
        for pat_u32 in patterns_start..patterns_end {
            let pattern = &mut self.timetable.patterns[pat_u32 as usize];
            let trips_ref = &self.timetable.trips;
            let st_arena = &self.timetable.stop_times;
            pattern.trips.sort_by_key(|&tidx| {
                let t = &trips_ref[tidx.index()];
                if t.stop_count > 0 {
                    st_arena[t.stop_times_offset as usize].arrival_or_start_secs
                } else {
                    0
                }
            });
        }

        let primary_timezone = feed.agencies.first().map(|a| a.timezone);

        self.timetable.feeds.push(FeedMetadata {
            idx: feed_idx,
            id: feed_id,
            agencies: feed.agencies,
            primary_timezone,
            places_span: places_start..places_end,
            routes_span: routes_start..routes_end,
            patterns_span: patterns_start..patterns_end,
            trips_span: trips_start..trips_end,
            services_span: services_start..services_end,
            shapes_span: shapes_start..shapes_end,
            warnings: feed.warnings,
            dropped_counts: feed.dropped_counts,
        });

        Ok(feed_idx)
    }

    /// Consumes the builder, bulk-loads the [`rstar::RTree`] spatial index over all places,
    /// and returns the completed [`RaptorTimetable`].
    pub fn build(mut self) -> RaptorTimetable {
        self.timetable.stop_times.shrink_to_fit();
        self.timetable.places.shrink_to_fit();
        self.timetable.trips.shrink_to_fit();
        self.timetable.patterns.shrink_to_fit();
        self.timetable.place_spatial_index = build_place_spatial_index(&self.timetable.places);
        self.timetable
    }

    #[cfg(feature = "gtfs-flex")]
    fn intern_flex_duration(&mut self, params: FlexDurationParams) -> u16 {
        if !params.is_some() {
            return NO_FLEX_DURATION_IDX;
        }
        let key = [
            params.mean_duration_factor.unwrap_or(f64::NAN).to_bits(),
            params.mean_duration_offset.unwrap_or(f64::NAN).to_bits(),
            params.safe_duration_factor.unwrap_or(f64::NAN).to_bits(),
            params.safe_duration_offset.unwrap_or(f64::NAN).to_bits(),
        ];
        if let Some(&idx) = self.flex_duration_lookup.get(&key) {
            return idx;
        }
        let idx = self
            .timetable
            .flex_duration_table
            .len()
            .min(u16::MAX as usize - 1) as u16;
        self.timetable.flex_duration_table.push(params);
        self.flex_duration_lookup.insert(key, idx);
        idx
    }

    fn interpolate_and_pack_stop_times(
        &mut self,
        trip_id: &TripId,
        stop_times: &mut [StopTime],
        stop_id_to_place: &HashMap<StopId, PlaceIdx>,
        #[cfg(feature = "gtfs-flex")] location_id_to_place: &HashMap<LocationId, PlaceIdx>,
        #[cfg(feature = "gtfs-flex")] location_group_id_to_place: &HashMap<
            LocationGroupId,
            PlaceIdx,
        >,
    ) -> Result<(Vec<PlaceIdx>, Vec<CompactStopTime>), RoutingError> {
        let n = stop_times.len();
        if n == 0 {
            return Ok((Vec::new(), Vec::new()));
        }

        let mut fixed_times: Vec<Option<(GtfsTime, GtfsTime)>> = vec![None; n];
        for (i, st) in stop_times.iter().enumerate() {
            match st.timing {
                StopTimeTiming::Scheduled { arrival, departure } => {
                    fixed_times[i] = Some((arrival, departure));
                }
                StopTimeTiming::Uninterpolated {
                    arrival: Some(a),
                    departure: None,
                } => {
                    fixed_times[i] = Some((a, a));
                }
                StopTimeTiming::Uninterpolated {
                    arrival: None,
                    departure: Some(d),
                } => {
                    fixed_times[i] = Some((d, d));
                }
                StopTimeTiming::Uninterpolated {
                    arrival: Some(a),
                    departure: Some(d),
                } => {
                    fixed_times[i] = Some((a, d));
                }
                StopTimeTiming::Uninterpolated {
                    arrival: None,
                    departure: None,
                } => {}
                #[cfg(feature = "gtfs-flex")]
                StopTimeTiming::FlexWindow(_) => {}
            }
        }

        let mut i = 0;
        while i < n {
            if matches!(
                stop_times[i].timing,
                StopTimeTiming::Uninterpolated {
                    arrival: None,
                    departure: None
                }
            ) {
                if i == 0 {
                    return Err(RoutingError::UninterpolatableTripBoundary {
                        trip_id: trip_id.to_string(),
                        stop_sequence: stop_times[0].stop_sequence,
                    });
                }
                let prev_idx = i - 1;
                let Some((_, start_dep)) = fixed_times[prev_idx] else {
                    return Err(RoutingError::UninterpolatableTripBoundary {
                        trip_id: trip_id.to_string(),
                        stop_sequence: stop_times[i].stop_sequence,
                    });
                };

                let mut next_idx = i + 1;
                while next_idx < n
                    && matches!(
                        stop_times[next_idx].timing,
                        StopTimeTiming::Uninterpolated {
                            arrival: None,
                            departure: None
                        }
                    )
                {
                    next_idx += 1;
                }
                if next_idx >= n {
                    return Err(RoutingError::UninterpolatableTripBoundary {
                        trip_id: trip_id.to_string(),
                        stop_sequence: stop_times[n - 1].stop_sequence,
                    });
                }
                let Some((end_arr, _)) = fixed_times[next_idx] else {
                    return Err(RoutingError::UninterpolatableTripBoundary {
                        trip_id: trip_id.to_string(),
                        stop_sequence: stop_times[next_idx].stop_sequence,
                    });
                };

                let start_arc = stop_times[prev_idx].shape_ref.map(|r| r.arc_meters);
                let end_arc = stop_times[next_idx].shape_ref.map(|r| r.arc_meters);

                for k in i..next_idx {
                    let ratio = match (
                        start_arc,
                        end_arc,
                        stop_times[k].shape_ref.map(|r| r.arc_meters),
                    ) {
                        (Some(s_m), Some(e_m), Some(k_m)) if e_m > s_m => (k_m - s_m) / (e_m - s_m),
                        _ => (k - prev_idx) as f64 / (next_idx - prev_idx) as f64,
                    };
                    let interp_t = GtfsTime::linear_interpolate(start_dep, end_arr, ratio);
                    fixed_times[k] = Some((interp_t, interp_t));
                }
                i = next_idx;
            } else {
                i += 1;
            }
        }

        let mut prev_time: Option<GtfsTime> = None;
        let mut place_indices = Vec::with_capacity(n);
        let mut compact_sts = Vec::with_capacity(n);

        for (idx, st) in stop_times.iter().enumerate() {
            let place_idx = resolve_place_idx(
                &st.place,
                stop_id_to_place,
                #[cfg(feature = "gtfs-flex")]
                location_id_to_place,
                #[cfg(feature = "gtfs-flex")]
                location_group_id_to_place,
            );
            place_indices.push(place_idx);

            let shape_arc_meters = st
                .shape_ref
                .map(|r| r.arc_meters as f32)
                .unwrap_or(f32::NAN);

            #[cfg(feature = "gtfs-flex")]
            let flex_duration_idx = self.intern_flex_duration(st.flex_duration);
            #[cfg(not(feature = "gtfs-flex"))]
            let flex_duration_idx = NO_FLEX_DURATION_IDX;

            let compact = match st.timing {
                #[cfg(feature = "gtfs-flex")]
                StopTimeTiming::FlexWindow(w) => {
                    CompactStopTime::new_flex_window(w, shape_arc_meters, flex_duration_idx)
                }
                _ => {
                    let (arr, dep) =
                        fixed_times[idx].expect("all non-flex stop times resolved above");
                    if let Some(prev) = prev_time {
                        if arr < prev || dep < arr {
                            return Err(RoutingError::DecreasingStopTimes {
                                trip_id: trip_id.to_string(),
                                stop_sequence: st.stop_sequence,
                            });
                        }
                    }
                    prev_time = Some(dep);
                    CompactStopTime::new_fixed(arr, dep, shape_arc_meters, flex_duration_idx)
                }
            };

            compact_sts.push(compact);
        }

        Ok((place_indices, compact_sts))
    }
}

impl TryFrom<ValidatedFeed> for RaptorTimetable {
    type Error = RoutingError;

    fn try_from(feed: ValidatedFeed) -> Result<Self, Self::Error> {
        let mut builder = RaptorTimetableBuilder::new();
        builder.add_feed(FeedId::new("default"), feed)?;
        Ok(builder.build())
    }
}

fn resolve_place_idx(
    place: &StopPlace,
    stop_id_to_place: &HashMap<StopId, PlaceIdx>,
    #[cfg(feature = "gtfs-flex")] location_id_to_place: &HashMap<LocationId, PlaceIdx>,
    #[cfg(feature = "gtfs-flex")] location_group_id_to_place: &HashMap<LocationGroupId, PlaceIdx>,
) -> PlaceIdx {
    match place {
        StopPlace::Stop(s) => stop_id_to_place[&s.id],
        #[cfg(feature = "gtfs-flex")]
        StopPlace::FlexZone(z) => location_id_to_place[&z.id],
        #[cfg(feature = "gtfs-flex")]
        StopPlace::LocationGroup(g) => location_group_id_to_place[&g.id],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::PickupDropOffType;
    use crate::raw::{RawFeed, RawStopTime, RawTrip};
    use crate::traits::TemporalStopTime;
    use std::path::PathBuf;

    fn fixture_path(rel_path: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel_path)
    }

    #[test]
    fn test_interpolated_stop_times_in_phase_3() {
        // Notice: this single test compiles and passes both WITH and WITHOUT `--features gtfs-flex`!
        let validated = ValidatedFeed::from_path(fixture_path("fixtures/interpolated_stop_times"))
            .expect("Phase 2 should accept uninterpolated intermediate stop times");
        let trip1 = &validated.trips[&TripId::new("trip1")];
        assert!(matches!(
            trip1.stop_times[1].timing,
            StopTimeTiming::Uninterpolated {
                arrival: None,
                departure: None
            }
        ));

        // Phase 3 interpolates the missing intermediate stop time between 14:00:00 and 15:00:00
        let timetable =
            RaptorTimetable::try_from(validated).expect("Phase 3 should interpolate stop times");
        let st_slice = timetable.trip_stop_times(TripIdx::new(0));
        assert_eq!(st_slice.len(), 3);

        let st0_dep = st_slice[0].departure_time().unwrap();
        let st1_arr = st_slice[1].arrival_time().unwrap();
        let st2_arr = st_slice[2].arrival_time().unwrap();
        assert!(
            st1_arr > st0_dep && st1_arr < st2_arr,
            "interpolated time {st1_arr} should be strictly between {st0_dep} and {st2_arr}"
        );
    }

    #[test]
    fn test_interpolation_shape_distance_weighted_and_error_cases() {
        // 1. Shape distance-weighted interpolation vs uniform step interpolation
        let mut raw = RawFeed::from_path(fixture_path("fixtures/interpolated_stop_times")).unwrap();
        raw.trips[0].shape_id = Some(ShapeId::new("A_shp"));
        // A_shp has raw distances 0 -> 20 -> 50; assign raw_dist 5.0 (25% of 20.0) to stop 1
        raw.stop_times[0].shape_dist_traveled = Some(0.0);
        raw.stop_times[1].shape_dist_traveled = Some(5.0);
        raw.stop_times[2].shape_dist_traveled = Some(20.0);

        let validated = ValidatedFeed::try_from(raw).unwrap();
        let timetable = RaptorTimetable::try_from(validated).unwrap();
        let st1 = timetable.trip_stop_times(TripIdx::new(0))[1];
        // Since shape_ref is populated asymmetrically near the start, interpolated time is closer to 14:00 than 14:30
        assert!(st1.arrival_time().unwrap() < GtfsTime::from_hms(14, 30, 0));

        // 2. UninterpolatableTripBoundary when stop 0 is missing times
        let mut bad_boundary_raw =
            RawFeed::from_path(fixture_path("fixtures/interpolated_stop_times")).unwrap();
        bad_boundary_raw.stop_times[0].arrival_time = None;
        bad_boundary_raw.stop_times[0].departure_time = None;
        let bad_boundary_val = ValidatedFeed::try_from(bad_boundary_raw).unwrap();
        assert!(matches!(
            RaptorTimetable::try_from(bad_boundary_val),
            Err(RoutingError::UninterpolatableTripBoundary { .. })
        ));

        // 3. DecreasingStopTimes when scheduled times go backwards across stops
        let mut dec_raw = RawFeed::from_path(fixture_path("fixtures/basic")).unwrap();
        dec_raw.stop_times[1].arrival_time = Some(GtfsTime::from_hms(13, 0, 0));
        dec_raw.stop_times[1].departure_time = Some(GtfsTime::from_hms(13, 0, 0));
        let dec_val = ValidatedFeed::try_from(dec_raw).unwrap();
        assert!(matches!(
            RaptorTimetable::try_from(dec_val),
            Err(RoutingError::DecreasingStopTimes { .. })
        ));
    }

    #[test]
    fn test_pattern_deduplication_and_lenient_phase_3_policy() {
        let mut raw = RawFeed::from_path(fixture_path("fixtures/basic")).unwrap();
        raw.frequencies = None;

        // Add trip2 with identical stop sequence and pickup/drop-off profile -> shares PatternIdx
        raw.trips.push(RawTrip {
            id: TripId::new("trip2"),
            service_id: ServiceId::new("service1"),
            route_id: RouteId::new("route1"),
            ..Default::default()
        });
        let template_sts: Vec<_> = raw
            .stop_times
            .iter()
            .filter(|st| st.trip_id.as_str() == "trip1")
            .cloned()
            .collect();
        for mut st in template_sts.clone() {
            st.trip_id = TripId::new("trip2");
            st.arrival_time = st.arrival_time.map(|t| t.add_seconds(7200));
            st.departure_time = st.departure_time.map(|t| t.add_seconds(7200));
            raw.stop_times.push(st);
        }

        // Add trip3 with same stops but pickup_type = NotAvailable on stop 0 -> distinct PatternIdx
        raw.trips.push(RawTrip {
            id: TripId::new("trip3"),
            service_id: ServiceId::new("service1"),
            route_id: RouteId::new("route1"),
            ..Default::default()
        });
        for (i, mut st) in template_sts.into_iter().enumerate() {
            st.trip_id = TripId::new("trip3");
            st.arrival_time = st.arrival_time.map(|t| t.add_seconds(14400));
            st.departure_time = st.departure_time.map(|t| t.add_seconds(14400));
            if i == 0 {
                st.pickup_type = PickupDropOffType::NotAvailable;
            }
            raw.stop_times.push(st);
        }

        // Also inject a decreasing-stop-time trip to verify Phase 3 DropInvalidTripsWithWarning
        raw.trips.push(RawTrip {
            id: TripId::new("dirty_decreasing_trip"),
            service_id: ServiceId::new("service1"),
            route_id: RouteId::new("route1"),
            ..Default::default()
        });
        raw.stop_times.push(RawStopTime {
            trip_id: TripId::new("dirty_decreasing_trip"),
            stop_id: Some(StopId::new("stop1")),
            stop_sequence: 1,
            arrival_time: Some(GtfsTime::from_hms(10, 5, 0)),
            departure_time: Some(GtfsTime::from_hms(10, 5, 0)),
            ..Default::default()
        });
        raw.stop_times.push(RawStopTime {
            trip_id: TripId::new("dirty_decreasing_trip"),
            stop_id: Some(StopId::new("stop2")),
            stop_sequence: 2,
            arrival_time: Some(GtfsTime::from_hms(10, 4, 0)),
            departure_time: Some(GtfsTime::from_hms(10, 4, 0)),
            ..Default::default()
        });

        let validated =
            ValidatedFeed::from_raw_with_policy(raw, ValidationPolicy::DropInvalidTripsWithWarning)
                .unwrap();
        let mut builder = RaptorTimetable::builder()
            .with_validation_policy(ValidationPolicy::DropInvalidTripsWithWarning);
        builder.add_feed("test_feed", validated).unwrap();
        let timetable = builder.build();

        assert_eq!(timetable.trips.len(), 3);
        assert_eq!(timetable.patterns.len(), 2);
        let feed_id = FeedId::new("test_feed");
        let t1 = timetable
            .lookup_trip(&feed_id, &TripId::new("trip1"))
            .unwrap();
        let t2 = timetable
            .lookup_trip(&feed_id, &TripId::new("trip2"))
            .unwrap();
        let t3 = timetable
            .lookup_trip(&feed_id, &TripId::new("trip3"))
            .unwrap();
        assert_eq!(
            timetable.trips[t1.index()].pattern_idx,
            timetable.trips[t2.index()].pattern_idx
        );
        assert_ne!(
            timetable.trips[t1.index()].pattern_idx,
            timetable.trips[t3.index()].pattern_idx
        );
        assert_eq!(timetable.feeds[0].dropped_counts.malformed_trips, 1);
    }
}
