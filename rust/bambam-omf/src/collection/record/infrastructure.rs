use std::collections::{HashMap, HashSet};

use geo::{Centroid, Geometry, Point};
use serde::{Deserialize, Serialize};

use crate::collection::{OvertureMapsCollectionError, OvertureRecord};

use super::{geometry_wkb_codec, OvertureMapsBbox, OvertureMapsNames, OvertureMapsSource};

/// Default Overture Maps `theme=base/type=infrastructure` classes relevant to
/// transportation network control and pedestrian/bicycle comfort metrics.
pub const DEFAULT_TRANSPORTATION_INFRASTRUCTURE_CLASSES: &[&str] = &[
    "traffic_signals",
    "stop",
    "stop_position",
    "give_way",
    "crossing",
    "pedestrian_crossing",
];

/// Represents an infrastructure record as defined in the Overture Maps Foundation
/// `theme=base/type=infrastructure` schema.
///
/// See <https://docs.overturemaps.org/schema/reference/base/infrastructure/>
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InfrastructureRecord {
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
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sources: Option<Vec<Option<OvertureMapsSource>>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub subtype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub names: Option<OvertureMapsNames>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub level: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub surface: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub wikidata: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub source_tags: Option<HashMap<String, Option<String>>>,
}

impl TryFrom<OvertureRecord> for InfrastructureRecord {
    type Error = OvertureMapsCollectionError;

    fn try_from(value: OvertureRecord) -> Result<Self, Self::Error> {
        match value {
            OvertureRecord::Infrastructure(record) => Ok(record),
            _ => Err(OvertureMapsCollectionError::DeserializeTypeError(format!(
                "Cannot transform record {value:#?} into InfrastructureRecord"
            ))),
        }
    }
}

impl InfrastructureRecord {
    /// Returns the default set of infrastructure `class` values queried for
    /// transportation network enrichment.
    pub fn default_transportation_classes() -> HashSet<String> {
        DEFAULT_TRANSPORTATION_INFRASTRUCTURE_CLASSES
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    }

    pub fn get_geometry(&self) -> Option<&Geometry<f32>> {
        self.geometry.as_ref()
    }

    /// Returns a representative point for spatial lookup (the point itself for
    /// `Geometry::Point`, or the centroid for linear/polygonal infrastructure
    /// features such as pedestrian crossings).
    pub fn get_reference_point(&self) -> Option<Point<f32>> {
        let geom = self.geometry.as_ref()?;
        match geom {
            Geometry::Point(pt) => Some(*pt),
            other => other.centroid(),
        }
    }

    pub fn is_traffic_signals(&self) -> bool {
        self.class.as_deref() == Some("traffic_signals")
    }

    pub fn is_stop(&self) -> bool {
        matches!(self.class.as_deref(), Some("stop" | "stop_position"))
    }

    pub fn is_give_way(&self) -> bool {
        self.class.as_deref() == Some("give_way")
    }

    pub fn is_crossing(&self) -> bool {
        matches!(
            self.class.as_deref(),
            Some("crossing" | "pedestrian_crossing")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::OvertureRecordType;
    use arrow::{
        array::{BinaryArray, Float32Array, Int32Array, RecordBatch, StringArray, StructArray},
        datatypes::{DataType, Field, Fields, Schema},
    };
    use geozero::{CoordDimensions, ToWkb};
    use std::sync::Arc;

    #[test]
    fn test_infrastructure_record_from_arrow_wkb_binary_batch() {
        let pt_f64 = geo::Geometry::Point(geo::Point::new(-105.2705_f64, 40.0150_f64));
        let wkb_bytes = pt_f64.to_wkb(CoordDimensions::xy()).unwrap();

        let bbox_fields = Fields::from(vec![
            Field::new("xmin", DataType::Float32, true),
            Field::new("xmax", DataType::Float32, true),
            Field::new("ymin", DataType::Float32, true),
            Field::new("ymax", DataType::Float32, true),
        ]);
        let bbox_array = StructArray::new(
            bbox_fields.clone(),
            vec![
                Arc::new(Float32Array::from(vec![Some(-105.2705_f32)])),
                Arc::new(Float32Array::from(vec![Some(-105.2705_f32)])),
                Arc::new(Float32Array::from(vec![Some(40.0150_f32)])),
                Arc::new(Float32Array::from(vec![Some(40.0150_f32)])),
            ],
            None,
        );

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("geometry", DataType::Binary, true),
            Field::new("bbox", DataType::Struct(bbox_fields), false),
            Field::new("version", DataType::Int32, false),
            Field::new("subtype", DataType::Utf8, true),
            Field::new("class", DataType::Utf8, true),
        ]));

        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(vec!["gers-infra-123"])),
                Arc::new(BinaryArray::from(vec![Some(wkb_bytes.as_slice())])),
                Arc::new(bbox_array),
                Arc::new(Int32Array::from(vec![1])),
                Arc::new(StringArray::from(vec![Some("transportation")])),
                Arc::new(StringArray::from(vec![Some("traffic_signals")])),
            ],
        )
        .unwrap();

        let records = OvertureRecordType::Infrastructure
            .process_batch::<InfrastructureRecord>(&batch)
            .unwrap();
        assert_eq!(records.len(), 1);
        let infra = InfrastructureRecord::try_from(records.into_iter().next().unwrap()).unwrap();
        assert_eq!(infra.id, "gers-infra-123");
        assert!(infra.is_traffic_signals());
        let ref_pt = infra.get_reference_point().unwrap();
        assert!((ref_pt.x() - (-105.2705)).abs() < 1e-4);
        assert!((ref_pt.y() - 40.0150).abs() < 1e-4);
    }
}
