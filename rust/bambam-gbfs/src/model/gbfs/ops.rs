/// builds a globally-unique identifier for a zone. based on the fact that
/// system_ids are defined as globally unique. as documented at
/// <https://github.com/MobilityData/gbfs/blob/master/gbfs.md#system_informationjson>:
///
/// > [system_id] is a globally unique identifier for the vehicle share system. Each distinct system
/// > or geographic area in which vehicles are operated MUST have its own unique system_id. It
/// > is up to the publisher of the feed to guarantee uniqueness and MUST be checked against
/// > existing system_id fields in systems.csv to ensure this. This value is intended to remain
/// > the same over the life of the system.
/// >
/// > System IDs SHOULD be recognizable as belonging to a particular system as opposed to random
/// > strings - for example, bcycle_austin or biketown_pdx.
pub fn fully_qualified_zone_id(system_id: &str, zone_feature_index: usize) -> String {
    format!("{system_id}#{zone_feature_index}")
}
