use std::collections::HashMap;

use chrono::Duration;
use routee_compass_core::model::{cost::TraversalCost, network::VertexId};
use serde::{Deserialize, Serialize};

use crate::model::{
    destination::{BinRangeConfig, DestinationPredicateConfig},
    output_plugin::{
        isochrone::IsochroneOutputFormat,
        opportunity::{OpportunityFormat, OpportunityOrientation},
    },
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BambamResult {
    request: BambamRequest,
    opportunity_totals: Option<HashMap<String, usize>>,
    info: Option<BambamResultInfo>,
    aggregate_opportunities: Option<HashMap<String, BambamAggregateOpportunities>>,
    disaggregate_opportunities: Option<BambamDisaggregateOpportunities>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BambamRequest {
    mode: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BambamResultInfo {
    opportunity_runtime: Duration,
    tree_size: usize,
    activity_types: Option<Vec<String>>,
    bin_range: Option<BinRangeConfig>,
    destination_filter: Option<Vec<DestinationPredicateConfig>>,
    isochrone_format: Option<IsochroneOutputFormat>,
    opportunity_format: Option<OpportunityFormat>,
    opportunity_orientation: Option<OpportunityOrientation>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BambamAggregateOpportunities {
    isochrone: Option<serde_json::Value>,
    opportunities: Option<HashMap<String, f64>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BambamDisaggregateOpportunities {
    opportunities: Option<HashMap<VertexId, OpportunityCounts>>,
    state: Option<HashMap<VertexId, serde_json::Value>>,
    cost: TraversalCost,
}

pub type OpportunityCounts = HashMap<String, f64>;
