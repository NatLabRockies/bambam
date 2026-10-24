use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use super::serialize_ops as ops;
use crate::{
    app::network::NetworkEdgeListConfiguration,
    collection::{
        record::SegmentHeading, OvertureMapsCollectionError, SegmentAccessRestrictionWhen,
        SegmentFullType, TransportationCollection, TransportationSegmentRecord,
    },
    graph::{
        island_detection::IslandDetectionAlgorithm,
        segment_ops,
        serialize_ops::{clean_edge_list, compute_vertex_remapping},
        vertex_serializable::{
            attach_infrastructure_to_vertices, VertexCompleteRecord, VertexMappingRecord,
            DEFAULT_INFRASTRUCTURE_MATCH_DISTANCE_METERS,
        },
        OmfGraphSummary,
    },
};
pub use bambam_network::consts::*;
use bambam_network::VectorizedEdgeList;
use itertools::Itertools;
use rayon::prelude::*;
use routee_compass_core::model::network::{EdgeId, EdgeList, EdgeListId, Vertex};

pub type OmfEdgeList = VectorizedEdgeList<SegmentFullType>;

pub struct OmfGraphVectorized {
    pub vertices: Vec<Vertex>,
    pub vertices_complete: Vec<VertexCompleteRecord>,
    pub vertex_mappings: Vec<VertexMappingRecord>,
    pub edge_lists: Vec<OmfEdgeList>,
    pub edge_list_config: Vec<NetworkEdgeListConfiguration>,
    /// for each OMF ID, the vertex index
    pub vertex_lookup: HashMap<String, usize>,
}

impl OmfGraphVectorized {
    /// create a vectorized graph dataset from a [TransportationCollection]
    pub fn new(
        collection: &TransportationCollection,
        configuration: &[NetworkEdgeListConfiguration],
        island_detection_configuration: Option<IslandDetectionAlgorithm>,
    ) -> Result<Self, OvertureMapsCollectionError> {
        Self::new_with_match_distance(
            collection,
            configuration,
            island_detection_configuration,
            None,
        )
    }

    /// create a vectorized graph dataset from a [TransportationCollection] with a configurable
    /// spatial matching distance (in meters) for attaching infrastructure records to vertices.
    pub fn new_with_match_distance(
        collection: &TransportationCollection,
        configuration: &[NetworkEdgeListConfiguration],
        island_detection_configuration: Option<IslandDetectionAlgorithm>,
        infrastructure_match_distance_meters: Option<f64>,
    ) -> Result<Self, OvertureMapsCollectionError> {
        // process all connectors into vertices
        log::info!("Creating vertex lookup");
        let (mut vertices, mut vertex_lookup) =
            ops::create_vertices_and_lookup(&collection.connectors, None)?;

        log::info!("Processing edge lists");
        // for each mode configuration, create an edge list
        let mut edge_lists: Vec<OmfEdgeList> = vec![];
        for (index, edge_list_config) in configuration.iter().enumerate() {
            let edge_list_id = EdgeListId(index);

            // create arguments for segment processing into edges
            let mut filter = edge_list_config.filter.clone();
            filter.sort(); // sort for performance (filter.iter().all() below is short-circuiting)

            log::info!("Filtering edge list {edge_list_id}");
            // filter to the segments that match our travel mode filter(s)
            let segments: Vec<&TransportationSegmentRecord> = collection
                .segments
                .par_iter()
                .filter(|r| filter.iter().all(|f| f.matches_filter(r)))
                .collect();
            let segment_lookup = ops::create_segment_lookup(&segments);

            // the splits are locations in each segment record where we want to define a vertex
            // which may not yet exist on the graph. this is where we begin to impose directivity
            // in our records.
            log::info!("Creating splits");
            let mut splits = vec![];
            for heading in [SegmentHeading::Forward, SegmentHeading::Backward] {
                let mut when: SegmentAccessRestrictionWhen = edge_list_config.into();
                when.heading = Some(heading);

                let directed_splits = ops::find_splits(
                    &segments,
                    Some(&when),
                    segment_ops::process_simple_connector_splits,
                )?;
                splits.extend(directed_splits);
            }

            // create all edges based on the above split points using all vertices.
            log::info!("Creating edges");
            let edges = ops::create_edges(
                &segments,
                &segment_lookup,
                &splits,
                &vertices,
                &vertex_lookup,
                edge_list_id,
            )?;
            log::info!("Creating geometries");
            let geometries = ops::create_geometries(&segments, &segment_lookup, &splits)?;
            log::info!("Creating bearings");
            let bearings = ops::bearing_deg_from_geometries(&geometries)?;
            log::info!("Creating classes");
            let classes = ops::create_classes(&segments, &segment_lookup, &splits)?;

            log::info!("Creating speeds");
            let speeds = ops::create_speeds(&segments, &segment_lookup, &splits)?;
            log::info!("Creating speed lookup");
            let speed_lookup = ops::create_speed_by_class_lookup(
                &speeds,
                &segments,
                &segment_lookup,
                &splits,
                &classes,
            )?;

            // insert global speed value for reference
            log::info!("Computing global speed");
            let global_speed =
                ops::get_global_average_speed(&speeds, &segments, &segment_lookup, &splits)?;

            // standardized edge mappings
            log::info!("Computing edge mappings");
            let edge_mappings = ops::get_edge_mappings(&segments, &segment_lookup, &splits)?;

            // match speeds according to classes
            log::info!("Completing speeds vector with default global");
            let speeds = speeds
                .into_par_iter()
                .zip(&classes)
                .map(|(opt_speed, class)| match opt_speed {
                    Some(speed) => Some(speed),
                    None => speed_lookup.get(class).copied(),
                })
                // Fix the None with -1 for now
                .map(|opt| match opt {
                    Some(v) => v,
                    None => global_speed,
                })
                .collect::<Vec<f64>>();

            // transform speed lookup into owned string
            let mut speed_lookup = speed_lookup
                .iter()
                .map(|(&k, v)| (k.as_str(), *v))
                .collect::<HashMap<String, f64>>();
            speed_lookup.insert(String::from(GLOBAL_AVG_SPEED_KEY), global_speed);

            let edge_list = OmfEdgeList {
                edge_list_id,
                edges: EdgeList(edges.into_boxed_slice()),
                geometries,
                classes,
                speeds,
                speed_lookup,
                bearings,
                edge_mappings,
            };
            edge_lists.push(edge_list);
        }

        // Compute islands in resulting edge lists and remove island edges
        log::info!("Compute islands");
        if let Some(island_detection) = island_detection_configuration {
            let ref_edge_lists = edge_lists
                .iter()
                .map(|e| &e.edges)
                .collect::<Vec<&EdgeList>>();
            let island_edges = island_detection.run(&ref_edge_lists, &vertices)?;

            // Refactor Vec into Hashmap
            let mut edges_lookup: HashMap<EdgeListId, Vec<EdgeId>> = HashMap::new();
            for (a, b) in &island_edges {
                edges_lookup.entry(*a).or_default().push(*b);
            }

            // Compute and apply vertex remapping
            let vertex_remapping = compute_vertex_remapping(&vertices, &edge_lists, &island_edges)?;
            vertices = vertices
                .into_iter()
                .filter_map(|vertex| {
                    vertex_remapping[vertex.vertex_id.0].map(|vertex_id| Vertex {
                        vertex_id,
                        ..vertex
                    })
                })
                .collect();

            // dropping entries for removed vertices.
            vertex_lookup.retain(|_, v| vertex_remapping[*v].is_some());
            // Update vertex_lookup to reflect the remapped vertex indices,
            for v in vertex_lookup.values_mut() {
                *v = vertex_remapping[*v].ok_or(OvertureMapsCollectionError::InternalError(format!("vertex index {v} expected after island computation but was flagged for deletion")))?.0;
            }

            // Clean the edge lists
            log::info!("Apply islands algorithm result");
            edge_lists = edge_lists
                .into_iter()
                .map(|omf_list| {
                    let empty_vec = vec![];
                    let edges_to_remove: HashSet<&EdgeId> = edges_lookup
                        .get(&omf_list.edge_list_id)
                        .unwrap_or(&empty_vec)
                        .iter()
                        .collect();

                    let mask = omf_list
                        .edges
                        .0
                        .iter()
                        .map(|edge| !edges_to_remove.contains(&edge.edge_id))
                        .collect::<Vec<bool>>();

                    clean_edge_list(omf_list, mask, &vertex_remapping).map_err(Into::into)
                })
                .collect::<Result<Vec<OmfEdgeList>, OvertureMapsCollectionError>>()?;
        };

        // Order connector IDs by final vertex_id
        let connector_ids: Vec<String> = vertex_lookup
            .iter()
            .sorted_by_key(|(_, v)| **v)
            .map(|(k, _)| k.clone())
            .collect();

        let vertex_mappings: Vec<VertexMappingRecord> = connector_ids
            .iter()
            .enumerate()
            .map(|(vertex_id, source_id)| VertexMappingRecord {
                vertex_id,
                source_id: source_id.clone(),
            })
            .collect();

        let match_distance = infrastructure_match_distance_meters
            .unwrap_or(DEFAULT_INFRASTRUCTURE_MATCH_DISTANCE_METERS);
        log::info!(
            "Attaching {} infrastructure records to {} vertices (tolerance: {match_distance}m)",
            collection.infrastructure.len(),
            vertices.len()
        );
        let vertices_complete = attach_infrastructure_to_vertices(
            &vertices,
            &connector_ids,
            &collection.infrastructure,
            match_distance,
        );

        let result = Self {
            vertices,
            vertices_complete,
            vertex_mappings,
            edge_lists,
            edge_list_config: configuration.to_vec(),
            vertex_lookup,
        };

        Ok(result)
    }

    /// write the graph to disk in vectorized Compass format.
    pub fn write_compass(
        &self,
        summary: &OmfGraphSummary,
        output_directory: &Path,
        overwrite: bool,
    ) -> Result<(), OvertureMapsCollectionError> {
        kdam::term::init(false);
        kdam::term::hide_cursor().map_err(|e| {
            OvertureMapsCollectionError::InternalError(format!("progress bar error: {e}"))
        })?;

        // create output directory if missing
        crate::util::fs::create_dirs(output_directory)?;

        // write the TOML summary file
        write_summary(output_directory, summary)?;

        // copy default configuration file into the output directory
        crate::util::fs::copy_default_config(output_directory)?;

        let mode_names: Vec<String> = self
            .edge_list_config
            .iter()
            .map(|c| c.mode.clone())
            .collect();

        bambam_network::fs::write_compass_dataset(
            &self.vertices,
            &self.vertices_complete,
            &self.vertex_mappings,
            &self.edge_lists,
            &mode_names,
            |class: &SegmentFullType| class.as_str(),
            output_directory,
            overwrite,
        )?;

        kdam::term::show_cursor().map_err(|e| {
            OvertureMapsCollectionError::InternalError(format!("progress bar error: {e}"))
        })?;

        Ok(())
    }
}

fn write_summary(
    output_directory: &Path,
    summary: &OmfGraphSummary,
) -> Result<(), OvertureMapsCollectionError> {
    let summary_toml = toml::to_string_pretty(&summary).map_err(|e| {
        OvertureMapsCollectionError::InternalError(format!("failure serializing summary TOML: {e}"))
    })?;
    let summary_path = output_directory.join("summary.toml");
    std::fs::write(&summary_path, &summary_toml).map_err(|e| {
        OvertureMapsCollectionError::WriteError {
            path: summary_path,
            message: e.to_string(),
        }
    })
}
