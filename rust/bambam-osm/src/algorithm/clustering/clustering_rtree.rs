use crate::model::osm::graph::OsmNodeId;

use super::ClusteredGeometry;
use geo::Convert;
use geo::{BoundingRect, Coord, Polygon};
use geozero::ToWkt;
use itertools::Itertools;
use kdam::tqdm;
use rstar::primitives::{GeomWithData, Rectangle};
use rstar::{RTree, RTreeObject};

pub type ClusteredIntersections = GeomWithData<Rectangle<(f32, f32)>, ClusteredGeometry>;

/// build an undirected graph of node geometries that intersect spatially.
/// clusters are represented simply, without any changes to their geometries and linear-time
/// intersection search for clusters. this performance hit is taken to avoid any edge cases
/// where manipulation via overlay operations might lead to representation errors.
pub fn build(
    geometries: &[(OsmNodeId, Polygon<f32>)],
) -> Result<RTree<ClusteredIntersections>, String> {
    // intersection performed via RTree.

    let mut rtree: RTree<ClusteredIntersections> = RTree::new();
    let iter = tqdm!(
        geometries.iter(),
        total = geometries.len(),
        desc = "spatial intersection"
    );

    // add each geometry to the rtree.
    for (index, polygon) in iter {
        let rect = rect_from_geometries(&[polygon])?;
        let query = GeomWithData::new(rect, ClusteredGeometry::new(*index, polygon.clone()));
        let intersecting = rtree
            .drain_in_envelope_intersecting(query.envelope())
            .sorted_by_key(|obj| obj.data.ids())
            .collect_vec();
        if intersecting.is_empty() {
            // nothing intersects with this new cluster, insert it and move on to the next row.
            rtree.insert(query);
        } else {
            // prepare to merge this geometry with any intersecting geometries by union.
            let mut new_cluster = ClusteredGeometry::new(*index, polygon.clone());

            for obj in intersecting.into_iter() {
                // it is still possible that none of the "intersecting" geometries actually
                // truly intersect since we only compared the bounding boxes at this point.
                if obj.data.intersects(polygon) {
                    // merge the intersecting data. since it was drained from the rtree, we are done.
                    new_cluster.merge_and_sort_with(&obj.data);
                } else {
                    // false alarm, this one doesn't actually intersect, put it back
                    // in the tree without changes since it was drained.
                    rtree.insert(obj);
                }
            }

            let new_rect = rect_from_geometries(&new_cluster.polygons())?;
            let new_obj = GeomWithData::new(new_rect, new_cluster);
            rtree.insert(new_obj);
        }
    }
    eprintln!();

    Ok(rtree)
}

/// helper function to create a rectangular rtree envelope for a given geometry
fn rect_from_geometries(ps: &[&Polygon<f32>]) -> Result<Rectangle<(f32, f32)>, String> {
    if ps.is_empty() {
        return Err(String::from(
            "rect_from_geometries called with empty collection",
        ));
    }
    let mut mins = vec![];
    let mut maxs = vec![];
    for p in ps {
        let bbox_rect = p.bounding_rect().ok_or_else(|| {
            format!("internal error: cannot get bounds of geometry: '{}'", {
                let p_f64: geo::Polygon<f64> = p.convert();
                geo::Geometry::from(p_f64).to_wkt().unwrap_or_default()
            })
        })?;
        mins.push(bbox_rect.min());
        maxs.push(bbox_rect.max());
    }
    let min_coord = mins
        .into_iter()
        .min_by_key(ordering_key)
        .ok_or_else(|| String::from("internal error: empty 'mins' collection"))?;
    let max_coord = maxs
        .into_iter()
        .max_by_key(ordering_key)
        .ok_or_else(|| String::from("internal error: empty 'maxs' collection"))?;
    let envelope = Rectangle::from_corners(min_coord.x_y(), max_coord.x_y());
    Ok(envelope)
}

/// called on WGS84 coordinates to create an ordering. since floating point
/// values have no ordering in Rust, we use scaling and conversion to i64
/// which is a feasible bijection since the max float values are +- 180.
fn ordering_key(coord: &Coord<f32>) -> (i64, i64) {
    let x = (coord.x * 100_000.0) as i64;
    let y = (coord.y * 100_000.0) as i64;
    (x, y)
}
