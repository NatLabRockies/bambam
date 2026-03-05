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

use chrono::Duration;
use routee_compass::plugin::output::OutputPluginError;
use routee_compass_core::model::{cost::TraversalCost, network::VertexId};
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
    ) -> Result<Option<HashMap<String, usize>>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::OPPORTUNITY_TOTALS)
    }

    /// Writes the root-level `opportunity_totals` map.
    pub fn set_opportunity_totals(
        &mut self,
        totals: &HashMap<String, usize>,
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
        get_field(self.0, bambam_field::TREE_SIZE)
    }

    pub fn get_opportunity_runtime(&self) -> Result<Duration, OutputPluginError> {
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
        get_field(self.0, bambam_field::TREE_SIZE)
    }
    pub fn set_tree_size(&mut self, v: usize) -> Result<(), OutputPluginError> {
        set_field(self.0, bambam_field::TREE_SIZE, v)
    }

    pub fn get_opportunity_runtime(&self) -> Result<Duration, OutputPluginError> {
        get_field(self.0, bambam_field::OPPORTUNITY_PLUGIN_RUNTIME)
    }
    pub fn set_opportunity_runtime(&mut self, v: &Duration) -> Result<(), OutputPluginError> {
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
    pub fn set_isochrone(&mut self, bin_key: &str, v: Value) {
        self.ensure_bin(bin_key);
        self.0[bin_key][bambam_field::ISOCHRONE] = v;
    }

    /// Returns typed opportunity counts for `bin_key`.
    pub fn get_opportunities(
        &self,
        bin_key: &str,
    ) -> Result<Option<HashMap<String, f64>>, OutputPluginError> {
        match self
            .0
            .get(bin_key)
            .and_then(|b| b.get(bambam_field::OPPORTUNITIES))
        {
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
        self.ensure_bin(bin_key);
        let serialized = serde_json::to_value(v).map_err(|e| {
            OutputPluginError::OutputPluginFailed(format!("cannot serialize opportunities: {e}"))
        })?;
        self.0[bin_key][bambam_field::OPPORTUNITIES] = serialized;
        Ok(())
    }

    fn ensure_bin(&mut self, bin_key: &str) {
        if self.0.get(bin_key).is_none() {
            self.0[bin_key] = json!({});
        }
    }
}

// ─── disaggregate section ─────────────────────────────────────────────────────

/// Typed read/write view over the `disaggregate_opportunities` subtree.
pub struct DisaggregateSection<'a>(&'a mut Value);

impl<'a> DisaggregateSection<'a> {
    pub fn get_opportunities(
        &self,
    ) -> Result<Option<HashMap<VertexId, OpportunityCounts>>, OutputPluginError> {
        get_field_opt(self.0, bambam_field::OPPORTUNITIES)
    }

    pub fn set_opportunities(
        &mut self,
        v: &HashMap<VertexId, OpportunityCounts>,
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
    if root.get(key).is_none() {
        root[key] = json!({});
    }
    root.get_mut(key)
        .ok_or_else(|| OutputPluginError::InternalError(format!("could not open section '{key}'")))
}
