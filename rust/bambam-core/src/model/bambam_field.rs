//! Fields and types assigned to the JSON output during bambam runs.
//!
//! # Examples
//!
//! ### Aggregate Data Rows
//!
//! ```json
//! {
//!   "opportunity_format": "aggregate",
//!   "opportunity_totals": {},
//!   "activity_types": [],
//!   "info": {
//!     "opportunity_runtime": "hh:mm:ss",
//!     "mep_runtime": "hh:mm:ss",
//!     "tree_size": 0,
//!   }
//!   "bin": {
//!     10: {
//!       "isochrone": {},
//!       "opportunities" {},
//!       "mep": {},
//!       "info": {
//!         "time_bin": { .. },
//!         "bin_runtime":
//!       },
//!     }
//!   }
//! }
//! ```
//!
//! ### Disaggregate Data Rows
//! ```json
//! {
//!   "opportunity_format": "disaggregate",
//!   "opportunity_totals": {},
//!   "activity_types": [],
//!   "opportunities": {
//!     "{EdgeListId}-{EdgeId}": {
//!       "counts": {},
//!       "state": []
//!     }
//!   }
//! }
//! ```
//!
use crate::model::TimeBin;
use itertools::Itertools;
use routee_compass::plugin::output::OutputPluginError;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

pub const TIME_BINS: &str = "bin";
pub const TIME_BIN: &str = "time_bin";
pub const INFO: &str = "info";
pub const MODE: &str = "mode";
pub const ISOCHRONE: &str = "isochrone";
pub const ISOCHRONE_FORMAT: &str = "isochrone_format";
pub const TREE_SIZE: &str = "tree_size";
pub const ACTIVITY_TYPES: &str = "activity_types";
pub const OPPORTUNITIES: &str = "opportunities";
pub const OPPORTUNITY_COUNTS: &str = "opportunity_counts";
pub const OPPORTUNITY_ORIENTATION: &str = "opportunity_orientation";
pub const OPPORTUNITY_FORMAT: &str = "opportunity_format";
pub const OPPORTUNITY_TOTALS: &str = "opportunity_totals";
pub const VEHICLE_STATE: &str = "vehicle_state";
pub const OPP_FMT_AGGREGATE: &str = "aggregate";
pub const OPP_FMT_DISAGGREGATE: &str = "disaggregate";
pub const OPPORTUNITY_PLUGIN_RUNTIME: &str = "opportunity_runtime";
pub const OPPORTUNITY_BIN_RUNTIME: &str = "bin_runtime";

pub mod get {
    use itertools::Itertools;
    use routee_compass::plugin::output::OutputPluginError;
    use routee_compass_core::model::{
        network::{EdgeId, EdgeListId, VertexId},
        state::StateVariable,
    };
    use serde::de::DeserializeOwned;
    use serde_json::Value;
    use std::collections::HashMap;

    use crate::model::{
        bambam_field::as_usize,
        output_plugin::{
            isochrone::IsochroneOutputFormat,
            opportunity::{OpportunityFormat, OpportunityOrientation},
        },
    };

    pub fn mode(value: &Value) -> Result<String, OutputPluginError> {
        let path = ["request", super::MODE];
        super::get_nested(value, &path).map_err(|e| {
            let dot_path = path.join(".");
            OutputPluginError::OutputPluginFailed(format!(
                "failure retrieving 'mode' value from '{dot_path}': {e}"
            ))
        })
    }
    pub fn activity_types(value: &Value) -> Result<Vec<String>, OutputPluginError> {
        get_from_value(super::ACTIVITY_TYPES, value)
    }
    pub fn isochrone_format(value: &Value) -> Result<IsochroneOutputFormat, OutputPluginError> {
        get_from_value(super::ISOCHRONE_FORMAT, value)
    }
    pub fn opportunity_format(value: &Value) -> Result<OpportunityFormat, OutputPluginError> {
        get_from_value(super::OPPORTUNITY_FORMAT, value)
    }
    pub fn opportunity_orientation(
        value: &Value,
    ) -> Result<OpportunityOrientation, OutputPluginError> {
        get_from_value(super::OPPORTUNITY_ORIENTATION, value)
    }
    pub fn disaggregate_vertex_id(value: &str) -> Result<VertexId, OutputPluginError> {
        let id: usize = super::as_usize(value)?;
        Ok(VertexId(id))
    }
    pub fn disaggregate_edge_id(value: &str) -> Result<(EdgeListId, EdgeId), OutputPluginError> {
        match value.split("-").collect_vec()[..] {
            [] => Err(OutputPluginError::OutputPluginFailed("disaggregate edge identifier is empty".to_string())),
            [edge_list_str, edge_str] => {
                let edge_list_id = EdgeListId(as_usize(edge_list_str)?);
                let edge_id = EdgeId(as_usize(edge_str)?);
                Ok((edge_list_id, edge_id))
            },
            _ => Err(OutputPluginError::OutputPluginFailed(format!("disaggregate edge identifier is malformed, expected '<EdgeListId>-<EdgeId>', found '{value}'")))
        }
    }
    pub fn totals(value: &Value) -> Result<HashMap<String, f64>, OutputPluginError> {
        get_from_value(super::OPPORTUNITY_TOTALS, value)
    }
    pub fn counts(value: &Value) -> Result<HashMap<String, f64>, OutputPluginError> {
        get_from_value(super::OPPORTUNITY_COUNTS, value)
    }
    pub fn state(value: &Value) -> Result<Vec<StateVariable>, OutputPluginError> {
        get_from_value(super::VEHICLE_STATE, value)
    }

    /// helper for deserializing fields from a JSON value in a deserializable type
    fn get_from_value<T>(field: &str, value: &Value) -> Result<T, OutputPluginError>
    where
        T: DeserializeOwned,
    {
        let value = value.get(field).ok_or_else(|| {
            OutputPluginError::InternalError(format!("cannot find '{field}' in output row"))
        })?;
        serde_json::from_value(value.clone()).map_err(|e| {
            OutputPluginError::OutputPluginFailed(format!(
                "found '{field}' in output row but cannot deserialize due to: {e}"
            ))
        })
    }
}

mod set {}

/// gets a deserialized value from a json object at some path. not compatible with json arrays.
pub fn get_nested<T: DeserializeOwned>(json: &Value, path: &[&str]) -> Result<T, String> {
    let mut cursor = json;
    for k in path {
        match cursor.get(k) {
            Some(child) => {
                cursor = child;
            }
            None => return Err(nested_error("get", path.to_vec(), k, cursor)),
        }
    }
    let result = serde_json::from_value(cursor.clone())
        .map_err(|e| format!("unable to deserialize value '{cursor}': {e}"))?;
    Ok(result)
}

/// inserts a json value into a json object at some path, adding any missing parent objects
/// along the way. not compatible with json arrays.
pub fn insert_nested_with_parents(
    json: &mut Value,
    path: &[&str],
    key: &str,
    value: Value,
    overwrite: bool,
) -> Result<(), String> {
    let parents = path.to_vec();
    for i in 0..parents.len() {
        let key = parents[i];
        insert_nested(json, &parents[0..i], key, json![{}], false)?;
    }
    insert_nested(json, path, key, value, overwrite)
}

/// inserts a json value into a json object at some path. not compatible with json arrays.
pub fn insert_nested(
    json: &mut Value,
    path: &[&str],
    key: &str,
    value: Value,
    overwrite: bool,
) -> Result<(), String> {
    let mut cursor = json;
    for k in path {
        if cursor.get(k).is_none() {
            return Err(nested_error("insert", path.to_vec(), k, cursor));
        };
        match cursor.get_mut(k) {
            Some(child) => {
                cursor = child;
            }
            None => unreachable!("invariant: already None-checked above"),
        }
    }
    let exists = cursor.get(key).is_some();
    if exists && !overwrite {
        Ok(())
    } else {
        cursor[key] = value;
        Ok(())
    }
}

/// assures that the structure exists for a time bin.
///
///
/// with time bin [0, 10]:
///
/// {
///   "bin": {
///     "10": {
///       "info": { "time_bin": { .. } },
///     }
///   }
/// }
pub fn scaffold_time_bin(json: &mut Value, time_bin: &TimeBin) -> Result<(), String> {
    if json.get(TIME_BINS).is_none() {
        json[TIME_BINS] = json![{}];
    }
    let time_bin_key = time_bin.key();
    insert_nested(json, &[TIME_BINS], &time_bin_key, json![{}], false)?;
    insert_nested(
        json,
        &[TIME_BINS, &time_bin_key],
        INFO,
        json![{ TIME_BIN: json![time_bin] }],
        false,
    )?;
    Ok(())
}

type TimeBinsIter<'a> = Box<dyn Iterator<Item = Result<(TimeBin, &'a Value), String>> + 'a>;
type TimeBinsIterMut<'a> = Box<dyn Iterator<Item = Result<(TimeBin, &'a mut Value), String>> + 'a>;

pub fn get_time_bins(output: &serde_json::Value) -> Result<Vec<TimeBin>, String> {
    let bins_value = output
        .get(TIME_BINS)
        .ok_or_else(|| field_error(vec![TIME_BINS]))?;
    let bins = bins_value
        .as_object()
        .ok_or_else(|| type_error(vec![TIME_BINS], String::from("JSON object")))?
        .values()
        .map(|v| get_nested(v, &[INFO, TIME_BIN]))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(bins)
}

pub fn time_bins_iter(output: &serde_json::Value) -> Result<TimeBinsIter<'_>, String> {
    let bins_value = output
        .get(TIME_BINS)
        .ok_or_else(|| field_error(vec![TIME_BINS]))?;
    let bins = bins_value
        .as_object()
        .ok_or_else(|| type_error(vec![TIME_BINS], String::from("JSON object")))?
        .values()
        .map(|v| {
            let time_bin = get_nested(v, &[INFO, TIME_BIN]);
            time_bin.map(|t| (t, v))
        });
    Ok(Box::new(bins))
}

pub fn time_bins_iter_mut(output: &mut serde_json::Value) -> Result<TimeBinsIterMut<'_>, String> {
    let bins_value = output
        .get_mut(TIME_BINS)
        .ok_or_else(|| field_error(vec![TIME_BINS]))?;
    let bins = bins_value
        .as_object_mut()
        .ok_or_else(|| type_error(vec![TIME_BINS], String::from("JSON object")))?
        .values_mut()
        .map(move |v| {
            let time_bin = get_nested(v, &[INFO, TIME_BIN]);
            time_bin.map(|t| (t, v))
        });
    Ok(Box::new(bins))
}

fn field_error(fields: Vec<&str>) -> String {
    let path = fields.join(".");
    format!("expected path {path} missing from output object")
}

fn nested_error(action: &str, fields: Vec<&str>, failed_key: &str, object: &Value) -> String {
    let path = fields.join(".");
    let keylist = object
        .as_object()
        .map(|o| o.keys().collect_vec())
        .unwrap_or_default();
    let keys = if keylist.len() > 5 {
        let inner = keylist.iter().take(5).join(", ");
        format!("[{inner}, ...]")
    } else {
        let inner = keylist.iter().join(", ");
        format!("[{inner}]")
    };
    format!(
        "during {action}, expected path '{path}' missing key '{failed_key}' from JSON object available sibling keys: {keys}"
    )
}

fn type_error(fields: Vec<&str>, expected_type: String) -> String {
    let path = fields.join(".");
    format!("expected value at path {path} to be {expected_type}")
}

fn as_usize(value: &str) -> Result<usize, OutputPluginError> {
    value.parse().map_err(|e| {
        OutputPluginError::OutputPluginFailed(format!(
            "unable to read oppportunity key '{value}' as a numeric value: {e}"
        ))
    })
}
