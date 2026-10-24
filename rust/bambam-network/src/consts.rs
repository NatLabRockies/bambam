/// A distance tolerance (in meters) used to mitigate floating-point drift when extracting
/// LineString geometries for segment splits.
pub const F32_DISTANCE_TOLERANCE: f32 = 1e-6;

/// same intent as [F32_DISTANCE_TOLERANCE] for f64 values.
pub const F64_DISTANCE_TOLERANCE: f64 = 1e-6;

pub const COMPASS_VERTEX_FILENAME: &str = "vertices-compass.csv.gz";
pub const COMPLETE_VERTEX_FILENAME: &str = "vertices-complete.csv.gz";
pub const VERTEX_MAPPING_FILENAME: &str = "vertices-mapping.csv.gz";
pub const VERTEX_TRAFFIC_SIGNALS_FILENAME: &str = "vertices-traffic-signals-enumerated.txt.gz";
pub const VERTEX_STOP_FILENAME: &str = "vertices-stop-enumerated.txt.gz";
pub const VERTEX_GIVE_WAY_FILENAME: &str = "vertices-give-way-enumerated.txt.gz";
pub const VERTEX_CROSSING_FILENAME: &str = "vertices-crossing-enumerated.txt.gz";
pub const COMPASS_EDGES_FILENAME: &str = "edges-compass.csv.gz";
pub const EDGE_MAPPING_FILENAME: &str = "edges-mapping.csv.gz";
pub const GEOMETRIES_FILENAME: &str = "edges-geometries-enumerated.txt.gz";
pub const SPEEDS_FILENAME: &str = "edges-speeds-mph-enumerated.txt.gz";
pub const CLASSES_FILENAME: &str = "edges-classes-enumerated.txt.gz";
pub const SPEED_MAPPING_FILENAME: &str = "edges-classes-speed-mapping.csv.gz";
pub const OMF_SEGMENT_IDS_FILENAME: &str = "edges-omf-segment-ids.csv.gz";
pub const OMF_CONNECTOR_IDS_FILENAME: &str = "vertices-omf-connector-ids.txt.gz";
pub const BEARINGS_FILENAME: &str = "edges-bearings-enumerated.txt.gz";
pub const GLOBAL_AVG_SPEED_KEY: &str = "_global_";
