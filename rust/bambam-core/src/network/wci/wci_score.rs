pub const MIN_WCI_SCORE: i32 = -6;
pub const MAX_WCI_SCORE: i32 = 9;

/// A Walking Comfort Index (WCI) score, constrained to the integer range
/// `[MIN_WCI_SCORE, MAX_WCI_SCORE]`.
#[derive(Default, Eq, PartialEq, PartialOrd, Debug, Clone)]
pub struct WciScore(i32);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WciError {
    #[error("WciScore value '{0}' must be in the integer range: [-6..9]")]
    ValueError(i32),
}

// borrowed + borrowed -> owned
impl<'a> std::ops::Add<&'a WciScore> for &'a WciScore {
    type Output = WciScore;

    fn add(self, rhs: Self) -> Self::Output {
        let sum = self.0 + rhs.0;
        WciScore::new(sum).unwrap_or_else(|_| WciScore(sum.clamp(MIN_WCI_SCORE, MAX_WCI_SCORE)))
    }
}

// owned + borrowed -> owned
impl std::ops::Add<&WciScore> for WciScore {
    type Output = Self;

    fn add(self, rhs: &WciScore) -> Self::Output {
        let sum = self.0 + rhs.0;
        WciScore::new(sum).unwrap_or_else(|_| WciScore(sum.clamp(MIN_WCI_SCORE, MAX_WCI_SCORE)))
    }
}

// owned + owned -> owned
impl std::ops::Add<WciScore> for WciScore {
    type Output = Self;

    fn add(self, rhs: WciScore) -> Self::Output {
        let sum = self.0 + rhs.0;
        WciScore::new(sum).unwrap_or_else(|_| WciScore(sum.clamp(MIN_WCI_SCORE, MAX_WCI_SCORE)))
    }
}

impl std::fmt::Display for WciScore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl WciScore {
    pub fn new(value: i32) -> Result<WciScore, WciError> {
        if (MIN_WCI_SCORE..=MAX_WCI_SCORE).contains(&value) {
            Ok(WciScore(value))
        } else {
            Err(WciError::ValueError(value))
        }
    }

    /// Construct a `WciScore` from a raw component value without range
    /// validation, clamping into the valid range. Intended for internal
    /// component-score construction where values are known to be small.
    pub(crate) fn from_component(value: i32) -> WciScore {
        WciScore(value.clamp(MIN_WCI_SCORE, MAX_WCI_SCORE))
    }
}
