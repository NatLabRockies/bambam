use crate::model::feature::highway::Highway;

pub fn is_non_motorized_way(highway: &Highway) -> bool {
    matches!(
        highway,
        Highway::Cycleway | Highway::Path | Highway::Footway | Highway::Pedestrian | Highway::Steps
    )
}
