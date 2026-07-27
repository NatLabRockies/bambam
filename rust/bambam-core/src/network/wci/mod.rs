mod compute_wci;
mod ops;
mod wci_score;

pub use compute_wci::{compute_wci, WciComponentScores};
pub use ops::NO_CYCLEWAY_FOUND_SCORE;
pub use wci_score::{WciError, WciScore, MAX_WCI_SCORE, MIN_WCI_SCORE};
