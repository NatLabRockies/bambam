use super::opportunity_spatial_row::OpportunitySpatialRow;
use bambam_core::model::{
    bambam_ops::DestinationsIter,
    output_plugin::opportunity::{
        DestinationOpportunity, OpportunityOrientation, OpportunityRowId,
    },
};
use geo::Convert;
use itertools::Itertools;
use routee_compass::plugin::output::OutputPluginError;
use routee_compass_core::{
    algorithm::search::{SearchInstance, SearchTreeNode},
    model::{label::Label, network::VertexId},
};
use rstar::{RTree, RTreeObject};
use std::collections::{HashMap, HashSet};

/// represents activities which can become opportunities if they
/// are reached by some travel mode.
pub enum OpportunityModel {
    /// user provides a dataset with opportunity counts for each id of either
    /// vertices (source, destination) or edges in the network. assignment of
    /// opportunity counts is done by a simple lookup function.
    Tabular {
        activity_types: Vec<String>,
        activity_counts: Vec<Vec<f64>>,
        opportunity_orientation: OpportunityOrientation,
    },
    // TODO: rewrite or remove spatial variant
    //   - one of the challenges posed by this variant is ensuring no double-counting.
    //     a spatial opportunity "zone" may be associated with more than one location.
    //     how do we prevent double-counting? we need a OpportunityRowId::Spatial or
    //     otherwise and to deduplicate our results. it may be easier to always tabularize
    //     the opportunity data instead.

    // /// user provides a spatial dataset of opportunities. lookup will use a
    // /// spatial index to find
    // ///   - intersecting polygons
    // ///   - nearest points with some distance tolerance
    // ///
    // /// it becomes the responsibility of the downstream code to de-duplicate results
    // /// by making sure to only include one row with a given index value (slot 1 of the
    // /// attach_opportunities function result).
    // Spatial {
    //     activity_types: Vec<String>,
    //     rtree: RTree<OpportunitySpatialRow>,
    //     counts_by_spatial_row: Vec<Vec<f64>>,
    //     polygonal: bool,
    //     opportunity_orientation: OpportunityOrientation,
    // },
    /// Combines multiple opportunity models
    Combined { models: Vec<Box<OpportunityModel>> },
}

impl OpportunityModel {
    /// get the list of activity type names for this model.
    pub fn activity_types(&self) -> Vec<String> {
        match self {
            OpportunityModel::Tabular { activity_types, .. } => activity_types.to_vec(),
            // OpportunityModel::Spatial { activity_types, .. } => activity_types.to_vec(),
            OpportunityModel::Combined { models } => {
                models.iter().flat_map(|m| m.activity_types()).collect_vec()
            }
        }
    }

    /// get the overall total number of opportunities available in the system given the provided
    /// opportunity model
    pub fn opportunity_totals(&self) -> Result<HashMap<String, f64>, String> {
        match self {
            OpportunityModel::Tabular {
                activity_types,
                activity_counts,
                ..
            } => activity_totals(activity_types, activity_counts),
            // OpportunityModel::Spatial {
            //     activity_types,
            //     counts_by_spatial_row: activity_counts,
            //     ..
            // } => activity_totals(activity_types, activity_counts),
            OpportunityModel::Combined { models } => {
                // sums inner model totals, appending when same activity type is present in multiple models
                let mut result: HashMap<String, f64> = HashMap::new();
                for m in models.iter() {
                    let totals = m.opportunity_totals()?;
                    for (act, cnt) in totals.into_iter() {
                        result
                            .entry(act)
                            .and_modify(|acc| *acc += cnt)
                            .or_insert(cnt);
                    }
                }
                Ok(result)
            }
        }
    }

    pub fn vector_length(&self) -> usize {
        self.activity_types().len()
    }

    /// collect all opportunities that are reachable by some collection of destinations, with a
    /// check to confirm no duplicate opportunities are found.
    ///
    /// # Arguments
    ///
    /// * `destinations` - an iterator over the destinations found during the search
    /// * `si` - the RouteE Compass [`SearchInstance`] for the associated search query
    ///
    /// # Returns
    ///
    /// A vector of (destination id, opportunity counts by category) for each destination id.
    /// The opportunity count vectors are ordered to match this [`OpportunityModel`]'s
    /// activity_types vector.
    pub fn collect_trip_opportunities(
        &self,
        destinations: DestinationsIter<'_>,
        si: &SearchInstance,
    ) -> Result<Vec<(OpportunityRowId, DestinationOpportunity)>, OutputPluginError> {
        let mut found = HashMap::new();
        for dest_result in destinations {
            match dest_result {
                Err(e) => {
                    let msg = format!("failure collecting destinations: {e}");
                    return Err(OutputPluginError::OutputPluginFailed(msg));
                }
                Ok((src, branch)) => {
                    let row = self.collect_destination_opportunities(&src, branch, si)?;
                    for (id, opps) in row.into_iter() {
                        if let Some(et) = branch.incoming_edge() {
                            let state = et.result_state.clone();
                            let row = DestinationOpportunity {
                                counts: opps,
                                state,
                            };
                            // "overwrite" behavior on duplicate opportunity keys here by
                            // implicitly suppressing the Some(_) case.
                            let _ = found.insert(id, row);
                        }
                    }
                }
            }
        }
        let result = found.into_iter().collect_vec();
        Ok(result)
    }

    /// attaches opportunity counts for a single location in the graph.
    ///
    /// # Arguments
    /// * `destination_vertex_id` - the destination that was reached
    /// * `search_tree_branch` - the branch in the search tree that reached this destination.
    /// * `si` - the RouteE Compass [`SearchInstance`] for the associated search query
    ///
    /// # Returns
    ///
    /// an opportunity vector id along with a vector of opportunity counts.
    fn collect_destination_opportunities(
        &self,
        origin_label: &Label,
        search_tree_branch: &SearchTreeNode,
        si: &SearchInstance,
    ) -> Result<Vec<(OpportunityRowId, Vec<f64>)>, OutputPluginError> {
        match self {
            OpportunityModel::Tabular {
                activity_types: _,
                activity_counts,
                opportunity_orientation,
            } => {
                let opp_row = OpportunityRowId::new(
                    origin_label,
                    search_tree_branch,
                    opportunity_orientation,
                )?;

                // at this time, only vertex-oriented opportunities are supported (refactor needed due to EdgeLists)
                let index = match &opp_row {
                    OpportunityRowId::OriginVertex(label) => label.vertex_id().0,
                    OpportunityRowId::DestinationVertex(label) => label.vertex_id().0,
                    OpportunityRowId::Edge(edge_list_id, edge_id) => {
                        return Err(OutputPluginError::InternalError(
                            "edge-oriented opportunities not yet implemented".to_string(),
                        ))
                    }
                };

                let result = activity_counts
                    .get(index)
                    .map(|opps| (opp_row, opps.clone()))
                    .ok_or_else(|| {
                        OutputPluginError::OutputPluginFailed(format!(
                            "activity table lookup failed - {opportunity_orientation} index {index} not found"
                        ))
                    })?;
                Ok(vec![result])
            }
            // OpportunityModel::Spatial {
            //     activity_types,
            //     rtree,
            //     counts_by_spatial_row: activity_counts,
            //     polygonal,
            //     opportunity_orientation,
            // } => {
            //     let index = OpportunityRowId::new(
            //         origin_label,
            //         search_tree_branch,
            //         opportunity_orientation,
            //     )?;

            //     // search for the intersecting polygonal opportunity or nearest point opportunity
            //     let spatial_row = if *polygonal {
            //         let envelope = index.get_envelope_f64(si)?;
            //         rtree.locate_in_envelope_intersecting(&envelope).next()
            //     } else {
            //         let centroid = index.get_centroid_f64(si)?;
            //         rtree.nearest_neighbor(&centroid)
            //     };

            //     // return the found activities stored at the associated spatial row
            //     match spatial_row {
            //         None => Ok(vec![(index, vec![0.0; activity_types.len()])]),
            //         Some(found) => match activity_counts.get(found.index) {
            //             Some(counts) => Ok(vec![(index, counts.clone())]),
            //             None => {
            //                 let geom_type = if *polygonal { "polygon" } else { "point" };
            //                 Err(OutputPluginError::OutputPluginFailed(format!(
            //                     "expected spatial {} activity count with index {} not found",
            //                     geom_type, found.index
            //                 )))
            //             }
            //         },
            //     }
            // }
            OpportunityModel::Combined { models } => {
                let mut collection: HashMap<OpportunityRowId, Vec<f64>> = HashMap::new();
                let mut padding_length: usize = 0;
                for model in models.iter() {
                    let vector_length = model.vector_length();
                    let matches = model
                        .collect_destination_opportunities(origin_label, search_tree_branch, si)?
                        .into_iter()
                        .collect::<HashMap<_, _>>();

                    // Get all indices that need to be updated (existing + new)
                    let all_indices = collection
                        .keys()
                        .cloned()
                        .chain(matches.keys().cloned())
                        .collect::<HashSet<_>>();

                    for idx in all_indices.into_iter() {
                        let vector_extension = match matches.get(&idx) {
                            Some(match_vector) => match_vector.clone(),
                            None => vec![0.0; vector_length],
                        };

                        collection
                            .entry(idx)
                            .and_modify(|existing| existing.extend(vector_extension.clone()))
                            .or_insert({
                                let mut new_counts = vec![0.0; padding_length];
                                new_counts.extend(vector_extension);
                                new_counts
                            });
                    }
                    padding_length += vector_length;
                }
                // ensure we are right-padded to the correct length as well
                let result = collection
                    .into_iter()
                    .map(|(k, mut v)| {
                        v.resize(padding_length, 0.0);
                        (k, v)
                    })
                    .collect_vec();
                Ok(result)
            }
        }
    }
}

/// sums all counts into a global total for each category
fn activity_totals(
    activity_types: &Vec<String>,
    activity_counts: &Vec<Vec<f64>>,
) -> Result<HashMap<String, f64>, String> {
    let mut sums = vec![0.0; activity_types.len()];
    for row in activity_counts {
        if activity_types.len() != row.len() {
            return Err(format!(
                "number of activity types and row columns must match, found {} != {}",
                activity_types.len(),
                row.len()
            ));
        }
        for idx in 0..row.len() {
            sums[idx] += row[idx];
        }
    }
    let result = activity_types
        .clone()
        .into_iter()
        .zip(sums)
        .collect::<HashMap<_, _>>();
    Ok(result)
}
