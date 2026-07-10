use crate::model::osm::graph::OsmNodeDataSerializable;

pub fn compute_traffic_signal_score(src_node: &OsmNodeDataSerializable) -> i32 {
    if has_traffic_signals(src_node) {
        2
    } else if has_stop_sign(src_node) {
        1
    } else {
        0
    }
}

/// returns true if the node has a stop sign
pub fn has_stop_sign(node: &OsmNodeDataSerializable) -> bool {
    node.clone()
        .highway
        .as_ref()
        .is_some_and(|highway| highway.contains("stop"))
}

/// returns true if the node has a traffic light
pub fn has_traffic_signals(node: &OsmNodeDataSerializable) -> bool {
    node.clone()
        .highway
        .as_ref()
        .is_some_and(|highway| highway.contains("traffic_signals"))
}
