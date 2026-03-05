//! Typed section wrappers for working with bambam `serde_json::Value` output rows.
//!
//! Rather than fully deserializing an output row into [`crate::model::BambamResult`] and
//! re-serializing it at every output-plugin boundary, each section wrapper holds a
//! `&serde_json::Value` or `&mut serde_json::Value` pointing at a specific subtree and exposes typed getters
//! and setters that only parse the fields they actually touch.
//!
//! The [`BambamOutputRow`] struct is the single entry point.  Call one of its section
//! accessors to obtain a scoped wrapper, do your reads/writes, drop the wrapper, and
//! move on to the next section.
//!
//! # plugin usage pattern
//!
//! ```ignore
//! fn run_plugin(output: &mut serde_json::Value) -> Result<serde_json::Value, OutputPluginError> {
//!     let mut row = BambamOutputRow::new(output);
//!
//!     // read the mode from the request section (immutable)
//!     let mode = row.request()?.get_mode()?;
//!
//!     // write to the info section
//!     {
//!         let mut info = row.info_mut()?;
//!         info.set_opportunity_format(OpportunityFormat::Aggregate)?;
//!         info.set_tree_size(42)?;
//!     }
//!
//!     // write binned aggregate data
//!     {
//!         let mut agg = row.aggregate()?;
//!         agg.set_opportunities("10", &counts)?;
//!     }
//!
//!     Ok(output.clone()) // or take ownership depending on plugin signature
//! }
//! ```

use std::collections::HashMap;

use routee_compass::plugin::output::OutputPluginError;
use routee_compass_core::model::cost::TraversalCost;
use serde_json::{json, Value};

use crate::model::{
    bambam_field,
    destination::{BinRangeConfig, DestinationPredicate},
    output_plugin::{
        isochrone::{GeometryModelConfig, IsochroneAlgorithm, IsochroneOutputFormat},
        opportunity::{OpportunityFormat, OpportunityOrientation},
    },
};

// ─── top-level entry point ────────────────────────────────────────────────────

/// The primary typed handle over a bambam output row `Value`.
///
/// Obtain section wrappers from this struct.  Each wrapper mutably borrows the
/// relevant subtree for its lifetime, so they cannot overlap — obtain, use, and
/// drop a section before requesting the next one.
pub struct BambamOutputRow<'a>(pub &'a mut Value);

impl<'a> BambamOutputRow<'a> {
    pub fn new(value: &'a mut Value) -> Self {
        Self(value)
    }

    /// Returns a read-only wrapper over the `request` subtree.
    pub fn request(&self) -> Result<RequestSection<'_>, OutputPluginError> {
        let section = self.0.get("request").ok_or_else(|| {
            OutputPluginError::InternalError(
                "expected 'request' section in bambam output row".to_string(),
            )
        })?;
        Ok(RequestSection(section))
    }

    /// Returns a read-only wrapper over the `info` subtree.
    pub fn info_ref(&self) -> Result<InfoSectionRef<'_>, OutputPluginError> {
        let section = self.0.get(bambam_field::INFO).ok_or_else(|| {
            OutputPluginError::InternalError(
                "expected 'info' section in bambam output row".to_string(),
            )
        })?;
        Ok(InfoSectionRef(section))
    }

    /// Opens (or creates) the `info` subtree and returns a typed wrapper.
    pub fn info_mut(&mut self) -> Result<InfoSectionMut<'_>, OutputPluginError> {
        let section = ensure_object(self.0, bambam_field::INFO)?;
        Ok(InfoSectionMut(section))
    }

    /// Opens (or creates) the `aggregate_opportunities` subtree.
    pub fn aggregate(&mut self) -> Result<AggregateSection<'_>, OutputPluginError> {
        let section = ensure_object(self.0, "aggregate_opportunities")?;
        Ok(AggregateSection(section))
    }

    /// Opens (or creates) the `disaggregate_opportunities` subtree.
    pub fn disaggregate(&mut self) -> Result<DisaggregateSection<'_>, OutputPluginError> {
        let section = ensure_object(self.0, "disaggregate_opportunities")?;
        Ok(DisaggregateSection(section))
    }

    /// Returns the root-level `opportunity_totals` map.
    pub fn get_opportunity_totals(
        &self,
    ) -> Result<Option<HashMap<String, f64>>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::OPPORTUNITY_TOTALS)
    }

    /// Writes the root-level `opportunity_totals` map.
    pub fn set_opportunity_totals(
        &mut self,
        totals: &HashMap<String, f64>,
    ) -> Result<(), OutputPluginError> {
        set_field(self.0, bambam_field::OPPORTUNITY_TOTALS, totals)
    }
}

// ─── request section ──────────────────────────────────────────────────────────

/// Read-only typed view over the `request` subtree.
pub struct RequestSection<'a>(&'a Value);

impl<'a> RequestSection<'a> {
    /// Returns the transport `mode` string (e.g. `"car"`, `"transit"`).
    pub fn get_mode(&self) -> Result<String, OutputPluginError> {
        get_field(self.0, bambam_field::MODE)
    }
}

// ─── info section ─────────────────────────────────────────────────────────────

/// Typed read-only view over the `info` subtree.
pub struct InfoSectionRef<'a>(&'a Value);

impl<'a> InfoSectionRef<'a> {
    pub fn get_tree_size(&self) -> Result<usize, OutputPluginError> {
        get_field(self.0, bambam_field::N_DESTINATIONS)
    }

    pub fn get_opportunity_runtime(&self) -> Result<String, OutputPluginError> {
        get_field(self.0, bambam_field::OPPORTUNITY_PLUGIN_RUNTIME)
    }

    pub fn get_activity_types(&self) -> Result<Option<Vec<String>>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::ACTIVITY_TYPES)
    }

    pub fn get_bin_range(&self) -> Result<Option<BinRangeConfig>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::BIN_RANGE)
    }

    pub fn get_destination_filter(
        &self,
    ) -> Result<Option<Vec<DestinationPredicate>>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::DESTINATION_FILTER)
    }

    pub fn get_geometry_model(&self) -> Result<Option<GeometryModelConfig>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::GEOMETRY_MODEL)
    }

    pub fn get_isochrone_algorithm(&self) -> Result<Option<IsochroneAlgorithm>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::ISOCHRONE_ALGORITHM)
    }

    pub fn get_isochrone_format(&self) -> Result<Option<IsochroneOutputFormat>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::ISOCHRONE_FORMAT)
    }

    pub fn get_opportunity_format(&self) -> Result<Option<OpportunityFormat>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::OPPORTUNITY_FORMAT)
    }

    pub fn get_opportunity_orientation(
        &self,
    ) -> Result<Option<OpportunityOrientation>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::OPPORTUNITY_ORIENTATION)
    }
}

/// Typed read/write view over the `info` subtree.
pub struct InfoSectionMut<'a>(&'a mut Value);

impl<'a> InfoSectionMut<'a> {
    pub fn get_tree_size(&self) -> Result<usize, OutputPluginError> {
        get_field(self.0, bambam_field::N_DESTINATIONS)
    }
    pub fn set_tree_size(&mut self, v: usize) -> Result<(), OutputPluginError> {
        set_field(self.0, bambam_field::N_DESTINATIONS, v)
    }

    pub fn get_opportunity_runtime(&self) -> Result<String, OutputPluginError> {
        get_field(self.0, bambam_field::OPPORTUNITY_PLUGIN_RUNTIME)
    }
    pub fn set_opportunity_runtime(&mut self, v: String) -> Result<(), OutputPluginError> {
        set_field(self.0, bambam_field::OPPORTUNITY_PLUGIN_RUNTIME, v)
    }

    pub fn get_activity_types(&self) -> Result<Option<Vec<String>>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::ACTIVITY_TYPES)
    }
    pub fn set_activity_types(&mut self, v: &[String]) -> Result<(), OutputPluginError> {
        set_field(self.0, bambam_field::ACTIVITY_TYPES, v)
    }

    pub fn get_bin_range(&self) -> Result<Option<BinRangeConfig>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::BIN_RANGE)
    }
    pub fn set_bin_range(&mut self, v: &BinRangeConfig) -> Result<(), OutputPluginError> {
        set_field(self.0, bambam_field::BIN_RANGE, v)
    }

    pub fn get_destination_filter(
        &self,
    ) -> Result<Option<Vec<DestinationPredicate>>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::DESTINATION_FILTER)
    }
    pub fn set_destination_filter(
        &mut self,
        v: &[DestinationPredicate],
    ) -> Result<(), OutputPluginError> {
        set_field(self.0, bambam_field::DESTINATION_FILTER, v)
    }

    pub fn get_geometry_model(&self) -> Result<Option<GeometryModelConfig>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::GEOMETRY_MODEL)
    }
    pub fn set_geometry_model(&mut self, v: &GeometryModelConfig) -> Result<(), OutputPluginError> {
        set_field(self.0, bambam_field::GEOMETRY_MODEL, v)
    }

    pub fn get_isochrone_algorithm(&self) -> Result<Option<IsochroneAlgorithm>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::ISOCHRONE_ALGORITHM)
    }
    pub fn set_isochrone_algorithm(
        &mut self,
        v: &IsochroneAlgorithm,
    ) -> Result<(), OutputPluginError> {
        set_field(self.0, bambam_field::ISOCHRONE_ALGORITHM, v)
    }

    pub fn get_isochrone_format(&self) -> Result<Option<IsochroneOutputFormat>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::ISOCHRONE_FORMAT)
    }
    pub fn set_isochrone_format(
        &mut self,
        v: &IsochroneOutputFormat,
    ) -> Result<(), OutputPluginError> {
        set_field(self.0, bambam_field::ISOCHRONE_FORMAT, v)
    }

    pub fn get_opportunity_format(&self) -> Result<Option<OpportunityFormat>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::OPPORTUNITY_FORMAT)
    }
    pub fn set_opportunity_format(
        &mut self,
        v: OpportunityFormat,
    ) -> Result<(), OutputPluginError> {
        set_field(self.0, bambam_field::OPPORTUNITY_FORMAT, v)
    }

    pub fn get_opportunity_orientation(
        &self,
    ) -> Result<Option<OpportunityOrientation>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::OPPORTUNITY_ORIENTATION)
    }
    pub fn set_opportunity_orientation(
        &mut self,
        v: OpportunityOrientation,
    ) -> Result<(), OutputPluginError> {
        set_field(self.0, bambam_field::OPPORTUNITY_ORIENTATION, v)
    }
}

// ─── aggregate section ────────────────────────────────────────────────────────

/// Typed read/write view over the `aggregate_opportunities` subtree.
///
/// Entries are keyed by time-bin string (e.g. `"10"`, `"20"`).  Each bin holds
/// an optional isochrone blob and an optional opportunity-count map.
pub struct AggregateSection<'a>(&'a mut Value);

impl<'a> AggregateSection<'a> {
    /// Returns the set of bin keys present in the section.
    pub fn bin_keys(&self) -> Vec<&str> {
        self.0
            .as_object()
            .map(|m| m.keys().map(String::as_str).collect())
            .unwrap_or_default()
    }

    /// Returns the raw isochrone `Value` for `bin_key`, if present.
    /// The blob is intentionally left as `Value` to avoid re-parsing geometry.
    pub fn get_isochrone(&self, bin_key: &str) -> Option<&Value> {
        self.0
            .get(bin_key)
            .and_then(|b| b.get(bambam_field::ISOCHRONE))
    }

    /// Writes a raw isochrone blob for `bin_key`.
    pub fn set_isochrone(&mut self, bin_key: &str, v: Value) -> Result<(), OutputPluginError> {
        self.ensure_bin(bin_key)?;
        self.0[bin_key][bambam_field::ISOCHRONE] = v;
        Ok(())
    }

    /// Returns the raw isochrone `Value` for `bin_key`, if present.
    /// The blob is intentionally left as `Value` to avoid re-parsing geometry.
    pub fn get_n_destinations(&self, bin_key: &str) -> Result<Option<usize>, OutputPluginError> {
        let value = self
            .0
            .get(bin_key)
            .and_then(|b| b.get(bambam_field::N_DESTINATIONS));
        let n = match value {
            Some(v) => v.as_u64(),
            None => return Ok(None),
        };
        match n {
            Some(number) => Ok(Some(number as usize)),
            None => {
                let msg = format!(
                    "value stored at aggregate.{bin_key}.{} is not an unsigned integer",
                    bambam_field::N_DESTINATIONS
                );
                Err(OutputPluginError::OutputPluginFailed(msg))
            }
        }
    }

    /// Writes a raw n_destinations blob for `bin_key`.
    pub fn set_n_destinations(&mut self, bin_key: &str, v: usize) -> Result<(), OutputPluginError> {
        self.ensure_bin(bin_key)?;
        self.0[bin_key][bambam_field::N_DESTINATIONS] = json![v];
        Ok(())
    }
    /// Returns typed opportunity counts for `bin_key`.
    pub fn get_opportunities(
        &self,
        bin_key: &str,
    ) -> Result<Option<HashMap<String, f64>>, OutputPluginError> {
        let value = self
            .0
            .get(bin_key)
            .and_then(|b| b.get(bambam_field::OPPORTUNITIES));
        match value {
            None => Ok(None),
            Some(v) => serde_json::from_value(v.clone())
                .map(Some)
                .map_err(|e| OutputPluginError::OutputPluginFailed(e.to_string())),
        }
    }

    /// Writes typed opportunity counts for `bin_key`.
    pub fn set_opportunities(
        &mut self,
        bin_key: &str,
        v: &HashMap<String, f64>,
    ) -> Result<(), OutputPluginError> {
        self.ensure_bin(bin_key)?;
        let serialized = serde_json::to_value(v).map_err(|e| {
            OutputPluginError::OutputPluginFailed(format!("cannot serialize opportunities: {e}"))
        })?;
        self.0[bin_key][bambam_field::OPPORTUNITIES] = serialized;
        Ok(())
    }

    /// Returns the bin-level runtime string for `bin_key`, if present.
    pub fn get_bin_runtime(&self, bin_key: &str) -> Result<Option<String>, OutputPluginError> {
        let value = self
            .0
            .get(bin_key)
            .and_then(|b| b.get(bambam_field::OPPORTUNITY_BIN_RUNTIME));
        match value {
            None => Ok(None),
            Some(v) => serde_json::from_value(v.clone())
                .map(Some)
                .map_err(|e| OutputPluginError::OutputPluginFailed(e.to_string())),
        }
    }

    /// Writes a bin-level runtime string for `bin_key`.
    pub fn set_bin_runtime(&mut self, bin_key: &str, v: String) -> Result<(), OutputPluginError> {
        self.ensure_bin(bin_key)?;
        let serialized = serde_json::to_value(v).map_err(|e| {
            OutputPluginError::OutputPluginFailed(format!("cannot serialize bin runtime: {e}"))
        })?;
        self.0[bin_key][bambam_field::OPPORTUNITY_BIN_RUNTIME] = serialized;
        Ok(())
    }

    fn ensure_bin(&mut self, bin_key: &str) -> Result<(), OutputPluginError> {
        if !self.0.is_object() {
            return Err(OutputPluginError::OutputPluginFailed(
                "aggregate_opportunities section is corrupted: not a JSON object".to_string(),
            ));
        }
        if self.0.get(bin_key).is_none() {
            self.0[bin_key] = json!({});
        }
        Ok(())
    }
}

// ─── disaggregate section ─────────────────────────────────────────────────────

/// Typed read/write view over the `disaggregate_opportunities` subtree.
pub struct DisaggregateSection<'a>(&'a mut Value);

impl<'a> DisaggregateSection<'a> {
    pub fn get_opportunities(
        &self,
    ) -> Result<Option<DisaggregateOpportunityCounts>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::OPPORTUNITIES)
    }

    pub fn set_opportunities(
        &mut self,
        v: &DisaggregateOpportunityCounts,
    ) -> Result<(), OutputPluginError> {
        set_field(self.0, bambam_field::OPPORTUNITIES, v)
    }

    /// Returns per-vertex state blobs.  Each value is left as raw `Value`
    /// to avoid re-parsing state variable arrays when they are not needed.
    pub fn get_state(&self) -> Result<Option<Value>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::SEARCH_STATE)
    }

    pub fn set_state(&mut self, v: &Value) -> Result<(), OutputPluginError> {
        set_field(self.0, bambam_field::SEARCH_STATE, v)
    }

    pub fn get_cost(&self) -> Result<TraversalCost, OutputPluginError> {
        get_field(self.0, bambam_field::COST)
    }

    pub fn set_cost(&mut self, v: TraversalCost) -> Result<(), OutputPluginError> {
        set_field(self.0, bambam_field::COST, v)
    }
}

/// Alias matching [`crate::model::bambam_json::OpportunityCounts`].
pub type OpportunityCounts = HashMap<String, f64>;
pub type DisaggregateOpportunityCounts = HashMap<String, OpportunityCounts>;

// ─── private helpers ─────────────────────────────────────────────────────────

fn get_field<T: serde::de::DeserializeOwned>(
    section: &Value,
    field: &str,
) -> Result<T, OutputPluginError> {
    let v = section.get(field).ok_or_else(|| {
        OutputPluginError::InternalError(format!("field '{field}' not found in section"))
    })?;
    serde_json::from_value(v.clone()).map_err(|e| {
        OutputPluginError::OutputPluginFailed(format!("cannot deserialize field '{field}': {e}"))
    })
}

fn get_field_opt<T: serde::de::DeserializeOwned>(
    section: &Value,
    field: &str,
) -> Result<Option<T>, OutputPluginError> {
    match section.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => serde_json::from_value(v.clone()).map(Some).map_err(|e| {
            OutputPluginError::OutputPluginFailed(format!(
                "cannot deserialize field '{field}': {e}"
            ))
        }),
    }
}

fn set_field<T: serde::Serialize>(
    section: &mut Value,
    field: &str,
    value: T,
) -> Result<(), OutputPluginError> {
    let v = serde_json::to_value(value).map_err(|e| {
        OutputPluginError::OutputPluginFailed(format!("cannot serialize field '{field}': {e}"))
    })?;
    section[field] = v;
    Ok(())
}

/// Ensures `key` exists as a JSON object inside `root`, creating it if absent.
fn ensure_object<'a>(root: &'a mut Value, key: &str) -> Result<&'a mut Value, OutputPluginError> {
    if !root.is_object() {
        return Err(OutputPluginError::OutputPluginFailed(format!(
            "output is corrupted: section not a JSON object, cannot hold {key}"
        )));
    }
    if root.get(key).is_none() {
        root[key] = json!({});
    }
    root.get_mut(key)
        .ok_or_else(|| OutputPluginError::InternalError(format!("could not open section '{key}'")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use routee_compass_core::model::unit::Cost;
    use serde_json::json;
    use std::collections::HashMap;

    /// Round-trip: write fields to info, read them back.
    #[test]
    fn info_section_round_trip() {
        let mut value = json!({"request": {"mode": "car"}});
        let mut row = BambamOutputRow::new(&mut value);

        // info section should be created on first access
        {
            let mut info = row.info_mut().unwrap();
            info.set_tree_size(42).unwrap();
            info.set_opportunity_runtime("00:00:01".to_string())
                .unwrap();
            info.set_activity_types(&["retail".to_string(), "food".to_string()])
                .unwrap();
        }

        // read back with immutable accessor
        let info = row.info_ref().unwrap();
        assert_eq!(info.get_tree_size().unwrap(), 42);
        assert_eq!(info.get_opportunity_runtime().unwrap(), "00:00:01");
        assert_eq!(
            info.get_activity_types().unwrap(),
            Some(vec!["retail".to_string(), "food".to_string()])
        );
    }

    /// Optional fields return None when absent.
    #[test]
    fn info_section_optional_fields_return_none() {
        let mut value = json!({"info": {}});
        let row = BambamOutputRow::new(&mut value);
        let info = row.info_ref().unwrap();
        assert!(info.get_activity_types().unwrap().is_none());
        assert!(info.get_bin_range().unwrap().is_none());
        assert!(info.get_destination_filter().unwrap().is_none());
        assert!(info.get_opportunity_format().unwrap().is_none());
        assert!(info.get_opportunity_orientation().unwrap().is_none());
        assert!(info.get_geometry_model().unwrap().is_none());
        assert!(info.get_isochrone_algorithm().unwrap().is_none());
        assert!(info.get_isochrone_format().unwrap().is_none());
    }

    /// Request section: read mode field.
    #[test]
    fn request_section_get_mode() {
        let mut value = json!({"request": {"mode": "transit"}});
        let row = BambamOutputRow::new(&mut value);
        assert_eq!(row.request().unwrap().get_mode().unwrap(), "transit");
    }

    /// Request section: missing request key returns error.
    #[test]
    fn request_section_missing_returns_error() {
        let mut value = json!({});
        let row = BambamOutputRow::new(&mut value);
        assert!(row.request().is_err());
    }

    /// Aggregate section: write and read opportunities per bin.
    #[test]
    fn aggregate_section_opportunities_round_trip() {
        let mut value = json!({});
        let mut row = BambamOutputRow::new(&mut value);

        let mut counts = HashMap::new();
        counts.insert("retail".to_string(), 100.0);
        counts.insert("food".to_string(), 50.5);

        {
            let mut agg = row.aggregate().unwrap();
            agg.set_opportunities("10", &counts)
                .expect("failed to set opportunities");
            agg.set_n_destinations("10", 7)
                .expect("failed to set n destinations");
            agg.set_bin_runtime("10", "00:00:00.5".to_string()).unwrap();
        }

        {
            let agg = row.aggregate().unwrap();
            assert_eq!(agg.bin_keys(), vec!["10"]);
            let read_back = agg.get_opportunities("10").unwrap().unwrap();
            assert_eq!(read_back["retail"], 100.0);
            assert_eq!(read_back["food"], 50.5);
            assert_eq!(agg.get_n_destinations("10").unwrap(), Some(7));
            assert_eq!(
                agg.get_bin_runtime("10").unwrap(),
                Some("00:00:00.5".to_string())
            );
        }
    }

    /// Aggregate section: multiple bins are independent.
    #[test]
    fn aggregate_section_multiple_bins() {
        let mut value = json!({});
        let mut row = BambamOutputRow::new(&mut value);

        let mut c1 = HashMap::new();
        c1.insert("retail".to_string(), 10.0);
        let mut c2 = HashMap::new();
        c2.insert("retail".to_string(), 20.0);

        {
            let mut agg = row.aggregate().unwrap();
            agg.set_opportunities("10", &c1).unwrap();
            agg.set_opportunities("20", &c2).unwrap();
        }

        {
            let agg = row.aggregate().unwrap();
            let mut keys = agg.bin_keys();
            keys.sort();
            assert_eq!(keys, vec!["10", "20"]);
            assert_eq!(
                agg.get_opportunities("10").unwrap().unwrap()["retail"],
                10.0
            );
            assert_eq!(
                agg.get_opportunities("20").unwrap().unwrap()["retail"],
                20.0
            );
        }
    }

    /// Aggregate section: reading a nonexistent bin returns None.
    #[test]
    fn aggregate_section_missing_bin_returns_none() {
        let mut value = json!({});
        let mut row = BambamOutputRow::new(&mut value);
        let agg = row.aggregate().unwrap();
        assert!(agg.get_opportunities("999").unwrap().is_none());
        assert!(agg.get_isochrone("999").is_none());
        assert!(agg.get_n_destinations("999").unwrap().is_none());
    }

    /// Isochrone round-trip through aggregate section.
    #[test]
    fn aggregate_section_isochrone_round_trip() {
        let mut value = json!({});
        let mut row = BambamOutputRow::new(&mut value);
        let geojson = json!({"type": "Polygon", "coordinates": [[[0,0],[1,0],[1,1],[0,0]]]});

        {
            let mut agg = row.aggregate().unwrap();
            agg.set_isochrone("10", geojson.clone())
                .expect("failed to set isochrone");
        }

        {
            let agg = row.aggregate().unwrap();
            assert_eq!(agg.get_isochrone("10").unwrap(), &geojson);
        }
    }

    /// Opportunity totals round-trip at top level.
    #[test]
    fn opportunity_totals_round_trip() {
        let mut value = json!({});
        let mut row = BambamOutputRow::new(&mut value);

        let mut totals = HashMap::new();
        totals.insert("retail".to_string(), 5000.0);
        row.set_opportunity_totals(&totals).unwrap();

        let read = row.get_opportunity_totals().unwrap().unwrap();
        assert_eq!(read["retail"], 5000.0);
    }

    /// Opportunity totals return None when absent.
    #[test]
    fn opportunity_totals_absent() {
        let mut value = json!({});
        let row = BambamOutputRow::new(&mut value);
        assert!(row.get_opportunity_totals().unwrap().is_none());
    }

    /// Disaggregate section: set and get cost field.
    #[test]
    fn disaggregate_section_cost_round_trip() {
        use routee_compass_core::model::cost::TraversalCost;
        let mut value = json!({});
        let mut row = BambamOutputRow::new(&mut value);
        let cost = TraversalCost {
            objective_cost: Cost::new(42.0),
            total_cost: Cost::new(42.0),
        };
        {
            let mut dis = row.disaggregate().unwrap();
            dis.set_cost(cost.clone()).unwrap();
        }

        {
            let dis = row.disaggregate().unwrap();
            let result = dis.get_cost().unwrap();
            assert_eq!(&result.objective_cost, &cost.objective_cost);
            assert_eq!(&result.total_cost, &cost.total_cost);
        }
    }
}
