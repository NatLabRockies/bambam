/// file where each line contains a unique zone identifier.
pub const ZONE_IDS_FILENAME: &str = "zone_ids_enumerated.txt.gz";

/// file where each line describes a flex zone-to-zone relation.
pub const RECORDS_FILENAME: &str = "records.csv.gz";

/// file with zone geometries by zone id
pub const GEOMETRIES_FILENAME: &str = "geometries.csv.gz";

/// string used to name this travel mode in the label, constraint and traversal models.
pub const MODE_NAME: &str = "gtfs-flex";
