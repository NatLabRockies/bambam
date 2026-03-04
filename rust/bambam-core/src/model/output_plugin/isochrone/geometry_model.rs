use geo::{line_measures::Densifiable, Haversine, LineString, MultiPoint, Point};
use routee_compass::plugin::{output::OutputPluginError, PluginError};
use routee_compass_core::{
    algorithm::search::SearchTreeNode,
    model::{label::Label, map::MapModel, network::EdgeId},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uom::si::f64::Length;

use crate::model::output_plugin::isochrone::GeometryModelConfig;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum GeometryModel {
    DestinationPoint,
    LinestringCoordinates,
    LinestringStride {
        stride: Length,
    },
    BufferedLinestring {
        buffer_radius: Length,
        buffer_stride: Length,
    },
    BufferedDestinationPoint {
        buffer_radius: Length,
        buffer_stride: Length,
    },
}

impl TryFrom<&GeometryModelConfig> for GeometryModel {
    type Error = PluginError;

    fn try_from(value: &GeometryModelConfig) -> Result<Self, Self::Error> {
        use GeometryModelConfig as Conf;
        match value {
            Conf::DestinationPoint => Ok(Self::DestinationPoint),
            Conf::LinestringCoordinates => Ok(Self::LinestringCoordinates),
            Conf::LinestringStride {
                stride,
                distance_unit,
            } => {
                if stride <= &0.0 {
                    Err(OutputPluginError::BuildFailed(format!("linestring stride must be strictly positive, found {stride} {distance_unit}")).into())
                } else {
                    Ok(Self::LinestringStride {
                        stride: distance_unit.to_uom(*stride),
                    })
                }
            }
            Conf::BufferedLinestring {
                buffer_radius,
                buffer_stride,
                distance_unit,
            } => {
                if buffer_radius <= &0.0 {
                    Err(OutputPluginError::BuildFailed(format!("linestring buffer radius must be strictly positive, found {buffer_radius} {distance_unit}")).into())
                } else if buffer_stride <= &0.0 {
                    Err(OutputPluginError::BuildFailed(format!("linestring stride must be strictly positive, found {buffer_stride} {distance_unit}")).into())
                } else {
                    Ok(Self::BufferedLinestring {
                        buffer_stride: distance_unit.to_uom(*buffer_stride),
                        buffer_radius: distance_unit.to_uom(*buffer_radius),
                    })
                }
            }
            Conf::BufferedDestinationPoint {
                buffer_radius,
                buffer_stride,
                distance_unit,
            } => {
                if buffer_radius <= &0.0 {
                    Err(OutputPluginError::BuildFailed(format!("destination point buffer radius must be strictly positive, found {buffer_radius} {distance_unit}")).into())
                } else if buffer_stride <= &0.0 {
                    Err(OutputPluginError::BuildFailed(format!("destination point stride must be strictly positive, found {buffer_stride} {distance_unit}")).into())
                } else {
                    Ok(Self::BufferedDestinationPoint {
                        buffer_stride: distance_unit.to_uom(*buffer_stride),
                        buffer_radius: distance_unit.to_uom(*buffer_radius),
                    })
                }
            }
        }
    }
}

impl GeometryModel {
    pub fn generate_destination_points(
        &self,
        destinations: &[(Label, &SearchTreeNode)],
        map_model: Arc<MapModel>,
    ) -> Result<MultiPoint<f32>, OutputPluginError> {
        let mut result: Vec<Point<f32>> = Vec::new();
        for (_label, branch) in destinations.iter() {
            if let Some(e) = branch.incoming_edge() {
                let linestring = map_model
                    .get_linestring(&e.edge_list_id, &e.edge_id)
                    .map_err(|e| {
                        OutputPluginError::OutputPluginFailed(format!(
                            "failure generating destination points: {e}"
                        ))
                    })?;
                let points = self.linestring_to_points(e.edge_id, linestring)?;
                result.extend(points);
            }
        }

        let mp = MultiPoint::new(result);
        Ok(mp)
    }

    pub fn linestring_to_points(
        &self,
        edge_id: EdgeId,
        linestring: &LineString<f32>,
    ) -> Result<Vec<Point<f32>>, OutputPluginError> {
        match self {
            GeometryModel::DestinationPoint => {
                let last_point = linestring.points().next_back().ok_or_else(|| {
                    OutputPluginError::OutputPluginFailed(format!(
                        "geometry for edge_id {edge_id} has no points",
                    ))
                })?;
                Ok(vec![last_point])
            }
            GeometryModel::LinestringCoordinates => Ok(linestring.points().collect()),
            GeometryModel::LinestringStride { stride } => {
                let meters = stride.get::<uom::si::length::meter>() as f32;
                let dense_linestring = linestring.densify(&Haversine, meters);
                Ok(dense_linestring.into_points())
            }
            GeometryModel::BufferedLinestring {
                buffer_radius: _,
                buffer_stride: _,
            } => {
                todo!("geo rust does not currently support geometry buffering")
            }
            GeometryModel::BufferedDestinationPoint {
                buffer_radius: _,
                buffer_stride: _,
            } => todo!("geo rust does not currently support geometry buffering"),
        }
    }
}
