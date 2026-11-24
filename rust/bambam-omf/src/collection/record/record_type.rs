use arrow::{array::RecordBatch, datatypes::FieldRef};
use serde::de::DeserializeOwned;
use serde_arrow::schema::{SchemaLike, TracingOptions};

use crate::collection::{OvertureMapsCollectionError, OvertureRecord};

/// Tracing options used to introspect the top-level fields of Overture record structs
/// for both Parquet column projection (`ProjectionMask`) and `raw/*.parquet` serialization.
pub fn record_schema_tracing_options() -> TracingOptions {
    TracingOptions::default()
        .allow_null_fields(true)
        .enums_without_data_as_strings(true)
        .map_as_struct(false)
}

/// Derives the top-level Parquet column names directly from the target Rust record type `R`
/// via `serde_arrow` schema tracing (`FieldRef::from_type::<R>`).
///
/// This guarantees that the Parquet `ProjectionMask` stays automatically synchronized with
/// the fields defined on `R` without maintaining a separate column list.
pub fn projected_columns_for_record<R>() -> Result<Vec<String>, OvertureMapsCollectionError>
where
    R: DeserializeOwned,
{
    let fields = Vec::<FieldRef>::from_type::<R>(record_schema_tracing_options()).map_err(|e| {
        OvertureMapsCollectionError::SerializationError(format!(
            "failed to trace schema fields for '{}': {e}",
            std::any::type_name::<R>()
        ))
    })?;
    Ok(fields.into_iter().map(|f| f.name().to_string()).collect())
}

pub enum OvertureRecordType {
    Places,
    Buildings,
    Segment,
    Connector,
    Infrastructure,
}

impl OvertureRecordType {
    pub fn format_url(&self, release_str: &str) -> String {
        match self {
            OvertureRecordType::Places => {
                format!("release/{release_str}/theme=places/type=place/").to_owned()
            }
            OvertureRecordType::Buildings => {
                format!("release/{release_str}/theme=buildings/type=building/").to_owned()
            }
            OvertureRecordType::Segment => {
                format!("release/{release_str}/theme=transportation/type=segment/").to_owned()
            }
            OvertureRecordType::Connector => {
                format!("release/{release_str}/theme=transportation/type=connector/").to_owned()
            }
            OvertureRecordType::Infrastructure => {
                format!("release/{release_str}/theme=base/type=infrastructure/").to_owned()
            }
        }
    }

    /// processes an arrow [RecordBatch] into an [OvertureRecord] collection,
    /// deserializing into the underlying row type struct along the way.
    pub fn process_batch<R>(
        &self,
        record_batch: &RecordBatch,
    ) -> Result<Vec<OvertureRecord>, OvertureMapsCollectionError>
    where
        R: DeserializeOwned + Into<OvertureRecord>,
    {
        let as_rows: Vec<R> = self.deserialize_batch(record_batch)?;
        let as_result: Vec<OvertureRecord> = as_rows.into_iter().map(Into::into).collect();
        Ok(as_result)
    }

    /// deserializes an arrow [RecordBatch] directly into a typed `Vec<R>` without
    /// wrapping each element in [OvertureRecord].
    pub fn deserialize_batch<R>(
        &self,
        record_batch: &RecordBatch,
    ) -> Result<Vec<R>, OvertureMapsCollectionError>
    where
        R: DeserializeOwned,
    {
        serde_arrow::from_record_batch(record_batch)
            .map_err(|e| OvertureMapsCollectionError::DeserializeError(format!("Serde error: {e}")))
    }
}

impl std::fmt::Display for OvertureRecordType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Places => write!(f, "Places"),
            Self::Buildings => write!(f, "Buildings"),
            Self::Segment => write!(f, "Segment"),
            Self::Connector => write!(f, "Connector"),
            Self::Infrastructure => write!(f, "Infrastructure"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{
        BuildingsRecord, InfrastructureRecord, PlacesRecord, TransportationConnectorRecord,
        TransportationSegmentRecord,
    };

    #[test]
    fn test_projected_columns_derived_from_record_types() {
        let seg_cols = projected_columns_for_record::<TransportationSegmentRecord>().unwrap();
        assert_eq!(
            seg_cols,
            vec![
                "id",
                "geometry",
                "bbox",
                "subtype",
                "class",
                "subclass",
                "version",
                "connectors",
                "subclass_rules",
                "access_restrictions",
                "speed_limits",
            ]
        );

        let conn_cols = projected_columns_for_record::<TransportationConnectorRecord>().unwrap();
        assert_eq!(conn_cols, vec!["id", "geometry", "bbox", "version"]);

        let infra_cols = projected_columns_for_record::<InfrastructureRecord>().unwrap();
        assert_eq!(
            infra_cols,
            vec![
                "id",
                "geometry",
                "bbox",
                "version",
                "subtype",
                "class",
                "source_tags",
            ]
        );

        let place_cols = projected_columns_for_record::<PlacesRecord>().unwrap();
        assert_eq!(
            place_cols,
            vec![
                "geometry",
                "categories",
                "basic_category",
                "taxonomy",
                "operating_status",
                "id",
                "bbox",
                "version",
            ]
        );

        let bldg_cols = projected_columns_for_record::<BuildingsRecord>().unwrap();
        assert_eq!(
            bldg_cols,
            vec!["id", "geometry", "bbox", "version", "subtype", "class"]
        );
    }
}
