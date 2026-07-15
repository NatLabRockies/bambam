/// The CyclewayTag is a type that characterizes the
/// level of safety of a cycleway.
///
/// You can generate a new cycleway tag by passing in
/// the OSM way's cycleway attribute.
#[derive(Debug)]
pub enum CyclewayTag {
    DedicatedNoBuffer,
    NoDedicatedWithFacilities,
    NoDedicatedNoFacilities,
}

impl CyclewayTag {
    pub fn new(tag: &str) -> Self {
        if Self::is_dedicated_no_buffer(tag) {
            CyclewayTag::DedicatedNoBuffer
        } else if Self::is_no_dedicated_with_facilities(tag) {
            CyclewayTag::NoDedicatedWithFacilities
        } else {
            CyclewayTag::NoDedicatedNoFacilities
        }
    }
    //TODO: ADD DedicatedWithBuffer variant for LTS computations.
    /// DedicatedNoBuffer variant is when a cycleway
    /// is it's own dedicated space, but is still
    /// a part of the road.
    fn is_dedicated_no_buffer(tag: &str) -> bool {
        matches!(tag, "lane" | "designated" | "track")
    }

    /// NoDedicatedWithFacilities variant is when a
    /// cycleway does not have a designated lane, but
    /// has some facilities for cycling awareness such as
    /// signage for sharing the road with vehicle traffic.
    fn is_no_dedicated_with_facilities(tag: &str) -> bool {
        matches!(tag, "crossing" | "shared" | "shared_lane")
    }
}
