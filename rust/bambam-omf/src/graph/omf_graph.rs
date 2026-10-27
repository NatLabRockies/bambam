use std::{collections::HashMap, path::Path};

use crate::{
    app::network::NetworkEdgeListConfiguration,
    collection::{OvertureMapsCollectionError, SegmentFullType, TransportationCollection},
    graph::{
        island_detection::IslandDetectionAlgorithm,
        segment_ops,
        vertex_serializable::{VertexCompleteRecord, VertexMappingRecord},
        NetworkGraphSummary,
    },
};
use bambam_network::{build_vectorized_network_graph, write_summary, VectorizedEdgeList};
use routee_compass_core::model::network::Vertex;

pub struct OmfGraphVectorized {
    pub vertices: Vec<Vertex>,
    pub vertices_complete: Vec<VertexCompleteRecord>,
    pub vertex_mappings: Vec<VertexMappingRecord>,
    pub edge_lists: Vec<VectorizedEdgeList<SegmentFullType>>,
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
        let built = build_vectorized_network_graph(
            &collection.connectors,
            &collection.segments,
            &collection.infrastructure,
            configuration,
            island_detection_configuration.as_ref(),
            infrastructure_match_distance_meters,
            segment_ops::process_simple_connector_splits,
            None,
        )?;

        Ok(Self {
            vertices: built.vertices,
            vertices_complete: built.vertices_complete,
            vertex_mappings: built.vertex_mappings,
            edge_lists: built.edge_lists,
            edge_list_config: built.edge_list_config,
            vertex_lookup: built.vertex_lookup,
        })
    }

    /// write the graph to disk in vectorized Compass format.
    pub fn write_compass(
        &self,
        summary: &NetworkGraphSummary,
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
