use crate::model::osm::{
    graph::{osm_way_data::OsmWayData, AdjacencyDirection, OsmGraph, OsmNodeId, Path3},
    OsmError,
};
use itertools::Itertools;
use kdam::{tqdm, Bar, BarExt};
use rayon::prelude::*;
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

/// simplifies the graph based on the OSMNX is_endpoint and get_paths_to_simplify heuristics.
/// updates the graph adjacencies in-place.
/// after simplification, an adjacency may be aggregated.
pub fn simplify_graph(graph: &mut OsmGraph, parallelize: bool) -> Result<(), OsmError> {
    let endpoints: HashSet<OsmNodeId> = get_enpoint_node_ids(graph, parallelize)?;
    log::info!("simplify: identified {} simple endpoints", endpoints.len());

    let simplified_paths = get_paths_to_simplify(&endpoints, graph, parallelize)?;

    let (nodes_to_remove, simplified_ways) =
        generate_simplified_ways(&simplified_paths, graph, parallelize)?;

    update_graph_adjacencies(nodes_to_remove, simplified_ways, graph)?;

    log::info!(
        "simplified adjacency list has {} nodes, {} segments",
        graph.n_connected_nodes(),
        graph.n_connected_ways()
    );

    Ok(())
}

/// creates the collection of simplified paths between the provided set
/// of endpoint node ids.
fn get_paths_to_simplify(
    endpoints: &HashSet<OsmNodeId>,
    graph: &OsmGraph,
    parallelize: bool,
) -> Result<Vec<Path3>, OsmError> {
    log::info!(
        "get_paths_to_simplify: found {} endpoints out of {} total nodes",
        endpoints.len(),
        graph.n_connected_nodes()
    );
    // let graph_shared = Arc::new(graph);

    // create pairs of (endpoint, successor)
    let endpoint_successor_pairs = create_endpoint_successor_pairs(endpoints, graph, parallelize)?;

    // sorted.. for determinism™
    let sort_pairs_iter = tqdm!(
        endpoint_successor_pairs.iter().sorted(),
        total = endpoint_successor_pairs.len(),
        desc = "sort endpoint/successor pairs"
    );
    let sorted_pairs = sort_pairs_iter.collect_vec();

    // create simplified paths from (endpoint, successor) pairs
    let result: Vec<Path3> = find_simplified_paths(&sorted_pairs, endpoints, graph, parallelize)?;

    Ok(result)
}

/// finds valid pairs of (endpoint, successor) from which we can call the build_path function.
fn create_endpoint_successor_pairs<'a>(
    endpoints: &'a HashSet<OsmNodeId>,
    graph: &'a OsmGraph,
    parallelize: bool,
) -> Result<Vec<(&'a OsmNodeId, &'a OsmNodeId)>, OsmError> {
    // create pairs of (endpoint, successor)
    if parallelize {
        log::debug!(
            "simplify_ops::get_paths_to_simplify (parallel) - find (endpoint, successor) pairs"
        );
        let bar = Arc::new(Mutex::new(
            Bar::builder()
                .total(endpoints.len())
                .desc(String::from(
                    "simplify: find (endpoint,successor) pairs (parallelized)",
                ))
                .build()
                .map_err(OsmError::InternalError)?,
        ));
        let pairs = endpoints
            .into_par_iter()
            .flat_map(|src| {
                if let Ok(mut b) = bar.clone().lock() {
                    let _ = b.update(1);
                }
                find_successors_for_endpoint(src, endpoints, graph)
            })
            .collect::<Vec<(&OsmNodeId, &OsmNodeId)>>();
        eprintln!();
        Ok(pairs)
    } else {
        log::debug!(
            "simplify_ops::get_paths_to_simplify (synchronous) - find (endpoint, successor) pairs"
        );
        let successor_iter = tqdm!(
            endpoints.iter(),
            desc = "simplify: find (endpoint,successor) pairs (synchronous)",
            total = endpoints.len()
        );
        let pairs = successor_iter
            .flat_map(|src| find_successors_for_endpoint(src, endpoints, graph))
            .collect::<Vec<(&OsmNodeId, &OsmNodeId)>>();
        eprintln!();
        Ok(pairs)
    }
}

/// creates simplified paths from (endpoint, successor) pairs
fn find_simplified_paths(
    pairs: &Vec<&(&OsmNodeId, &OsmNodeId)>,
    endpoints: &HashSet<OsmNodeId>,
    graph: &OsmGraph,
    parallelize: bool,
) -> Result<Vec<Path3>, OsmError> {
    // build progress bar
    let par_str = if parallelize {
        "parallelized"
    } else {
        "synchronous"
    };
    let bar = Arc::new(Mutex::new(
        Bar::builder()
            .total(graph.n_connected_nodes())
            .desc(format!("simplify: find paths to simplify ({par_str})"))
            .build()
            .map_err(OsmError::InternalError)?,
    ));

    if parallelize {
        let paths_result = pairs
            .into_par_iter()
            .map(|(endpoint, successor)| {
                if let Ok(mut b) = bar.clone().lock() {
                    let _ = b.update(1);
                }
                build_path(endpoint, successor, endpoints, graph)
            })
            .collect::<Result<Vec<_>, _>>()?;
        eprintln!();
        Ok(paths_result)
    } else {
        let paths_result = pairs
            .iter()
            .map(|(endpoint, successor)| {
                if let Ok(mut b) = bar.clone().lock() {
                    let _ = b.update(1);
                }
                build_path(endpoint, successor, endpoints, graph)
            })
            .collect::<Result<Vec<_>, _>>()?;
        eprintln!();
        Ok(paths_result)
    }
}

fn update_graph_adjacencies(
    nodes_to_remove: Vec<OsmNodeId>,
    simplified_ways: Vec<OsmWayData>,
    graph: &mut OsmGraph,
) -> Result<(), OsmError> {
    let n_nodes_to_remove = nodes_to_remove.len();
    let n_edges_to_add = simplified_ways.len();

    let node_iter = tqdm!(
        nodes_to_remove.into_iter().sorted(),
        desc = "simplify: remove interstitial nodes",
        total = n_nodes_to_remove
    );
    for node_id in node_iter {
        // this implicitly removes the old adjacencies
        graph.disconnect_node(&node_id, true)?;
    }
    eprintln!();

    let way_iter = tqdm!(
        simplified_ways
            .into_iter()
            .sorted_by_cached_key(|w| w.osmid),
        desc = "simplify: add simplified ways",
        total = n_edges_to_add
    );
    for way in way_iter {
        let src = way.src_node_id()?;
        let dst = way.dst_node_id()?;
        graph.replace_ways(&src, &dst, vec![way])?;
    }
    eprintln!();

    Ok(())
}

/// produces simplified [`OsmWayData`] records by aggregating all of the [`OsmWayData`] records
/// found along the given paths. produces the list of interstitial [`OsmNodeId`]s to remove along with the
/// newly created aggregate [`OsmWayData`] records.
fn generate_simplified_ways(
    paths: &Vec<Path3>,
    graph: &OsmGraph,
    parallelize: bool,
) -> Result<(Vec<OsmNodeId>, Vec<OsmWayData>), OsmError> {
    // build progress bar
    let par_str = if parallelize {
        "parallelized"
    } else {
        "synchronous"
    };
    let bar = Arc::new(Mutex::new(
        Bar::builder()
            .total(graph.n_connected_nodes())
            .desc(format!(
                "simplify: create simplified adjacencies ({par_str})"
            ))
            .build()
            .map_err(OsmError::InternalError)?,
    ));

    let result: (Vec<Vec<OsmNodeId>>, Vec<OsmWayData>) = if parallelize {
        paths
            .into_par_iter()
            .map(|path| {
                if let Ok(mut bar_inner) = bar.clone().lock() {
                    let _ = bar_inner.update(1);
                }
                simplify_way(path, graph)
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .unzip()
    } else {
        paths
            .iter()
            .map(|path| {
                if let Ok(mut bar_inner) = bar.clone().lock() {
                    let _ = bar_inner.update(1);
                }
                simplify_way(path, graph)
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .unzip()
    };

    let (nodes, simplified_ways) = result;
    let mut nodes_to_remove = nodes.into_iter().flatten().collect_vec();
    nodes_to_remove.dedup();

    Ok((nodes_to_remove, simplified_ways))
}

/// produces a simplified [`OsmWayData`] record by aggregating all of the [`OsmWayData`] records
/// found along the given path. produces the list of interstitial [`OsmNodeId`]s to remove along with the
/// newly created aggregate [`OsmWayData`].
fn simplify_way(path: &Path3, graph: &OsmGraph) -> Result<(Vec<OsmNodeId>, OsmWayData), OsmError> {
    let ways = path
        .iter()
        .tuple_windows()
        .map(|(u, v)| graph.get_ways_from_od(u, v))
        .collect::<Result<Vec<_>, _>>()?;
    let ways_flattened = ways.into_iter().flatten().collect_vec();
    let new_way = OsmWayData::try_from(ways_flattened.as_slice())?;
    let interstitial_nodes = path
        .iter()
        .dropping(1)
        .dropping_back(1)
        .cloned()
        .collect_vec();
    Ok((interstitial_nodes, new_way))
}

/// from osmnx.simplification._get_paths_to_simplify, runs the logic of the python code
/// for finding (endpoint, successor) pairs. in python:
///
/// for endpoint in endpoints:
///   for successor in G.successors(endpoint):
///     if successor not in endpoints:
///
///
fn find_successors_for_endpoint<'a>(
    endpoint: &'a OsmNodeId,
    endpoints: &HashSet<OsmNodeId>,
    graph: &'a OsmGraph,
) -> Vec<(&'a OsmNodeId, &'a OsmNodeId)> {
    graph
        .neighbor_iterator(endpoint, AdjacencyDirection::Forward)
        .filter_map(|dst| {
            if !endpoints.contains(dst) {
                Some((endpoint, dst))
            } else {
                None
            }
        })
        .collect_vec()
}

/// for each (connected) node in the graph, test using the node_is_endpoint predicate.
fn get_enpoint_node_ids(
    graph: &OsmGraph,
    parallelize: bool,
) -> Result<HashSet<OsmNodeId>, OsmError> {
    // build progress bar
    let par_str = if parallelize {
        "parallelized"
    } else {
        "synchronous"
    };
    let bar = Arc::new(Mutex::new(
        Bar::builder()
            .total(graph.n_connected_nodes())
            .desc(format!("simplify: find enpoints ({par_str})"))
            .build()
            .map_err(OsmError::InternalError)?,
    ));

    // run algorithm
    if parallelize {
        // let endpoint_count = Arc::new(Mutex::new(0));
        // let error_count = Arc::new(Mutex::new(0));
        let result = graph
            .connected_node_iterator(false)
            .collect_vec()
            .par_iter()
            .filter_map(|id| {
                if let Ok(mut b) = bar.clone().lock() {
                    let _ = b.update(1);
                }
                // choosing filter_map over filter in order to end up with deref'd OsmNodeIds
                match node_is_endpoint(id, graph) {
                    Ok(true) => {
                        // if let Ok(mut count) = endpoint_count.clone().lock() {
                        //     *count += 1;
                        // }
                        Some(**id)
                    }
                    Ok(false) => None,
                    Err(e) => {
                        // if let Ok(mut count) = error_count.clone().lock() {
                        //     *count += 1;
                        // }
                        log::warn!("Error checking if node {id} is endpoint: {e}");
                        None
                    }
                }
            })
            .collect::<HashSet<_>>();
        // let final_endpoint_count = *endpoint_count.lock().unwrap();
        // let final_error_count = *error_count.lock().unwrap();
        // log::info!(
        //     "Found {} endpoints, {} errors during endpoint detection",
        //     final_endpoint_count,
        //     final_error_count
        // );
        eprintln!();
        Ok(result)
    } else {
        // let mut endpoint_count = 0;
        // let mut error_count = 0;
        let result = graph
            .connected_node_iterator(false)
            .filter_map(|id| match node_is_endpoint(id, graph) {
                Ok(true) => {
                    // endpoint_count += 1;
                    Some(*id)
                }
                Ok(false) => None,
                Err(e) => {
                    // error_count += 1;
                    log::warn!("Error checking if node {id} is endpoint: {e}");
                    None
                }
            })
            .collect::<HashSet<_>>();
        // log::info!(
        //     "Found {} endpoints, {} errors during endpoint detection",
        //     endpoint_count,
        //     error_count
        // );
        eprintln!();
        Ok(result)
    }
}

/// osmnx.simplification._build_path.
fn build_path(
    endpoint: &OsmNodeId,
    endpoint_successor: &OsmNodeId,
    endpoints: &HashSet<OsmNodeId>,
    graph: &OsmGraph,
) -> Result<Vec<OsmNodeId>, OsmError> {
    // # start building path from endpoint node through its successor
    let mut path = vec![*endpoint, *endpoint_successor];

    // # for each successor of the endpoint's successor
    // for this_successor in G.successors(endpoint_successor):
    for this_successor in graph.neighbor_iterator(endpoint_successor, AdjacencyDirection::Forward) {
        //     successor = this_successor
        let mut successor = this_successor;
        //     if successor not in path:
        if !path.contains(successor) {
            //         # if this successor is already in the path, ignore it, otherwise add
            //         # it to the path
            //         path.append(successor)
            path.push(*successor);
            //         while successor not in endpoints:
            while !endpoints.contains(successor) {
                //             # find successors (of current successor) not in path
                //             successors = [n for n in G.successors(successor) if n not in path]
                let successors = graph
                    .neighbor_iterator(successor, AdjacencyDirection::Forward)
                    .filter(|node_id| !path.contains(node_id))
                    .collect_vec();

                match successors[..] {
                    //             if len(successors) == 1:
                    [one_successor] => {
                        //             # 99%+ of the time there will be only 1 successor: add to path
                        //                 successor = successors[0]
                        //                 path.append(successor)
                        successor = one_successor;
                        path.push(*successor);
                    }
                    //             # handle relatively rare cases or OSM digitization quirks
                    //             elif len(successors) == 0:
                    [] => {
                        //                 if endpoint in G.successors(successor):
                        let endpoint_in_successors = graph.has_neighbor(
                            successor,
                            endpoint,
                            Some(AdjacencyDirection::Forward),
                        );
                        if endpoint_in_successors {
                            //                     # we have come to the end of a self-looping edge, so
                            //                     # add first node to end of path to close it and return
                            //                     return [*path, endpoint]
                            path.push(*endpoint);
                            return Ok(path);
                        }
                        //                 # otherwise, this can happen due to OSM digitization error
                        //                 # where a one-way street turns into a two-way here, but
                        //                 # duplicate incoming one-way edges are present
                        //                 msg = f"Unexpected simplify pattern handled near {successor}"
                        //                 utils.log(msg, level=lg.WARNING)
                        //                 return path
                        log::warn!("Unexpected simplify pattern handled near {successor}");
                        return Ok(path);
                    }
                    _ => {
                        //             else:  # pragma: no cover
                        //                 # if successor has >1 successors, then successor must have
                        //                 # been an endpoint because you can go in 2 new directions.
                        //                 # this should never occur in practice
                        //                 msg = f"Impossible simplify pattern failed near {successor}."
                        //                 raise GraphSimplificationError(msg)
                        return Err(OsmError::GraphSimplificationError(format!(
                            "Impossible simplify pattern failed near node {}, which should be an endpoint as it has {} successors {{{}}}",
                            successor,
                            successors.len(),
                            successors.iter().map(|s| format!("{s}")).join(", ")
                        )));
                    }
                }
            }

            //         # if this successor is an endpoint, we've completed the path
            //         return path
            return Ok(path);
        }
    }

    // # if endpoint_successor has no successors not already in the path, return
    // # the current path: this is usually due to a digitization quirk on OSM
    // return path
    Ok(path)
}

/// osmnx.simplification._is_endpoint. currently only implementing heuristics 1-3, as
/// 4+5 do not apply to our situation.
///
/// node is an endpoint if it satisfies one of the following rules:
///
///    1) It is its own neighbor (ie, it self-loops).
///
///    2) Or, it has no incoming edges or no outgoing edges (ie, all its incident edges are inbound or all its incident edges are outbound).
///
///    3) Or, it does not have exactly two neighbors and degree of 2 or 4.
///
///    4) Or, if `node_attrs_include` is not None and it has one or more of the attributes in `node_attrs_include`.
///
///    5) Or, if `edge_attrs_differ` is not None and its incident edges have different values than each other for any of the edge attributes in `edge_attrs_differ`.
fn node_is_endpoint(id: &OsmNodeId, graph: &OsmGraph) -> Result<bool, OsmError> {
    // neighbors is the set of unique nodes connected to this node
    let succ = graph.get_out_neighbors(id).unwrap_or_default();
    let pred = graph.get_in_neighbors(id).unwrap_or_default();
    let neighbors = pred.into_iter().chain(succ).collect::<HashSet<_>>();
    let n = neighbors.len();

    // degree is the number of edges incident to this node
    let in_edges = graph
        .in_multiedge_iterator(id)
        .collect::<Result<Vec<_>, _>>()?;
    let out_edges = graph
        .out_multiedge_iterator(id)
        .collect::<Result<Vec<_>, _>>()?;
    let in_deg = in_edges.into_iter().map(|e| e.len()).sum::<usize>();
    let out_deg = out_edges.into_iter().map(|e| e.len()).sum::<usize>();
    let d = in_deg + out_deg;

    // RULE 1
    // if the node appears in its list of neighbors, it self-loops: this is
    // always an endpoint
    if neighbors.contains(id) {
        // log::debug!("{} : endpoint=TRUE - self-loop (n={}, d={})", id, n, d);
        return Ok(true);
    }

    // RULE 2
    // if node has no incoming edges or no outgoing edges, it is an endpoint
    if in_deg == 0 || out_deg == 0 {
        // log::debug!(
        //     "{} : endpoint=TRUE - no in or no out edges (in_deg={}, out_deg={}, n={}, d={})",
        //     id,
        //     in_deg,
        //     out_deg,
        //     n,
        //     d
        // );
        return Ok(true);
    }

    // RULE 3
    // else, if it does NOT have 2 neighbors AND either 2 or 4 directed edges,
    // it is an endpoint. either it has 1 or 3+ neighbors, in which case it is
    // a dead-end or an intersection of multiple streets or it has 2 neighbors
    // but 3 degree (indicating a change from oneway to twoway) or more than 4
    // degree (indicating a parallel edge) and thus is an endpoint
    // if not ((n == 2) and (d in {2, 4})):
    if !((n == 2) && (d == 2 || d == 4)) {
        // log::debug!(
        //     "{} : endpoint=TRUE - not2 + 2or4 rule (n={}, d={})",
        //     id,
        //     n,
        //     d
        // );
        return Ok(true);
    }

    // // RULE 4  (SKIPPED)
    // // non-strict mode: does it contain an attr denoting that it is an endpoint
    // if node_attrs_include is not None and len(set(node_attrs_include) & G.nodes[node].keys()) > 0:
    //     return True

    // // RULE 5  (SKIPPED)
    // // non-strict mode: do its incident edges have different attr values? for
    // // each attribute to check, collect the attribute's values in all inbound
    // // and outbound edges. if there is more than 1 unique value then this node
    // // is an endpoint
    // if edge_attrs_differ is not None:
    //     for attr in edge_attrs_differ:
    //         in_values = {v for _, _, v in G.in_edges(node, data=attr, keys=False)}
    //         out_values = {v for _, _, v in G.out_edges(node, data=attr, keys=False)}
    //         if len(in_values | out_values) > 1:
    //             return True
    // log::debug!("{} : endpoint=FALSE (n={}, d={})", id, n, d);
    Ok(false)
}
