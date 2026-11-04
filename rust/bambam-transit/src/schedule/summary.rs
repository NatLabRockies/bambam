use std::fmt::Display;

pub struct GtfsSummary {
    pub message: String,
    pub coverage: f64,
    pub trips: usize,
    pub shapes: usize,
    pub legs: usize,
    pub unique_legs: usize,
}

impl Display for GtfsSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{},{},{},{},{},{}",
            self.message, self.coverage, self.trips, self.shapes, self.legs, self.unique_legs
        )
    }
}

impl Default for GtfsSummary {
    fn default() -> Self {
        Self {
            message: String::from("inactive"),
            coverage: Default::default(),
            trips: Default::default(),
            shapes: Default::default(),
            legs: Default::default(),
            unique_legs: Default::default(),
        }
    }
}

impl GtfsSummary {
    pub fn error(msg: String) -> Self {
        Self {
            message: msg,
            coverage: Default::default(),
            trips: Default::default(),
            shapes: Default::default(),
            legs: Default::default(),
            unique_legs: Default::default(),
        }
    }
}
