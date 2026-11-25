use serde::{Deserialize, Serialize};

use crate::wci::wci::Wci;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct WciParams {
    /// value (inclusive) for accepting an edge for walk traversal. edges with WCI values
    /// below this number are rejected.
    pub wci_threshold: Wci,
}
