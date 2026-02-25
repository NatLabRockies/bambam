use itertools::Itertools;
use routee_compass_core::model::map::MapError;

#[derive(thiserror::Error, Debug)]
pub enum ScheduleError {
    #[error("Failed to parse gtfs bundle file into `Gtfs` struct: {0}")]
    BundleRead(#[from] gtfs_structures::Error), // { source: gtfs_structures::Error },
    #[error("failure running bambam_gtfs: {0}")]
    GtfsApp(String),
    #[error("Failed to match point with spatial index: {source}")]
    SpatialIndexMap {
        #[from]
        source: MapError,
    },
    #[error("Spatial index matched an edge instead of a vertex")]
    SpatialIndexIncorrectMap,
    #[error("Missing lon,lat data and parent_location for stop: {0}")]
    MissingStopLocationAndParent(String),
    #[error("Missing both arrival and departure times: {0}")]
    MissingAllStopTimes(String),
    #[error("At least one of the stops in edge is missing shape distance traveled: {0} or {1}")]
    MissingShapeDistanceTraveled(String, String),
    #[error("Failed to create vertex index: {0}")]
    FailedToCreateVertexIndex(String),
    #[error("Cannot find service in calendar.txt with service_id: {0}")]
    InvalidCalendar(String),
    #[error("Cannot find service in calendar_dates.txt with service_id: {0}")]
    InvalidCalendarDates(String),
    #[error("Invalid Edges and schedules keys")]
    InvalidResultKeys,
    #[error("error due to dataset contents: {0}")]
    InvalidData(String),
    #[error("GTFS archive is malformed: {0}")]
    MalformedGtfs(String),
    #[error("Internal Error: {0}")]
    Internal(String),
    #[error("errors encountered during batch bundle processing: {0}")]
    BatchProcessing(String),
}

pub fn batch_processing_error(errors: &[ScheduleError]) -> ScheduleError {
    let concatenated = errors.iter().map(|e| e.to_string()).join("\n  ");
    ScheduleError::BatchProcessing(format!("[\n  {concatenated}\n]"))
}
