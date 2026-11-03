//! Phase 1 unvalidated `RawFeed` and configurable `RawFeedReader`.

use crate::raw::archive::GtfsArchive;
use crate::raw::error::RawReadError;
use crate::raw::schedule::*;
use crate::raw::table::TableBundle;
use std::io::{Read, Seek};
use std::path::Path;

#[cfg(feature = "gtfs-flex")]
use crate::raw::flex::*;

/// Configuration options for loading a full [`RawFeed`] via [`RawFeedReader`].
#[derive(Clone, Debug)]
pub struct RawFeedReader {
    /// Whether to read `stop_times.txt` (default `true`).
    pub read_stop_times: bool,
    /// Whether to read `shapes.txt` (default `true`).
    pub read_shapes: bool,
    /// Whether to trim leading/trailing whitespace from CSV fields (default `true`).
    pub trim_fields: bool,
}

impl Default for RawFeedReader {
    fn default() -> Self {
        Self {
            read_stop_times: true,
            read_shapes: true,
            trim_fields: true,
        }
    }
}

/// Phase 1: Unvalidated, 1:1 deserialized GTFS tables read from a [`GtfsArchive`].
#[derive(Clone, Debug, Default)]
pub struct RawFeed {
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

    /// Rows from `booking_rules.txt` (GTFS-Flex, optional).
    #[cfg(feature = "gtfs-flex")]
    pub booking_rules: Option<Vec<RawBookingRule>>,
    /// Features from `locations.geojson` (GTFS-Flex, optional).
    #[cfg(feature = "gtfs-flex")]
    pub locations: Option<Vec<RawFlexLocation>>,
    /// Rows from `location_groups.txt` (GTFS-Flex, optional).
    #[cfg(feature = "gtfs-flex")]
    pub location_groups: Option<Vec<RawLocationGroup>>,
    /// Rows from `location_group_stops.txt` (GTFS-Flex, optional).
    #[cfg(feature = "gtfs-flex")]
    pub location_group_stops: Option<Vec<RawLocationGroupStop>>,
}

impl TableBundle for RawFeed {
    fn from_archive<R: Read + Seek>(archive: &mut GtfsArchive<R>) -> Result<Self, RawReadError> {
        let schedule: ScheduleTables = archive.load()?;
        #[cfg(feature = "gtfs-flex")]
        let flex: FlexTables = archive.load()?;

        Ok(Self {
            agencies: schedule.agencies,
            stops: schedule.stops,
            routes: schedule.routes,
            trips: schedule.trips,
            stop_times: schedule.stop_times,
            calendar: schedule.calendar,
            calendar_dates: schedule.calendar_dates,
            shapes: schedule.shapes,
            frequencies: schedule.frequencies,
            transfers: schedule.transfers,
            pathways: schedule.pathways,
            levels: schedule.levels,
            feed_info: schedule.feed_info,
            stop_areas: schedule.stop_areas,
            #[cfg(feature = "gtfs-flex")]
            booking_rules: flex.booking_rules,
            #[cfg(feature = "gtfs-flex")]
            locations: flex.locations,
            #[cfg(feature = "gtfs-flex")]
            location_groups: flex.location_groups,
            #[cfg(feature = "gtfs-flex")]
            location_group_stops: flex.location_group_stops,
        })
    }
}

impl RawFeed {
    /// Reads a [`RawFeed`] from a local directory or `.zip` file path using [`GtfsArchive`].
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, RawReadError> {
        GtfsArchive::open(path)?.load()
    }

    /// Reads a [`RawFeed`] from any seekable reader containing a `.zip` archive.
    pub fn from_reader<R: Read + Seek>(reader: R) -> Result<Self, RawReadError> {
        GtfsArchive::from_reader(reader)?.load()
    }
}

impl RawFeedReader {
    /// Configures whether `stop_times.txt` should be read.
    pub fn read_stop_times(mut self, read: bool) -> Self {
        self.read_stop_times = read;
        self
    }

    /// Configures whether `shapes.txt` should be read.
    pub fn read_shapes(mut self, read: bool) -> Self {
        self.read_shapes = read;
        self
    }

    /// Reads a [`RawFeed`] from a local directory or `.zip` archive path.
    pub fn read_from_path(&self, path: impl AsRef<Path>) -> Result<RawFeed, RawReadError> {
        let mut archive = GtfsArchive::open(path)?.with_trim_fields(self.trim_fields);
        self.read_from_archive(&mut archive)
    }

    /// Reads a [`RawFeed`] from a seekable `.zip` archive reader.
    pub fn read_from_reader<R: Read + Seek>(&self, reader: R) -> Result<RawFeed, RawReadError> {
        let mut archive = GtfsArchive::from_reader(reader)?.with_trim_fields(self.trim_fields);
        self.read_from_archive(&mut archive)
    }

    /// Reads a [`RawFeed`] from an open [`GtfsArchive`], respecting `read_stop_times` and `read_shapes`.
    pub fn read_from_archive<R: Read + Seek>(
        &self,
        archive: &mut GtfsArchive<R>,
    ) -> Result<RawFeed, RawReadError> {
        let agencies = archive.load()?;
        let stops = archive.load()?;
        let routes = archive.load()?;
        let trips = archive.load()?;
        let stop_times = if self.read_stop_times {
            archive.load()?
        } else {
            Vec::new()
        };
        let calendar = archive.load()?;
        let calendar_dates = archive.load()?;
        let shapes = if self.read_shapes {
            archive.load()?
        } else {
            None
        };
        let frequencies = archive.load()?;
        let transfers = archive.load()?;
        let pathways = archive.load()?;
        let levels = archive.load()?;
        let feed_info = archive.load()?;
        let stop_areas = archive.load()?;

        #[cfg(feature = "gtfs-flex")]
        let flex: FlexTables = archive.load()?;

        Ok(RawFeed {
            agencies,
            stops,
            routes,
            trips,
            stop_times,
            calendar,
            calendar_dates,
            shapes,
            frequencies,
            transfers,
            pathways,
            levels,
            feed_info,
            stop_areas,
            #[cfg(feature = "gtfs-flex")]
            booking_rules: flex.booking_rules,
            #[cfg(feature = "gtfs-flex")]
            locations: flex.locations,
            #[cfg(feature = "gtfs-flex")]
            location_groups: flex.location_groups,
            #[cfg(feature = "gtfs-flex")]
            location_group_stops: flex.location_group_stops,
        })
    }
}
