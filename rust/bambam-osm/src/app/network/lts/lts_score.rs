pub const MIN_LTS_SCORE: u8 = 1; // the best LTS score.
pub const MAX_LTS_SCORE: u8 = 4; // the worst LTS score.

#[derive(Default, Eq, PartialEq, PartialOrd, Debug)]
pub struct LtsScore(u8);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LtsError {
    #[error("LtsScore value '{0}' must be in the integer range: [1..4]")]
    ValueError(u8),
}

impl std::fmt::Display for LtsScore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl LtsScore {
    pub fn new(value: u8) -> Result<Self, LtsError> {
        if value < MIN_LTS_SCORE || value > MAX_LTS_SCORE {
            Err(LtsError::ValueError(value))
        } else {
            Ok(LtsScore(value))
        }
    }
}
