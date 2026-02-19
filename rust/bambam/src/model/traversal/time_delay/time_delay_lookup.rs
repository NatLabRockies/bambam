use super::{
    delay_aggregation_type::DelayAggregationType, time_delay_record::TimeDelayRecord,
    TimeDelayConfig,
};
use bambam_core::util::geo_utils;
use geo::{Geometry, Point};
use kdam::Bar;
use routee_compass_core::{
    config::{CompassConfigurationError, ConfigJsonExtensions},
    model::{network::Vertex, traversal::TraversalModelError, unit::TimeUnit},
    util::fs::read_utils,
};
use rstar::{RTree, AABB};
use std::path::Path;
use uom::si::f64::Time;

pub struct TimeDelayLookup {
    pub lookup: RTree<TimeDelayRecord>,
    pub config: TimeDelayConfig,
}

impl TimeDelayLookup {
    /// helper function for finding delays from graph vertices. in the case of multiple overlapping
    /// delay polygons, the first is selected.
    pub fn get_delay_for_vertex<'a>(&self, lookup_vertex: &Vertex) -> Option<Time> {
        let g = geo::Geometry::Point(geo::Point(lookup_vertex.coordinate.0));
        self.find_first_delay(&g)
    }

    /// gets a delay value from this lookup function and returns it in the base time unit.
    /// when delays are not expected to overlap, this function only takes the first overlapping
    /// row and returns that value.
    ///
    /// # Arguments
    ///
    /// * `geometry` - geometry to find intersecting time access records
    ///
    /// # Returns
    ///
    /// * Zero or one time access delay. If addditional records intersect the incoming geometry,
    ///   only the first is returned.
    pub fn find_first_delay(&self, geometry: &Geometry<f32>) -> Option<Time> {
        let envelope_option: Option<AABB<Point<f32>>> =
            geo_utils::get_centroid_as_envelope(geometry);
        let result = envelope_option.and_then(|envelope| {
            self.lookup
                .locate_in_envelope_intersecting(&envelope)
                .next()
                .map(|t| t.time)
        });
        result
    }

    /// gets a delay value from this lookup function and returns it in the base time unit.
    /// when many delays may overlap with this geometry, this function will take all intersecting
    /// rows and aggregate them together into a single delay value.
    ///
    /// # Arguments
    ///
    /// * `geometry` - geometry to find intersecting time access records
    ///
    /// # Returns
    ///
    /// * Zero or one time access delay. If addditional records intersect the incoming geometry,
    ///   only the first is returned.
    pub fn find_all_delays(&self, geometry: &Geometry<f32>) -> Option<Time> {
        let envelope_option: Option<AABB<Point<f32>>> =
            geo_utils::get_centroid_as_envelope(geometry);
        let time = envelope_option.and_then(|envelope| {
            let values = self
                .lookup
                .locate_in_envelope_intersecting(&envelope)
                .map(|record| record.time)
                .collect();
            self.config.aggregation.apply(values)
        });
        time
    }
}

impl TryFrom<TimeDelayConfig> for TimeDelayLookup {
    type Error = TraversalModelError;

    /// builds a new lookup function for zonal time delays at either trip departure or arrival
    ///
    /// # Arguments
    ///
    /// * `value` - JSON value for this lookup instance
    ///
    /// # Returns
    ///
    /// * an object that can be used to lookup time delay values, or an error
    fn try_from(config: TimeDelayConfig) -> Result<Self, Self::Error> {
        let data: Box<[TimeDelayRecord]> = read_utils::from_csv(
            &config.lookup_file,
            true,
            Some(Bar::builder().desc("time delay lookup")),
            None,
        )
        .map_err(|e| {
            TraversalModelError::BuildError(format!(
                "failure reading time delay rows from {}: {}",
                config.lookup_file, e
            ))
        })?;
        let lookup = RTree::bulk_load(data.to_vec());
        Ok(TimeDelayLookup { lookup, config })
    }
}
