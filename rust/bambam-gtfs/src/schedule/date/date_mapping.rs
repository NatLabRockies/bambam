use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::schedule::fq_ops;

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct DateMapping {
    pub feed_id: Option<String>,
    pub agency_id: Option<String>,
    pub route_id: String,
    pub service_id: String,
    pub target_date: NaiveDate,
    pub picked_date: NaiveDate,
}

impl DateMapping {
    pub fn get_fully_qualified_id(&self, edge_list_id: usize) -> String {
        fq_ops::get_fully_qualified_route_id(
            self.feed_id.as_deref(),
            self.agency_id.as_deref(),
            &self.route_id,
            &self.service_id,
            edge_list_id,
        )
    }
}
