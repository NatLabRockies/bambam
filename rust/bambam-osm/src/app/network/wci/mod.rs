pub mod compute_wci;
mod ops;
pub mod wci_score;
pub use compute_wci::compute_wci;
pub use wci_score::WciScore;
const NO_CYCLEWAY_FOUND_SCORE: i32 = -2; // If there is no cycleway found for a way, cycle component of WCI.
