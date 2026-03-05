use geo::{LineString, MultiPoint, Point};
use itertools::Itertools;
use routee_compass::plugin::output::OutputPluginError;
use routee_compass_core::{algorithm::search::SearchTreeNode, model::network::VertexId};
use std::collections::HashMap;

pub fn create_multipoint(
    tree: &HashMap<VertexId, SearchTreeNode>,
    geoms: &[LineString<f64>],
) -> Result<MultiPoint, OutputPluginError> {
    let edge_ids = tree
        .values()
        .flat_map(|node| node.incoming_edge().map(|e| e.edge_id))
        .collect::<Vec<_>>();

    let points = edge_ids
        .iter()
        .map(|eid| {
            let linestring = geoms.get(eid.0).ok_or_else(|| {
                OutputPluginError::OutputPluginFailed(format!("no geometry for edge_id {}", (*eid)))
            });

            linestring.map(|l| l.clone().into_points())
        })
        .flatten_ok()
        .collect::<Result<Vec<Point>, OutputPluginError>>()?;
    let geometry = MultiPoint::new(points);
    Ok(geometry)
}
