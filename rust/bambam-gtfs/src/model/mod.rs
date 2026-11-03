//! Phase 2: Relationally validated GTFS + GTFS-Flex domain model (`ValidatedFeed`).

mod agency;
mod calendar;
mod error;
#[cfg(feature = "gtfs-flex")]
pub mod flex;
mod place;
mod route;
mod shape;
mod trip;

pub use agency::Agency;
pub use calendar::{ServiceCalendar, WeeklyPattern};
pub use error::ModelError;
#[cfg(feature = "gtfs-flex")]
pub use flex::{BookingNotice, BookingRule, FlexDurationParams};
#[cfg(feature = "gtfs-flex")]
pub use place::{FlexLocation, LocationGroup};
pub use place::{Pathway, PlaceMember, Stop, StopArea, StopPlace, StopTransfer};
pub use route::Route;
pub use shape::{ShapeLinearRef, ShapePolyline, ShapeVertex};
pub use trip::{BoardingPolicy, FrequencyWindow, StopTime, StopTimeTiming, Trip};

use crate::ids::*;
use crate::primitives::TimeWindow;
use crate::raw::{RawFeed, RawStopTime};
use crate::traits::Spatial;
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::sync::Arc;

/// Phase 2: Relationally validated GTFS feed where all foreign keys are resolved,
/// mutually exclusive fields are collapsed into strict enums (`StopPlace`, `StopTimeTiming`),
/// and `ShapePolyline` linear references are attached to stop times.
#[derive(Clone, Debug, Default)]
pub struct ValidatedFeed {
    /// Validated agencies (`agency.txt`).
    pub agencies: Vec<Agency>,
    /// Validated stops by `StopId` (`stops.txt`).
    pub stops: HashMap<StopId, Arc<Stop>>,
    /// Validated routes by `RouteId` (`routes.txt`).
    pub routes: HashMap<RouteId, Route>,
    /// Unified service calendars by `ServiceId` (`calendar.txt` + `calendar_dates.txt`).
    pub services: HashMap<ServiceId, ServiceCalendar>,
    /// Validated shape polylines by `ShapeId` (`shapes.txt`).
    pub shapes: HashMap<ShapeId, Arc<ShapePolyline>>,
    /// Validated trips by `TripId` (`trips.txt` + `stop_times.txt` + `frequencies.txt`).
    pub trips: HashMap<TripId, Trip>,
    /// Validated stop areas (`stop_areas.txt`).
    pub stop_areas: Vec<StopArea>,

    /// Validated booking rules by `BookingRuleId` (`booking_rules.txt`, GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    pub booking_rules: HashMap<BookingRuleId, Arc<BookingRule>>,
    /// Validated flexible GeoJSON zones by `LocationId` (`locations.geojson`, GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    pub locations: HashMap<LocationId, Arc<FlexLocation>>,
    /// Validated location groups by `LocationGroupId` (`location_groups.txt`, GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    pub location_groups: HashMap<LocationGroupId, Arc<LocationGroup>>,
}

impl ValidatedFeed {
    /// Reconstructs the geographic linestring (`Vec<Coord>`) for `trip_id` between
    /// `from_stop_Idx_in_trip` and `to_stop_idx_in_trip` (inclusive indices into `trip.stop_times`).
    ///
    /// Uses the pre-resolved [`ShapeLinearRef`] on each [`StopTime`] if the trip has a shape,
    /// or falls back to straight-line representative coordinates between the stops.
    pub fn reconstruct_trip_leg_linestring(
        &self,
        trip_id: &TripId,
        from_stop_offset: usize,
        to_stop_offset: usize,
    ) -> Option<Vec<crate::primitives::Coord>> {
        let trip = self.trips.get(trip_id)?;
        if from_stop_offset >= trip.stop_times.len()
            || to_stop_offset >= trip.stop_times.len()
            || from_stop_offset > to_stop_offset
        {
            return None;
        }

        if let Some(shape_id) = &trip.shape_id {
            if let Some(shape) = self.shapes.get(shape_id) {
                let start_ref = trip.stop_times[from_stop_offset].shape_ref;
                let end_ref = trip.stop_times[to_stop_offset].shape_ref;
                if let (Some(s_ref), Some(e_ref)) = (start_ref, end_ref) {
                    return Some(shape.slice_linestring(s_ref.arc_meters, e_ref.arc_meters));
                }
            }
        }

        // Fallback: connect representative coordinates of the stop places directly
        let coords: Vec<_> = trip.stop_times[from_stop_offset..=to_stop_offset]
            .iter()
            .filter_map(|st| st.place.representative_coord())
            .collect();
        Some(coords)
    }
}

impl TryFrom<RawFeed> for ValidatedFeed {
    type Error = ModelError;

    fn try_from(raw: RawFeed) -> Result<Self, Self::Error> {
        // 1. Agencies
        let agencies: Vec<Agency> = raw.agencies.into_iter().map(Agency::from).collect();

        // 2. Stops + Transfers + Pathways
        let mut stop_map: HashMap<StopId, Stop> = raw
            .stops
            .into_iter()
            .map(|s| {
                let stop = Stop::from(s);
                (stop.id.clone(), stop)
            })
            .collect();

        if let Some(transfers) = raw.transfers {
            for tr in transfers {
                if !stop_map.contains_key(&tr.to_stop_id) {
                    return Err(ModelError::MissingReference {
                        source_file: "transfers.txt",
                        field_name: "to_stop_id",
                        id: tr.to_stop_id.to_string(),
                        target_file: "stops.txt",
                    });
                }
                let from_stop = stop_map.get_mut(&tr.from_stop_id).ok_or_else(|| {
                    ModelError::MissingReference {
                        source_file: "transfers.txt",
                        field_name: "from_stop_id",
                        id: tr.from_stop_id.to_string(),
                        target_file: "stops.txt",
                    }
                })?;
                from_stop.transfers.push(StopTransfer::from(tr));
            }
        }

        if let Some(pathways) = raw.pathways {
            for pw in pathways {
                if !stop_map.contains_key(&pw.to_stop_id) {
                    return Err(ModelError::MissingReference {
                        source_file: "pathways.txt",
                        field_name: "to_stop_id",
                        id: pw.to_stop_id.to_string(),
                        target_file: "stops.txt",
                    });
                }
                let from_stop = stop_map.get_mut(&pw.from_stop_id).ok_or_else(|| {
                    ModelError::MissingReference {
                        source_file: "pathways.txt",
                        field_name: "from_stop_id",
                        id: pw.from_stop_id.to_string(),
                        target_file: "stops.txt",
                    }
                })?;
                from_stop.pathways.push(Pathway::from(pw));
            }
        }

        let stops: HashMap<StopId, Arc<Stop>> = stop_map
            .into_iter()
            .map(|(id, s)| (id, Arc::new(s)))
            .collect();

        // 3. GTFS-Flex Locations, LocationGroups, and BookingRules
        #[cfg(feature = "gtfs-flex")]
        let locations: HashMap<LocationId, Arc<FlexLocation>> = {
            let mut map = HashMap::new();
            for raw_loc in raw.locations.unwrap_or_default() {
                // Global check: ID cannot exist in both stops.txt and locations.geojson
                if stops.contains_key(raw_loc.id.as_str()) {
                    return Err(ModelError::NamespaceCollision {
                        id: raw_loc.id.to_string(),
                    });
                }
                let loc = FlexLocation::from(raw_loc);
                map.insert(loc.id.clone(), Arc::new(loc));
            }
            map
        };

        #[cfg(feature = "gtfs-flex")]
        let location_groups: HashMap<LocationGroupId, Arc<LocationGroup>> = build_location_groups(
            raw.location_groups.unwrap_or_default(),
            raw.location_group_stops.unwrap_or_default(),
            &stops,
            &locations,
        )?;

        #[cfg(feature = "gtfs-flex")]
        let booking_rules: HashMap<BookingRuleId, Arc<BookingRule>> = raw
            .booking_rules
            .unwrap_or_default()
            .into_iter()
            .map(|r| {
                let rule = BookingRule::from(r);
                (rule.id.clone(), Arc::new(rule))
            })
            .collect();

        // 4. Unified ServiceCalendars (calendar.txt + calendar_dates.txt)
        let mut services: HashMap<ServiceId, ServiceCalendar> = HashMap::new();
        if let Some(calendars) = raw.calendar {
            for cal in calendars {
                let mut sc = ServiceCalendar::new(cal.id.clone());
                sc.weekly = Some(WeeklyPattern::from(&cal));
                services.insert(cal.id, sc);
            }
        }
        if let Some(cal_dates) = raw.calendar_dates {
            for cd in cal_dates {
                services
                    .entry(cd.service_id.clone())
                    .or_insert_with(|| ServiceCalendar::new(cd.service_id.clone()))
                    .add_exception(&cd);
            }
        }

        // 5. Routes
        let mut routes: HashMap<RouteId, Route> = raw
            .routes
            .into_iter()
            .map(|r| {
                let route = Route::from(r);
                (route.id.clone(), route)
            })
            .collect();

        // 6. Shapes with LRS
        let mut shapes: HashMap<ShapeId, Arc<ShapePolyline>> = HashMap::new();
        if let Some(raw_shapes) = raw.shapes {
            let mut grouped: HashMap<ShapeId, Vec<_>> = HashMap::new();
            for pt in raw_shapes {
                grouped.entry(pt.id.clone()).or_default().push(pt);
            }
            for (shape_id, pts) in grouped {
                let poly = ShapePolyline::from_raw_points(shape_id.clone(), pts)?;
                shapes.insert(shape_id, Arc::new(poly));
            }
        }

        // 7. Trips, StopTimes, and Frequencies
        let mut trips: HashMap<TripId, Trip> = HashMap::with_capacity(raw.trips.len());
        for rt in raw.trips {
            // Ensure route_id is present in routes map (some real-world/test fixtures omit a route row)
            routes.entry(rt.route_id.clone()).or_insert_with(|| Route {
                id: rt.route_id.clone(),
                short_name: None,
                long_name: None,
                desc: None,
                route_type: crate::enums::RouteType::Bus,
                url: None,
                agency_id: None,
                order: None,
                color: rgb::RGB8::new(255, 255, 255),
                text_color: rgb::RGB8::new(0, 0, 0),
                continuous_pickup: crate::enums::ContinuousPickupDropOff::NotAvailable,
                continuous_drop_off: crate::enums::ContinuousPickupDropOff::NotAvailable,
            });
            let shape_id = rt.shape_id.filter(|id| !id.is_empty());
            trips.insert(
                rt.id.clone(),
                Trip {
                    id: rt.id,
                    route_id: rt.route_id,
                    service_id: rt.service_id,
                    shape_id,
                    trip_headsign: rt.trip_headsign,
                    trip_short_name: rt.trip_short_name,
                    direction_id: rt.direction_id,
                    block_id: rt.block_id.filter(|id| !id.is_empty()),
                    wheelchair_accessible: rt.wheelchair_accessible,
                    bikes_allowed: rt.bikes_allowed,
                    stop_times: Vec::new(),
                    frequencies: Vec::new(),
                },
            );
        }

        // Group raw stop_times by trip_id and sort by stop_sequence first so LRS projection is monotonic
        let mut raw_st_by_trip: HashMap<TripId, Vec<RawStopTime>> = HashMap::new();
        for st in raw.stop_times {
            if !trips.contains_key(&st.trip_id) {
                return Err(ModelError::MissingReference {
                    source_file: "stop_times.txt",
                    field_name: "trip_id",
                    id: st.trip_id.to_string(),
                    target_file: "trips.txt",
                });
            }
            raw_st_by_trip
                .entry(st.trip_id.clone())
                .or_default()
                .push(st);
        }

        for (trip_id, mut raw_sts) in raw_st_by_trip {
            raw_sts.sort_by_key(|s| s.stop_sequence);
            let trip = trips.get_mut(&trip_id).expect("trip checked above");
            let trip_shape = trip
                .shape_id
                .as_ref()
                .and_then(|sid| shapes.get(sid))
                .cloned();

            let mut prev_shape_dist: Option<f64> = None;
            let mut prev_arc_meters: f64 = 0.0;

            let mut validated_sts = Vec::with_capacity(raw_sts.len());
            for raw_st in raw_sts {
                if let Some(d) = raw_st.shape_dist_traveled {
                    if let Some(prev_d) = prev_shape_dist {
                        if d < prev_d {
                            return Err(ModelError::NonMonotonicShapeDistance {
                                file_name: "stop_times.txt",
                                id: trip_id.to_string(),
                                previous: prev_d,
                                current: d,
                            });
                        }
                    }
                    prev_shape_dist = Some(d);
                }

                let place = resolve_stop_place(
                    &raw_st,
                    &stops,
                    #[cfg(feature = "gtfs-flex")]
                    &locations,
                    #[cfg(feature = "gtfs-flex")]
                    &location_groups,
                )?;

                let timing = resolve_stop_time_timing(&raw_st)?;

                let pickup = resolve_boarding_policy(
                    raw_st.pickup_type,
                    raw_st.continuous_pickup,
                    #[cfg(feature = "gtfs-flex")]
                    raw_st.pickup_booking_rule_id.as_ref(),
                    #[cfg(feature = "gtfs-flex")]
                    "pickup_booking_rule_id",
                    #[cfg(feature = "gtfs-flex")]
                    &booking_rules,
                )?;

                let drop_off = resolve_boarding_policy(
                    raw_st.drop_off_type,
                    raw_st.continuous_drop_off,
                    #[cfg(feature = "gtfs-flex")]
                    raw_st.drop_off_booking_rule_id.as_ref(),
                    #[cfg(feature = "gtfs-flex")]
                    "drop_off_booking_rule_id",
                    #[cfg(feature = "gtfs-flex")]
                    &booking_rules,
                )?;

                // Compute linear reference onto the trip's shape if available
                let shape_ref = if let Some(poly) = &trip_shape {
                    let resolved = if let Some(raw_dist) = raw_st.shape_dist_traveled {
                        poly.resolve_raw_dist(raw_dist)
                    } else if let Some(coord) = place.representative_coord() {
                        poly.project_coord_monotonic(coord, prev_arc_meters)
                    } else {
                        None
                    };
                    if let Some(r) = resolved {
                        prev_arc_meters = prev_arc_meters.max(r.arc_meters);
                    }
                    resolved
                } else {
                    None
                };

                #[cfg(feature = "gtfs-flex")]
                let flex_duration = FlexDurationParams {
                    mean_duration_factor: raw_st.mean_duration_factor,
                    mean_duration_offset: raw_st.mean_duration_offset,
                    safe_duration_factor: raw_st.safe_duration_factor,
                    safe_duration_offset: raw_st.safe_duration_offset,
                };

                validated_sts.push(StopTime {
                    stop_sequence: raw_st.stop_sequence,
                    place,
                    timing,
                    pickup,
                    drop_off,
                    stop_headsign: raw_st.stop_headsign,
                    timepoint: raw_st.timepoint,
                    shape_ref,
                    #[cfg(feature = "gtfs-flex")]
                    flex_duration,
                });
            }

            trip.stop_times = validated_sts;
        }

        if let Some(raw_freqs) = raw.frequencies {
            for rf in raw_freqs {
                let trip =
                    trips
                        .get_mut(&rf.trip_id)
                        .ok_or_else(|| ModelError::MissingReference {
                            source_file: "frequencies.txt",
                            field_name: "trip_id",
                            id: rf.trip_id.to_string(),
                            target_file: "trips.txt",
                        })?;
                let window = TimeWindow::new(rf.start_time, rf.end_time)?;
                trip.frequencies.push(FrequencyWindow {
                    window,
                    headway_secs: rf.headway_secs,
                    exact_times: rf.exact_times,
                });
            }
        }

        // 8. StopAreas
        let stop_areas = build_stop_areas(
            raw.stop_areas.unwrap_or_default(),
            &stops,
            #[cfg(feature = "gtfs-flex")]
            &locations,
        )?;

        Ok(ValidatedFeed {
            agencies,
            stops,
            routes,
            services,
            shapes,
            trips,
            stop_areas,
            #[cfg(feature = "gtfs-flex")]
            booking_rules,
            #[cfg(feature = "gtfs-flex")]
            locations,
            #[cfg(feature = "gtfs-flex")]
            location_groups,
        })
    }
}

#[cfg(feature = "gtfs-flex")]
fn build_location_groups(
    raw_groups: Vec<crate::raw::RawLocationGroup>,
    raw_group_stops: Vec<crate::raw::RawLocationGroupStop>,
    stops: &HashMap<StopId, Arc<Stop>>,
    locations: &HashMap<LocationId, Arc<FlexLocation>>,
) -> Result<HashMap<LocationGroupId, Arc<LocationGroup>>, ModelError> {
    // Preserve all groups defined in location_groups.txt, even if they have 0 stop rows
    let mut group_names: HashMap<LocationGroupId, Option<String>> = HashMap::new();
    let mut members_by_group: HashMap<LocationGroupId, Vec<PlaceMember>> = HashMap::new();

    for rg in raw_groups {
        group_names.insert(rg.location_group_id.clone(), rg.location_group_name);
        members_by_group.entry(rg.location_group_id).or_default();
    }

    for rgs in raw_group_stops {
        let member = if let Some(stop) = stops.get(rgs.stop_id.as_str()) {
            PlaceMember::Stop(Arc::clone(stop))
        } else if let Some(loc) = locations.get(rgs.stop_id.as_str()) {
            PlaceMember::FlexZone(Arc::clone(loc))
        } else {
            return Err(ModelError::MissingReference {
                source_file: "location_group_stops.txt",
                field_name: "stop_id",
                id: rgs.stop_id,
                target_file: "stops.txt or locations.geojson",
            });
        };

        members_by_group
            .entry(rgs.location_group_id)
            .or_default()
            .push(member);
    }

    let mut result = HashMap::with_capacity(members_by_group.len());
    for (group_id, members) in members_by_group {
        let name = group_names.remove(&group_id).flatten();
        let group = LocationGroup::new(group_id.clone(), name, members);
        result.insert(group_id, Arc::new(group));
    }

    Ok(result)
}

fn build_stop_areas(
    raw_areas: Vec<crate::raw::RawStopArea>,
    stops: &HashMap<StopId, Arc<Stop>>,
    #[cfg(feature = "gtfs-flex")] locations: &HashMap<LocationId, Arc<FlexLocation>>,
) -> Result<Vec<StopArea>, ModelError> {
    let mut area_map: HashMap<AreaId, Vec<PlaceMember>> = HashMap::new();
    let mut seen_order: Vec<AreaId> = Vec::new();
    let mut seen_set: HashSet<AreaId> = HashSet::new();

    for ra in raw_areas {
        let member = if let Some(stop) = stops.get(ra.stop_id.as_str()) {
            Some(PlaceMember::Stop(Arc::clone(stop)))
        } else {
            #[cfg(feature = "gtfs-flex")]
            {
                locations
                    .get(ra.stop_id.as_str())
                    .map(|loc| PlaceMember::FlexZone(Arc::clone(loc)))
            }
            #[cfg(not(feature = "gtfs-flex"))]
            {
                None
            }
        };

        if let Some(member) = member {
            if seen_set.insert(ra.area_id.clone()) {
                seen_order.push(ra.area_id.clone());
            }
            area_map.entry(ra.area_id).or_default().push(member);
        }
    }

    Ok(seen_order
        .into_iter()
        .filter_map(|id| area_map.remove(&id).map(|members| StopArea { id, members }))
        .collect())
}

fn resolve_stop_place(
    raw_st: &RawStopTime,
    stops: &HashMap<StopId, Arc<Stop>>,
    #[cfg(feature = "gtfs-flex")] locations: &HashMap<LocationId, Arc<FlexLocation>>,
    #[cfg(feature = "gtfs-flex")] location_groups: &HashMap<LocationGroupId, Arc<LocationGroup>>,
) -> Result<StopPlace, ModelError> {
    let stop_id = raw_st
        .stop_id
        .as_ref()
        .filter(|id| !id.as_str().trim().is_empty());

    #[cfg(feature = "gtfs-flex")]
    let location_id = raw_st
        .location_id
        .as_ref()
        .filter(|id| !id.as_str().trim().is_empty());
    #[cfg(feature = "gtfs-flex")]
    let location_group_id = raw_st
        .location_group_id
        .as_ref()
        .filter(|id| !id.as_str().trim().is_empty());

    // Enforce mutual exclusivity when gtfs-flex is enabled
    #[cfg(feature = "gtfs-flex")]
    {
        let count = usize::from(stop_id.is_some())
            + usize::from(location_id.is_some())
            + usize::from(location_group_id.is_some());
        if count > 1 {
            return Err(ModelError::MutuallyExclusivePlaceFields {
                trip_id: raw_st.trip_id.to_string(),
                stop_sequence: raw_st.stop_sequence,
                fields: "stop_id, location_id, location_group_id",
            });
        }
    }

    if let Some(sid) = stop_id {
        if let Some(stop) = stops.get(sid) {
            return Ok(StopPlace::Stop(Arc::clone(stop)));
        }
        // GTFS-Flex v1 backwards compatibility: allow stop_id to reference locations.geojson
        #[cfg(feature = "gtfs-flex")]
        if let Some(loc) = locations.get(sid.as_str()) {
            return Ok(StopPlace::FlexZone(Arc::clone(loc)));
        }

        return Err(ModelError::MissingReference {
            source_file: "stop_times.txt",
            field_name: "stop_id",
            id: sid.to_string(),
            target_file: if cfg!(feature = "gtfs-flex") {
                "stops.txt or locations.geojson"
            } else {
                "stops.txt"
            },
        });
    }

    #[cfg(feature = "gtfs-flex")]
    if let Some(lid) = location_id {
        let loc = locations
            .get(lid)
            .ok_or_else(|| ModelError::MissingReference {
                source_file: "stop_times.txt",
                field_name: "location_id",
                id: lid.to_string(),
                target_file: "locations.geojson",
            })?;
        return Ok(StopPlace::FlexZone(Arc::clone(loc)));
    }

    #[cfg(feature = "gtfs-flex")]
    if let Some(lgid) = location_group_id {
        let group = location_groups
            .get(lgid)
            .ok_or_else(|| ModelError::MissingReference {
                source_file: "stop_times.txt",
                field_name: "location_group_id",
                id: lgid.to_string(),
                target_file: "location_groups.txt",
            })?;
        return Ok(StopPlace::LocationGroup(Arc::clone(group)));
    }

    Err(ModelError::MissingStopTimePlace {
        trip_id: raw_st.trip_id.to_string(),
        stop_sequence: raw_st.stop_sequence,
    })
}

fn resolve_stop_time_timing(raw_st: &RawStopTime) -> Result<StopTimeTiming, ModelError> {
    #[cfg(feature = "gtfs-flex")]
    {
        let start_win = raw_st.start_pickup_drop_off_window;
        let end_win = raw_st.end_pickup_drop_off_window;
        if start_win.is_some() || end_win.is_some() {
            if raw_st.arrival_time.is_some() || raw_st.departure_time.is_some() {
                return Err(ModelError::InvalidStopTimeTiming {
                    trip_id: raw_st.trip_id.to_string(),
                    stop_sequence: raw_st.stop_sequence,
                    reason: "cannot specify both arrival/departure_time and start/end_pickup_drop_off_window".to_owned(),
                });
            }
            let (Some(start), Some(end)) = (start_win, end_win) else {
                return Err(ModelError::InvalidStopTimeTiming {
                    trip_id: raw_st.trip_id.to_string(),
                    stop_sequence: raw_st.stop_sequence,
                    reason: "both start_pickup_drop_off_window and end_pickup_drop_off_window must be provided together".to_owned(),
                });
            };
            let window = TimeWindow::new(start, end)?;
            return Ok(StopTimeTiming::FlexWindow(window));
        }
    }

    match (raw_st.arrival_time, raw_st.departure_time) {
        (Some(arr), Some(dep)) => {
            if arr > dep {
                return Err(ModelError::InvalidStopTimeTiming {
                    trip_id: raw_st.trip_id.to_string(),
                    stop_sequence: raw_st.stop_sequence,
                    reason: format!("arrival_time ({arr}) is after departure_time ({dep})"),
                });
            }
            Ok(StopTimeTiming::Scheduled {
                arrival: arr,
                departure: dep,
            })
        }
        (arrival, departure) => Ok(StopTimeTiming::Uninterpolated { arrival, departure }),
    }
}

fn resolve_boarding_policy(
    method: crate::enums::PickupDropOffType,
    continuous: crate::enums::ContinuousPickupDropOff,
    #[cfg(feature = "gtfs-flex")] rule_id: Option<&BookingRuleId>,
    #[cfg(feature = "gtfs-flex")] field_name: &'static str,
    #[cfg(feature = "gtfs-flex")] booking_rules: &HashMap<BookingRuleId, Arc<BookingRule>>,
) -> Result<BoardingPolicy, ModelError> {
    #[cfg(feature = "gtfs-flex")]
    let booking_rule = match rule_id.filter(|id| !id.is_empty()) {
        Some(id) => {
            let rule = booking_rules
                .get(id)
                .ok_or_else(|| ModelError::MissingReference {
                    source_file: "stop_times.txt",
                    field_name,
                    id: id.to_string(),
                    target_file: "booking_rules.txt",
                })?;
            Some(Arc::clone(rule))
        }
        None => None,
    };

    Ok(BoardingPolicy {
        method,
        continuous,
        #[cfg(feature = "gtfs-flex")]
        booking_rule,
    })
}
