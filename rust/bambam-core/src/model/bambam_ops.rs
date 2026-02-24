use crate::model::bambam_state;

use super::{bambam_field, TimeBin};
use geo::{line_measures::LengthMeasurable, Haversine, InterpolatableLine, LineString, Point};
use routee_compass::{app::search::SearchAppResult, plugin::PluginError};
use routee_compass_core::{
    algorithm::search::SearchTreeNode,
    model::{
        label::Label,
        state::{StateModel, StateModelError, StateVariable},
        unit::DistanceUnit,
    },
};
use std::collections::HashMap;
use uom::{
    si::f64::{Length, Time},
    ConstZero,
};
use wkt::ToWkt;

pub type DestinationsIter<'a> =
    Box<dyn Iterator<Item = Result<(Label, &'a SearchTreeNode), StateModelError>> + 'a>;

/// collects search tree branches that can be reached _as destinations_
/// within the given time bin.
pub fn collect_destinations<'a>(
    search_result: &'a SearchAppResult,
    time_bin: Option<&'a TimeBin>,
    state_model: &'a StateModel,
) -> DestinationsIter<'a> {
    let tree = match search_result.trees.first() {
        None => return Box::new(std::iter::empty()),
        Some(t) => t,
    };

    let tree_destinations = tree
        .iter()
        .filter_map(move |(label, branch)| apply_predicate(label, branch, time_bin, state_model));

    Box::new(tree_destinations)
}

/// apply the destinations predicate to this label/branch combination. designed
/// to be run from within a FilterMap call. returns
/// - None if the destination should be ignored
/// - Some(Ok(_)) if the destination is valid
/// - Some(Err(_)) if we encountered an error
pub fn apply_predicate<'a>(
    label: &Label,
    branch: &'a SearchTreeNode,
    time_bin: Option<&'a TimeBin>,
    state_model: &'a StateModel,
) -> Option<Result<(Label, &'a SearchTreeNode), StateModelError>> {
    match branch.incoming_edge() {
        None => None,
        Some(et) => {
            let result_state = &et.result_state;
            let within_bin = match &time_bin {
                Some(bin) => bin.state_time_within_bin(result_state, state_model),
                None => Ok(true),
            };
            match within_bin {
                Ok(true) => Some(Ok((label.clone(), branch))),
                Ok(false) => None,
                Err(e) => Some(Err(e)),
            }
        }
    }
}

pub fn points_along_linestring(
    linestring: &LineString<f32>,
    stride: &Length,
    _distance_unit: &DistanceUnit,
) -> Result<Vec<Point<f32>>, String> {
    let length: Length =
        Length::new::<uom::si::length::meter>(linestring.length(&Haversine) as f64);

    if &length < stride {
        match (linestring.points().next(), linestring.points().next_back()) {
            (Some(first), Some(last)) => Ok(vec![first, last]),
            _ => Err(format!(
                "invalid linestring, should have at least two points: {linestring:?}"
            )),
        }
    } else {
        // determine number of steps
        let n_strides = (length / *stride).value.ceil() as u64;
        let n_points = n_strides + 1;

        let length_meters = length.value;

        (0..=n_points)
            .map(|point_index| {
                let distance_to_point = stride.value * point_index as f64;
                let fraction = (distance_to_point / length_meters) as f32;
                let point = linestring
                    .point_at_ratio_from_start(&Haversine, fraction)
                    .ok_or_else(|| {
                        format!(
                            "unable to interpolate {}m/{}% into linestring with distance {}: {}",
                            distance_to_point,
                            (fraction * 10000.0).trunc() / 100.0,
                            length_meters,
                            linestring.to_wkt()
                        )
                    })?;
                Ok(point)
            })
            .collect::<Result<Vec<_>, String>>()
    }
}

pub fn accumulate_global_opps(
    opps: &[(usize, Vec<f64>)],
    colnames: &[String],
) -> Result<HashMap<String, f64>, PluginError> {
    let mut result: HashMap<String, f64> = HashMap::new();
    for (_, row) in opps.iter() {
        for (idx, value) in row.iter().enumerate() {
            let colname = colnames.get(idx).ok_or_else(|| {
                PluginError::InternalError(
                    "opportunity count row and activity types list do not match".to_string(),
                )
            })?;
            if let Some(val) = result.get_mut(colname) {
                *val += value;
            } else {
                result.insert(colname.to_string(), *value);
            }
        }
    }
    Ok(result)
}

/// helper that combines the arrival delay with the traversal time to produce
/// the time to reach this point and call it a destination.
pub fn get_reachability_time(
    state: &[StateVariable],
    state_model: &StateModel,
) -> Result<Time, StateModelError> {
    let trip_time = state_model.get_time(state, bambam_state::TRIP_TIME)?;
    let has_delay = state_model.contains_key(&bambam_state::TRIP_ARRIVAL_DELAY.to_string());
    let arrival_delay = if has_delay {
        state_model.get_time(state, bambam_state::TRIP_ARRIVAL_DELAY)?
    } else {
        Time::ZERO
    };
    Ok(trip_time + arrival_delay)
}

/// steps through each bin's output section for mutable updates
pub fn iterate_bins<'a>(
    output: &'a mut serde_json::Value,
) -> Result<Box<dyn Iterator<Item = (&'a String, &'a mut serde_json::Value)> + 'a>, PluginError> {
    let bins = output.get_mut(bambam_field::TIME_BINS).ok_or_else(|| {
        PluginError::UnexpectedQueryStructure(format!(
            "after running json structure plugin, cannot find key {}",
            bambam_field::TIME_BINS
        ))
    })?;
    let bins_map = bins.as_object_mut().ok_or_else(|| {
        PluginError::UnexpectedQueryStructure(format!(
            "after running json structure plugin, field {} was not a key/value map",
            bambam_field::TIME_BINS
        ))
    })?;
    Ok(Box::new(bins_map.iter_mut()))
}
