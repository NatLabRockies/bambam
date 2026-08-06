use crate::model::feature::highway::Highway;

pub fn is_non_motorized_way(highway: &Highway) -> bool {
    matches!(
        highway,
        Highway::Cycleway
            | Highway::Path
            | Highway::Footway
            | Highway::Pedestrian
            | Highway::LivingStreet
    )
}

pub fn is_unbikeable_way(highway: &Highway) -> bool {
    matches!(
        highway,
        // Major roadways and their links
        Highway::Motorway | Highway::Trunk | Highway::MotorwayLink | Highway::TrunkLink
    )
}
