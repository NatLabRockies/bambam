use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GbfsImportRecord {
    fq_id: String,
    system_id: String,
}
