use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Format for serializing GTFS edge schedules during import.
#[derive(Serialize, Deserialize, Debug, Default, ValueEnum, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleOutputFormat {
    /// Write compressed CSV (`edges-schedules.csv.gz`)
    Csv,
    /// Write pre-compiled CSR `EdgeTimetable` binary snapshot (`edges-schedules.rkyv`)
    #[default]
    Rkyv,
}
