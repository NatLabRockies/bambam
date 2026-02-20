use std::sync::Arc;

use geo::{Centroid, Convert};
use routee_compass::plugin::output::OutputPluginError;
use routee_compass_core::{
    algorithm::search::{SearchInstance, SearchTreeNode},
    model::{
        label::Label,
        map::MapModel,
        network::{EdgeId, EdgeListId, Graph},
    },
};
use rstar::{RTreeObject, AABB};
use serde::Serialize;
use wkt::ToWkt;

use crate::model::output_plugin::opportunity::opportunity_orientation::OpportunityOrientation;

// identifier in the graph tagging where an opportunity was found
#[derive(Serialize, Clone, PartialEq, Eq, Hash, Debug)]
pub enum OpportunityRowId {
    OriginVertex(Label),
    DestinationVertex(Label),
    Edge(EdgeListId, EdgeId),
}

impl std::fmt::Display for OpportunityRowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            OpportunityRowId::OriginVertex(label) => label.to_string(),
            OpportunityRowId::DestinationVertex(label) => label.to_string(),
            OpportunityRowId::Edge(edge_list_id, edge_id) => format!("{edge_list_id}-{edge_id}"),
        };
        write!(f, "{s}")
    }
}

impl OpportunityRowId {
    /// create a new opportunity vector identifier based on the table orientation which denotes where opportunities are stored
    pub fn new(
        child_label: &Label,
        branch: &SearchTreeNode,
        format: &OpportunityOrientation,
    ) -> Result<OpportunityRowId, OutputPluginError> {
        use OpportunityOrientation as O;
        match format {
            // stored at the origin of the edge, corresponding with the branch origin id
            O::OriginVertexOriented => match branch.parent_label() {
                None => Err(OutputPluginError::InternalError(String::from("while building EdgeOriented OpportunityRowId, was passed tree root, which has no corresponding edge"))),
                Some(parent_label) => Ok(Self::OriginVertex(parent_label.clone())),
            },
            // stored at the destination of the edge at the branch's terminal vertex id
            O::DestinationVertexOriented => {
                Ok(Self::DestinationVertex(child_label.clone()))
            },
            // stored on the edge itself
            O::EdgeOriented => {
                match branch.incoming_edge() {
                    None => Err(OutputPluginError::InternalError(String::from("while building EdgeOriented OpportunityRowId, was passed tree root, which has no corresponding edge"))),
                    Some(et) => Ok(Self::Edge(et.edge_list_id, et.edge_id)),
                }
            }
        }
    }

    /// helper to get the POINT geometry associated with this index, if defined
    pub fn get_vertex_point(
        &self,
        graph: Arc<Graph>,
    ) -> Result<geo::Point<f32>, OutputPluginError> {
        let vertex_id = match self {
            OpportunityRowId::OriginVertex(label) => Ok(label.vertex_id()),
            OpportunityRowId::DestinationVertex(label) => Ok(label.vertex_id()),
            OpportunityRowId::Edge(..) => Err(OutputPluginError::InternalError(String::from(
                "cannot get vertex point for edge",
            ))),
        }?;

        let vertex = graph.get_vertex(vertex_id).map_err(|_e| {
            OutputPluginError::OutputPluginFailed(format!("unknown vertex id '{vertex_id}'"))
        })?;
        let point = geo::Point::new(vertex.x(), vertex.y());
        Ok(point)
    }

    /// helper to get the LINESTRING geometry associated with this index, if defined
    pub fn get_edge_linestring(
        &self,
        map_model: Arc<MapModel>,
    ) -> Result<geo::LineString<f32>, OutputPluginError> {
        let (edge_list_id, edge_id) = match self {
            OpportunityRowId::Edge(edge_list_id, edge_id) => Ok((edge_list_id, edge_id)),
            _ => Err(OutputPluginError::InternalError(String::from(
                "cannot get edge linestring for vertex",
            ))),
        }?;
        map_model
            .get_linestring(edge_list_id, edge_id)
            .cloned()
            .map_err(|_e| {
                OutputPluginError::OutputPluginFailed(format!("unknown edge id '{edge_id}'"))
            })
    }

    pub fn get_envelope_f64(
        &self,
        si: &SearchInstance,
    ) -> Result<AABB<geo::Point>, OutputPluginError> {
        match self {
            OpportunityRowId::OriginVertex(_) => {
                let point = self.get_vertex_point(si.graph.clone())?.convert();
                Ok(point.envelope())
            }
            OpportunityRowId::DestinationVertex(_) => {
                let point = self.get_vertex_point(si.graph.clone())?.convert();
                Ok(point.envelope())
            }
            OpportunityRowId::Edge(..) => {
                let linestring = self.get_edge_linestring(si.map_model.clone())?.convert();
                Ok(linestring.envelope())
            }
        }
    }

    pub fn get_centroid_f64(&self, si: &SearchInstance) -> Result<geo::Point, OutputPluginError> {
        match self {
            OpportunityRowId::OriginVertex(_) => {
                let point = self.get_vertex_point(si.graph.clone())?.convert();
                let centroid = point.centroid();
                Ok(centroid)
            }
            OpportunityRowId::DestinationVertex(_) => {
                let point = self.get_vertex_point(si.graph.clone())?.convert();
                let centroid = point.centroid();
                Ok(centroid)
            }
            OpportunityRowId::Edge(..) => {
                let linestring = self.get_edge_linestring(si.map_model.clone())?.convert();
                let centroid = linestring.centroid().ok_or_else(|| {
                    OutputPluginError::OutputPluginFailed(format!(
                        "could not get centroid of LINESTRING {}",
                        linestring.to_wkt()
                    ))
                })?;
                Ok(centroid)
            }
        }
    }
}
