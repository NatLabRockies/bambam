use geo::{
    line_measures::Densifiable, Buffer, CoordsIter, Haversine, LineString, MultiPoint,
    MultiPolygon, Point,
};
use routee_compass::plugin::output::OutputPluginError;
use routee_compass_core::{
    algorithm::search::SearchTreeNode,
    model::{label::Label, map::MapModel},
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
        buffer_stride: Option<Length>,
    },
    BufferedDestinationPoint {
        buffer_radius: Length,
        buffer_stride: Option<Length>,
    },
}

impl TryFrom<&GeometryModelConfig> for GeometryModel {
    type Error = OutputPluginError;

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
                    Err(OutputPluginError::BuildFailed(format!("linestring stride must be strictly positive, found {stride} {distance_unit}")))
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
                    Err(OutputPluginError::BuildFailed(format!("linestring buffer radius must be strictly positive, found {buffer_radius} {distance_unit}")))
                } else if buffer_stride.iter().any(|v| v <= &0.0) {
                    Err(OutputPluginError::BuildFailed(format!(
                        "buffer stride must be strictly positive, found negative"
                    )))
                } else {
                    Ok(Self::BufferedLinestring {
                        buffer_radius: distance_unit.to_uom(*buffer_radius),
                        buffer_stride: buffer_stride.map(|s| distance_unit.to_uom(s)),
                    })
                }
            }
            Conf::BufferedDestinationPoint {
                buffer_radius,
                buffer_stride,
                distance_unit,
            } => {
                if buffer_radius <= &0.0 {
                    Err(OutputPluginError::BuildFailed(format!("destination point buffer radius must be strictly positive, found {buffer_radius} {distance_unit}")))
                } else if buffer_stride.iter().any(|v| v <= &0.0) {
                    Err(OutputPluginError::BuildFailed(format!(
                        "buffer stride must be strictly positive, found negative"
                    )))
                } else {
                    Ok(Self::BufferedLinestring {
                        buffer_radius: distance_unit.to_uom(*buffer_radius),
                        buffer_stride: buffer_stride.map(|s| distance_unit.to_uom(s)),
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
                let points = self.linestring_to_points(linestring)?;
                result.extend(points);
            }
        }

        let mp = MultiPoint::new(result);
        Ok(mp)
    }

    pub fn linestring_to_points(
        &self,
        linestring: &LineString<f32>,
    ) -> Result<Vec<Point<f32>>, OutputPluginError> {
        match self {
            GeometryModel::DestinationPoint => {
                let last_point = dst_point(linestring)?;
                Ok(vec![last_point])
            }
            GeometryModel::LinestringCoordinates => Ok(linestring.points().collect()),
            GeometryModel::LinestringStride { stride } => {
                let meters = stride.get::<uom::si::length::meter>() as f32;
                let dense_linestring = linestring.densify(&Haversine, meters);
                Ok(dense_linestring.into_points())
            }
            GeometryModel::BufferedLinestring {
                buffer_radius,
                buffer_stride,
            } => {
                let radius = buffer_radius.get::<uom::si::length::meter>() as f32;
                let radius_latlon = radius / 111132.0;
                let buffered = linestring.buffer(radius_latlon);
                let final_mp = stride(buffered, buffer_stride.as_ref());
                let result = final_mp
                    .coords_iter()
                    .map(|coord| Point::from(coord))
                    .collect();
                Ok(result)
            }
            GeometryModel::BufferedDestinationPoint {
                buffer_radius,
                buffer_stride,
            } => {
                let radius = buffer_radius.get::<uom::si::length::meter>() as f32;
                let radius_latlon = radius / 111132.0;
                let dst = dst_point(linestring)?;
                let buffered = dst.buffer(radius_latlon);
                let final_mp = stride(buffered, buffer_stride.as_ref());
                let result = final_mp
                    .coords_iter()
                    .map(|coord| Point::from(coord))
                    .collect();
                Ok(result)
            }
        }
    }
}

/// grab the last point in a linestring. assumes a non-empty linestring.
fn dst_point(linestring: &LineString<f32>) -> Result<Point<f32>, OutputPluginError> {
    linestring
        .points()
        .next_back()
        .ok_or_else(|| OutputPluginError::OutputPluginFailed(format!("geometry has no points",)))
}

/// if a stride value is provided, apply it to the multipolygon.
fn stride(buffered: MultiPolygon<f32>, buffer_stride: Option<&Length>) -> MultiPolygon<f32> {
    if let Some(buffer_stride) = buffer_stride {
        let stride = buffer_stride.get::<uom::si::length::meter>() as f32;
        let dense_linestring = buffered.densify(&Haversine, stride);
        dense_linestring
    } else {
        buffered
    }
}
