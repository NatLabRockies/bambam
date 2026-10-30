use std::collections::HashMap;

use super::{OsmNodes, OsmWays, Vertex};
use crate::{
    algorithm::truncation::ComponentFilter,
    model::osm::{
        import_ops::{
            extract_osm_connectors_and_segments_with_match_distance, process_osm_segment_splits,
            OsmCollection, OsmSegmentRecord,
        },
        OsmError,
    },
};
use bambam_network::{
    build_vectorized_network_graph, CliBoundingBox, EdgeListStats, EdgeMappingRecord,
    IslandDetectionAlgorithm, NetworkEdgeListConfiguration, NetworkGraphSource, NetworkGraphStats,
    NetworkGraphSummary, SegmentFullType, SegmentSplit, VectorizedEdgeList, VertexCompleteRecord,
    VertexMappingRecord,
};
use geo::{BoundingRect, Geometry};
use indexmap::IndexMap;

pub struct OsmGraphVectorized {
    /// study region / dataset name for `summary.toml`
    pub study_region: String,
    /// bounding box of the extent (if provided) for `summary.toml`
    pub bbox: Option<CliBoundingBox>,
    /// RouteE Compass vertices in final `VertexId` order
    pub vertices: Vec<Vertex>,
    /// enriched vertices with traffic_signals, stop, give_way, crossing
    pub vertices_complete: Vec<VertexCompleteRecord>,
    /// standardized vertex ID to OSM node ID mapping
    pub vertex_mappings: Vec<VertexMappingRecord>,
    /// mode-specific vectorized edge lists matching `bambam-omf` (`edge_lists` + `edge_list_config`)
    pub edge_lists: Vec<VectorizedEdgeList<SegmentFullType>>,
    /// mode configurations corresponding to `edge_lists`
    pub edge_list_config: Vec<NetworkEdgeListConfiguration>,
    /// for each OSM node ID string, the vertex index
    pub vertex_lookup: HashMap<String, usize>,
    /// raw OSM nodes and ways collection for Parquet retention
    pub raw_collection: Option<OsmCollection>,
}

impl OsmGraphVectorized {
    /// Builds an [`OsmGraphVectorized`] using the OMF-style Connector + Segment + Split
    /// workflow from `bambam-network` with default multimodal (`walk`, `bike`, `drive`)
    /// configurations.
    #[allow(clippy::too_many_arguments)]
    pub fn from_osm_maps(
        source_label: impl Into<String>,
        nodes_map: OsmNodes,
        ways_map: OsmWays,
        extent_opt: Option<&Geometry<f32>>,
        component_filter: &ComponentFilter,
        island_detection_opt: Option<&IslandDetectionAlgorithm>,
        truncate_by_edge: bool,
        simplify: bool,
        ignore_errors: bool,
        infrastructure_match_distance_meters: Option<f64>,
    ) -> Result<OsmGraphVectorized, OsmError> {
        let configs = NetworkEdgeListConfiguration::default_multimodal();
        Self::from_osm_maps_with_config(
            source_label,
            None,
            nodes_map,
            ways_map,
            extent_opt,
            &configs,
            component_filter,
            island_detection_opt,
            truncate_by_edge,
            simplify,
            ignore_errors,
            infrastructure_match_distance_meters,
        )
    }

    /// Builds an [`OsmGraphVectorized`] with optional mode-specific edge lists
    /// (e.g. `["walk", "bike", "drive"]`), sharing the exact same vertex space and
    /// output layout as `bambam-omf`.
    #[allow(clippy::too_many_arguments)]
    pub fn from_osm_maps_with_modes(
        source_label: impl Into<String>,
        nodes_map: OsmNodes,
        ways_map: OsmWays,
        extent_opt: Option<&Geometry<f32>>,
        component_filter: &ComponentFilter,
        island_detection_opt: Option<&IslandDetectionAlgorithm>,
        truncate_by_edge: bool,
        simplify: bool,
        ignore_errors: bool,
        infrastructure_match_distance_meters: Option<f64>,
        modes: Option<&[String]>,
    ) -> Result<OsmGraphVectorized, OsmError> {
        let configs = match modes.filter(|m| !m.is_empty()) {
            Some(mode_names) => mode_names
                .iter()
                .map(|m| {
                    NetworkEdgeListConfiguration::default_for_mode(m)
                        .map_err(OsmError::ConfigurationError)
                })
                .collect::<Result<Vec<_>, _>>()?,
            None => NetworkEdgeListConfiguration::default_multimodal(),
        };
        Self::from_osm_maps_with_config(
            source_label,
            None,
            nodes_map,
            ways_map,
            extent_opt,
            &configs,
            component_filter,
            island_detection_opt,
            truncate_by_edge,
            simplify,
            ignore_errors,
            infrastructure_match_distance_meters,
        )
    }

    /// Builds an [`OsmGraphVectorized`] using explicit [`NetworkEdgeListConfiguration`]s
    /// (such as from `bambam-network-config.json`) and multi-mode island detection, delegating
    /// directly to `bambam_network::build_vectorized_network_graph`.
    #[allow(clippy::too_many_arguments)]
    pub fn from_osm_maps_with_config(
        source_label: impl Into<String>,
        study_region_opt: Option<String>,
        nodes_map: OsmNodes,
        ways_map: OsmWays,
        extent_opt: Option<&Geometry<f32>>,
        configuration: &[NetworkEdgeListConfiguration],
        _component_filter: &ComponentFilter,
        island_detection_opt: Option<&IslandDetectionAlgorithm>,
        truncate_by_edge: bool,
        simplify: bool,
        ignore_errors: bool,
        infrastructure_match_distance_meters: Option<f64>,
    ) -> Result<OsmGraphVectorized, OsmError> {
        let source_str = source_label.into();
        let study_region = study_region_opt.unwrap_or_else(|| {
            std::path::Path::new(&source_str)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("osm_network")
                .to_string()
        });

        let bbox = extent_opt
            .and_then(|g| g.bounding_rect())
            .and_then(|rect| CliBoundingBox::from_coords(&rect.min(), &rect.max()).ok());

        let raw_collection = OsmCollection::from_maps(source_str, &nodes_map, &ways_map);

        let (connectors, infrastructure, segments) =
            extract_osm_connectors_and_segments_with_match_distance(
                &nodes_map,
                &ways_map,
                extent_opt,
                truncate_by_edge,
                simplify,
                ignore_errors,
                infrastructure_match_distance_meters,
            )?;

        if connectors.is_empty() {
            return Err(OsmError::NoNodesFound);
        }
        if segments.is_empty() {
            return Err(OsmError::NoWaysFound);
        }

        let effective_configs: Vec<NetworkEdgeListConfiguration> = if configuration.is_empty() {
            NetworkEdgeListConfiguration::default_multimodal()
        } else {
            configuration.to_vec()
        };

        let built = build_vectorized_network_graph(
            &connectors,
            &segments,
            &infrastructure,
            &effective_configs,
            island_detection_opt,
            infrastructure_match_distance_meters,
            process_osm_segment_splits,
            Some(osm_edge_mapping),
        )?;

        Ok(OsmGraphVectorized {
            study_region,
            bbox,
            vertices: built.vertices,
            vertices_complete: built.vertices_complete,
            vertex_mappings: built.vertex_mappings,
            edge_lists: built.edge_lists,
            edge_list_config: built.edge_list_config,
            vertex_lookup: built.vertex_lookup,
            raw_collection: Some(raw_collection),
        })
    }

    /// Builds a [`NetworkGraphSummary`] (`summary.toml`) matching the OMF summary structure.
    pub fn to_summary(&self) -> NetworkGraphSummary {
        let release = self
            .raw_collection
            .as_ref()
            .map(|c| format!("file://{}", c.source))
            .unwrap_or_else(|| "osm".to_string());
        let source = NetworkGraphSource::new(&release, &self.study_region, self.bbox.as_ref());
        let mut edge_list = IndexMap::new();
        for (cfg, el) in self.edge_list_config.iter().zip(self.edge_lists.iter()) {
            let stats = EdgeListStats::from_edge_list(el, |cls| cls.as_str());
            edge_list.insert(cfg.mode.clone(), stats);
        }
        let stats = NetworkGraphStats {
            vertices: self.vertices.len(),
            edge_list,
        };
        NetworkGraphSummary { source, stats }
    }
}

fn osm_edge_mapping(
    seg: &OsmSegmentRecord,
    edge_id: usize,
    split: &SegmentSplit,
) -> EdgeMappingRecord {
    EdgeMappingRecord {
        edge_id,
        source_id: seg.parent_way_id.to_string(),
        start_lr: seg.parent_start_lr,
        end_lr: seg.parent_end_lr,
        heading: split.heading.clone(),
    }
}
