use crate::collection::{OvertureMapsCollectionError, OvertureRecord};

use super::geometry_wkb_codec;
use super::OvertureMapsBbox;
use geo::Geometry;
use routee_compass_core::model::network::Vertex;
use serde::{Deserialize, Serialize};

/// Represents a transportation connector record as defined in the Overture Maps Foundation schema.
///
/// NOTE: The fields defined on this struct directly drive the Parquet [`ProjectionMask`]
/// used when reading Overture GeoParquet files (`FieldRef::from_type::<TransportationConnectorRecord>()`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransportationConnectorRecord {
    pub id: String,
    #[serde(
        with = "geometry_wkb_codec",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub geometry: Option<Geometry<f32>>,
    #[serde(default)]
    pub bbox: OvertureMapsBbox,
    #[serde(default)]
    pub version: i32,
    // pub sources: Option<Vec<Option<OvertureMapsSource>>>,
}

impl TryFrom<OvertureRecord> for TransportationConnectorRecord {
    type Error = OvertureMapsCollectionError;

    fn try_from(value: OvertureRecord) -> Result<Self, Self::Error> {
        match value {
            OvertureRecord::Connector(record) => Ok(record),
            _ => Err(OvertureMapsCollectionError::DeserializeTypeError(format!(
                "Cannot transform record {value:#?} into TransportationConnectorRecord"
            ))),
        }
    }
}

impl TransportationConnectorRecord {
    pub fn get_geometry(&self) -> Option<&Geometry<f32>> {
        self.geometry.as_ref()
    }

    pub fn try_to_vertex(&self, idx: usize) -> Result<Vertex, OvertureMapsCollectionError> {
        let geometry =
            self.get_geometry()
                .ok_or(OvertureMapsCollectionError::SerializationError(format!(
                    "Invalid or empty geometry {:?}",
                    self.get_geometry()
                )))?;

        let (x, y) = match geometry {
            Geometry::Point(point) => Ok(point.x_y()),
            _ => Err(OvertureMapsCollectionError::SerializationError(format!(
                "Incorrect geometry in ConnectorRecord: {geometry:?}"
            ))),
        }?;

        Ok(Vertex::new(idx, x, y))
    }
}

impl bambam_network::ImportConnector for TransportationConnectorRecord {
    fn connector_id(&self) -> &str {
        &self.id
    }

    fn try_to_vertex(&self, idx: usize) -> Result<Vertex, bambam_network::NetworkImportError> {
        TransportationConnectorRecord::try_to_vertex(self, idx).map_err(Into::into)
    }
}
