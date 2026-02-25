use crate::algorithm::consolidation::WayConsolidation;
use crate::algorithm::*;
use crate::model::osm::graph::AdjacencyDirection;
use crate::model::osm::graph::OsmNodeData;
use crate::model::osm::graph::OsmWayData;
use crate::model::osm::graph::OsmWayId;
use crate::model::osm::graph::{OsmGraph, OsmNodeId};
use crate::model::osm::OsmError;
use clustering::ClusteredIntersections;
use geo::Polygon;
use geo::{Coord, Geometry};
use itertools::Itertools;
use kdam::{term, tqdm, Bar, BarExt};
use rayon::prelude::*;
use rstar::RTree;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;

/// implements osmnx.simplification.consolidate_intersections with dead_ends=False, rebuild_graph=True,
/// reconnect_edges=True for the given distance tolerance.
/// comments describing the logic for this function is taken directly from osmnx's
/// osmnx.simplification._consolidate_intersections_rebuild_graph function.
///
/// # Arguments
///
/// * `graph`      - the original graph data from the .pbf file
/// * `tolerance`  - edge-connected endpoints within this distance threshold are merged
///  into a new graph vertex by their centroid
/// * `ignore_osm_parsing_errors` - if true, do not fail if a maxspeed or other attribute is not
///  valid wrt the OpenStreetMaps documentation
pub fn consolidate_graph(
    graph: &mut OsmGraph,
    tolerance: uom::si::f64::Length,
) -> Result<(), OsmError> {
    // STEP 1
    // buffer nodes to passed-in distance and merge overlaps. turn merged nodes
    // into gdf and get centroids of each cluster as x, y.

    log::info!("buffering with tolerance {tolerance:?}");
    let node_geometries = buffer_nodes(graph, tolerance)?;

    // STEP 2
    // attach each node to its cluster of merged nodes. first get the original
    // graph's node points then spatial join to give each node the label of
    // cluster it's within. make cluster labels type string.
    let mut rtree: RTree<ClusteredIntersections> =
        clustering::build(&node_geometries).map_err(|e| {
            OsmError::GraphConsolidationError(format!(
                "failure building geometry intersection graph: {e}"
            ))
        })?;

    // // DEBUG: before we "Drain" the tree
    // serde_json::to_writer(
    //     File::create("debug_nodes.json").unwrap(),
    //     &serde_json::to_value(node_geometries.iter().enumerate().collect_vec()).unwrap(),
    // );

    // return just the clusters. sorted for improved determinism.
    let clusters: Vec<Vec<OsmNodeId>> = rtree
        .drain()
        .map(|obj| obj.data.ids())
        .sorted()
        .collect_vec();
    let sum_conn = clusters.iter().map(|s| s.len() as f64).sum::<f64>();
    let avg_conn = sum_conn / clusters.len() as f64;
    log::info!(
        "spatial intersection graph has {} entries, avg {:.4} connections",
        clusters.len(),
        avg_conn
    );

    // STEP 3
    // if a cluster contains multiple components (i.e., it's not connected)
    // move each component to its own cluster (otherwise you will connect
    // nodes together that are not truly connected, e.g., nearby deadends or
    // surface streets with bridge).
    let merged_count = consolidate_clusters(&clusters, graph)?;
    if merged_count == 0 {
        return Err(OsmError::GraphConsolidationError(String::from(
            "merging simplified nodes resulted in 0 merged nodes",
        )));
    }

    log::info!("consolidated {merged_count} node clusters");
    // serde_json::to_writer(
    //     File::create("debug_merged.json").unwrap(),
    //     &serde_json::to_value(merged.iter().enumerate().collect_vec()).unwrap(),
    // )
    // .unwrap();

    ///////////////////////////////////////////////////////////////////////////////////
    // starting here, OSMNX has the trouble of coming back around to a valid NetworkX /
    // graph dataset with expected OSMNX attributes. in our case, our target is to    /
    // produce either a Compass Graph object or write {csv|txt}.gz files to disk.     /

    // STEP 4
    // create new empty graph and copy over misc graph data
    //   - we can probably ignore this step

    // STEP 5
    // create a new node for each cluster of merged nodes
    // regroup now that we potentially have new cluster labels from step 3
    // This step is no longer needed since we modify the graph in-place
    // // STEP 6
    // // create new edge from cluster to cluster for each edge in original graph
    // // STEP 7
    // // for every group of merged nodes with more than 1 node in it, extend the
    // // edge geometries to reach the new node point

    Ok(())
}

/// buffers the vertex geo::Points of the endpoints of the simplified graph
/// by some distance radius. returns the buffered geometries with matching
/// indices to the incoming endpoints dataset.
///
/// output geometries are in web mercator projection.
pub fn buffer_nodes(
    graph: &OsmGraph,
    radius: uom::si::f64::Length,
) -> Result<Vec<(OsmNodeId, Polygon<f32>)>, OsmError> {
    let bar = Arc::new(Mutex::new(
        Bar::builder()
            .total(graph.n_connected_nodes())
            .desc(format!(
                "node buffering ({} meters)",
                radius.get::<uom::si::length::meter>()
            ))
            .build()
            .map_err(OsmError::InternalError)?,
    ));

    let result = graph
        .connected_node_data_iterator(false)
        .collect::<Result<Vec<_>, _>>()?
        .into_par_iter()
        .map(|node| {
            let point = geo::Point(Coord::from((node.x, node.y)));
            let circle_g: Geometry<f32> = point.buffer(radius).map_err(|e| {
                OsmError::GraphConsolidationError(format!(
                    "while buffering nodes for consolidation, an error occurred: {e}"
                ))
            })?;
            let circle = match circle_g {
                Geometry::Polygon(polygon) => polygon,
                _ => {
                    return Err(OsmError::GraphConsolidationError(
                        "buffer of point produced non-polygonal geometry".to_string(),
                    ));
                }
            };
            if let Ok(mut b) = bar.clone().lock() {
                let _ = b.update(1);
            }
            Ok((node.osmid, circle))
        })
        .collect::<Result<Vec<_>, OsmError>>();

    result
}

// fn get_fill_value(
//     way: &OsmWayData,
//     maxspeeds_fill_lookup: &FillValueLookup,
// ) -> Result<uom::si::f64::Velocity, OsmError> {
//     let highway_class = way
//         .get_string_at_field("highway")
//         .map_err(OsmError::GraphConsolidationError)?;
//     let avg_speed = maxspeeds_fill_lookup.get(&highway_class);
//     let result = uom::si::f64::Velocity::new::<uom::si::velocity::kilometer_per_hour>(avg_speed);
//     Ok(result)
// }

/// with knowledge of which geometry indices contain spatially-similar nodes,
/// constructs new merged node data for the connected sub-clusters, assigning
/// the sub-cluster centroid as the new spatial coordinate.
fn consolidate_clusters(
    spatial_clusters: &[Vec<OsmNodeId>],
    graph: &mut OsmGraph,
) -> Result<usize, OsmError> {
    // for each spatial cluster,
    //   find sub-clusters by running a connected components search
    //   over the graph subset included in this spatial cluster

    // what keeps getting confused here is that we come up with the
    // endpoint indices somewhere between creating the SimplifiedGraph instance
    // and calling merge. we currently need to be able to bfs the simplified graph while
    // using geometry indices since the spatial clusters are collections of geometry indices.
    // perhaps they should instead be simplified graph node OSMIDs.

    log::info!(
        "consolidate clusters called with {} clusters over {} nodes",
        spatial_clusters.len(),
        spatial_clusters.iter().map(|c| c.len()).sum::<usize>()
    );

    term::init(false);
    term::hide_cursor().map_err(|e| OsmError::InternalError(format!("progress bar error: {e}")))?;
    let mut consolidated_count = 0;
    let outer_iter = tqdm!(
        spatial_clusters.iter(),
        total = spatial_clusters.len(),
        desc = "consolidate nodes",
        position = 0
    );
    for cluster in outer_iter {
        // run connected components to find the connected sub-graphs of this spatial cluster.
        // merge any discovered sub-components into a new node.
        let connected_clusters = ccc(cluster, graph)?;
        let cluster_iter = tqdm!(
            connected_clusters.into_iter(),
            desc = "find connected subgraphs within cluster",
            position = 1
        );
        for cluster_ids in cluster_iter {
            consolidate_nodes(cluster_ids, graph)?;
            consolidated_count += 1;
        }
    }

    eprintln!();
    eprintln!();
    term::show_cursor().map_err(|e| OsmError::InternalError(format!("progress bar error: {e}")))?;
    Ok(consolidated_count)
}

/// merges nodes into a new node, modifying the graph adjacencies accordingly.
///
/// # Arguments
///
/// * `node_ids` - nodes to consolidate. these should exist in the graph
/// * `graph` - the graph to inject the consolidated node
fn consolidate_nodes(node_ids: Vec<OsmNodeId>, graph: &mut OsmGraph) -> Result<(), OsmError> {
    if node_ids.len() <= 1 {
        return Ok(()); // Nothing to consolidate
    }

    log::debug!("Consolidating {} nodes: {:?}", node_ids.len(), node_ids);

    // arbitrarily picking the first osmid to be the osmid of the new node. the
    // remaining node ids will be removed along with the old version of the first osmid.
    let new_node_id: OsmNodeId = node_ids.first().cloned().ok_or_else(|| {
        OsmError::InternalError(String::from(
            "consolidate_nodes called with empty node_ids collection",
        ))
    })?;
    let remove_nodes = node_ids.iter().cloned().collect::<HashSet<_>>();

    // create a new node from the old nodes. does not mutate the graph.
    let nodes = &node_ids
        .iter()
        .map(|node_id| graph.get_node_data(node_id))
        .collect::<Result<Vec<_>, OsmError>>()?;
    let new_node = OsmNodeData::consolidate(&new_node_id, nodes)?;

    // collect the ways that are adjacent to all of the consolidated nodes as (u, v) id pairs
    // where u is the source, v is the destination of a way.
    // does not mutate the graph.
    let id_lookup = node_ids.iter().cloned().collect::<HashSet<_>>();
    let mut way_consolidation_map: HashMap<(OsmNodeId, AdjacencyDirection), HashSet<OsmWayId>> =
        HashMap::new();
    let mut all_ways: HashMap<OsmWayId, OsmWayData> = HashMap::new();

    for consolidated_id in node_ids.iter() {
        let neighbors = graph.get_directed_neighbors(consolidated_id);
        for (neighbor, dir) in neighbors.iter() {
            if id_lookup.contains(neighbor) {
                continue; // ignore ways that connect to any nodes being consolidated
            }

            // get the (directed) node pair containing way data using the original node IDs
            let (src, dst) = match dir {
                AdjacencyDirection::Forward => (consolidated_id, neighbor),
                AdjacencyDirection::Reverse => (neighbor, consolidated_id),
            };
            let adjacent_ways = graph.get_ways_from_od(src, dst)?;

            // Group way IDs by (neighbor, direction) and store way data
            let key = (*neighbor, *dir);
            let way_ids = way_consolidation_map.entry(key).or_default();
            for way in adjacent_ways {
                way_ids.insert(way.osmid);
                all_ways.insert(way.osmid, way.clone());
            }
        }
    }

    // Convert to consolidation records and update way nodes
    let mut way_consolidation_records = vec![];
    for ((neighbor, dir), way_ids) in way_consolidation_map.into_iter() {
        // Collect the actual way data for these IDs
        let mut ways: Vec<OsmWayData> = way_ids
            .into_iter()
            .filter_map(|way_id| all_ways.get(&way_id).cloned())
            .collect();

        update_way_nodes(&mut ways, &new_node_id, &remove_nodes, &dir)?;
        let way_consolidation = WayConsolidation::new(&neighbor, &dir, ways);
        way_consolidation_records.push(way_consolidation);
    }

    // begin graph mutation with DELETIONS.
    // here we remove every node from the original node id list.
    // this detaches all ways that touched any of the nodes as well, which we have stored clones of above.
    // the new consolidated node will be added back in next.
    log::debug!("Removing {} nodes before consolidation", node_ids.len());
    for node_id in node_ids.iter() {
        if node_id == &new_node_id {
            log::debug!("Removing node {node_id} (to be reintroduced as the consolidated node)");
            graph.remove_node(node_id)?;
        } else {
            log::debug!("Disconnecting node {node_id} during consolidation");
            graph.disconnect_node(node_id, true)?;
        }
    }

    // at this point, we can begin ADDITIONS, starting with the new consolidated node.
    log::debug!("Creating consolidated node with ID {new_node_id}");
    graph.create_isolated_node(new_node)?;

    // way insertion. for each (u, v) pair and way, wire it together with the consolidated
    // node and whatever node existed outside of the consolidated nodes.
    log::debug!(
        "Adding {} way consolidation records",
        way_consolidation_records.len()
    );
    for way_consolidation in way_consolidation_records.iter_mut() {
        let (src, dst) = way_consolidation.get_src_dst(&new_node_id);
        log::debug!("Adding adjacency from {src} to {dst}");
        graph.add_new_adjacency(&src, &dst, way_consolidation.drain_ways())?;
    }

    Ok(())
}

/// modifies the way.nodes collection so it does not include any removed nodes and
/// the new consolidated node is inserted in the correct place depending on the way direction.
fn update_way_nodes(
    ways: &mut [OsmWayData],
    new_node_id: &OsmNodeId,
    remove_nodes: &HashSet<OsmNodeId>,
    dir: &AdjacencyDirection,
) -> Result<(), OsmError> {
    for (way_idx, way) in ways.iter_mut().enumerate() {
        if way.nodes.is_empty() {
            return Err(OsmError::InternalError(format!(
                    "during consolidation (but before way.nodes update), way ()-[{}]->() (way index {}) has empty node list",
                    way.osmid, way_idx
                )));
        }

        // remove consolidated nodes from the Way nodelist, they are becoming a single point
        way.nodes.retain(|n| !remove_nodes.contains(n));

        // insert the new node in the correct position along this way
        match dir {
            AdjacencyDirection::Forward => {
                way.nodes.insert(0, *new_node_id);
            }
            AdjacencyDirection::Reverse => {
                way.nodes.push(*new_node_id);
            }
        };
    }

    Ok(())
}

/// connected components clustering algorithm.
/// finds the full set of sub-components within the provided set of
/// geometry indices.
///
/// # Arguments
/// * `geometry_indices`             - indices into the spatial intersection vector that will be considered for clustering
/// * `simplified`                   - the simplified graph
/// * `endpoint_index_osmid_mapping` - maps indices to Node OSMIDs
///
/// # Returns
///
/// A vector of vectors, each representing the sub-graph of the spatial cluster that is
/// connected in the simplified graph. these are Node OSMIDs so that that can be used to build a MergedNodeData
/// over a new vector of indexed [`MergedNodeData`].
fn ccc(cluster_ids: &[OsmNodeId], graph: &OsmGraph) -> Result<Vec<Vec<OsmNodeId>>, OsmError> {
    // handle trivial cases that do not require executing this algorithm
    match *cluster_ids {
        [] => return Ok(vec![]),
        [singleton] => {
            return Ok(vec![vec![singleton]]);
        }
        _ => {}
    };

    let mut clusters: Vec<Vec<OsmNodeId>> = vec![];
    let mut assigned: HashSet<OsmNodeId> = HashSet::default();

    // build the iterator over the nodes in the spatial overlay result, but instead
    // of using their geometry index, use their NodeOSMID.
    // only do a progress bar for non-trivial sizes of the geometry_ids argument
    // such as things larger than a road network intersection.
    // let use_progress_bar = cluster_ids.len() > 1000;
    let use_progress_bar = false;
    let cc_iter: Box<dyn Iterator<Item = &OsmNodeId>> = if use_progress_bar {
        Box::new(tqdm!(
            cluster_ids.iter(),
            total = cluster_ids.len(),
            desc = "connected components"
        ))
    } else {
        Box::new(cluster_ids.iter())
    };

    // store the NodeOsmids for quick lookup (the "valid set")
    let valid_set: HashSet<OsmNodeId> = cluster_ids.iter().cloned().collect::<HashSet<_>>();

    // as we iterate through each of the node ids in this spatial cluster,
    // we are looking to assign them to at least one sub-graph.
    for this_node_id in cc_iter {
        if !assigned.contains(this_node_id) {
            // found a label that is unassigned. begin the next cluster.
            // for each clustered geometry index, label it assigned and add it to this cluster
            let clustered_nodes = bfs_undirected(*this_node_id, graph, Some(&valid_set))?;
            let next_cluster = clustered_nodes
                .iter()
                .map(|n| {
                    assigned.insert(*n);
                    *n
                })
                .collect_vec();
            clusters.push(next_cluster);
        }
    }
    if use_progress_bar {
        eprintln!();
    }
    let out_size: usize = clusters.iter().map(|c| c.len()).sum();
    if out_size != cluster_ids.len() {
        // all nodes should be assigned to exactly one output vector.
        return Err(OsmError::GraphConsolidationError(format!(
            "ccc input size != output size ({} != {})",
            cluster_ids.len(),
            out_size
        )));
    }
    Ok(clusters)
}
