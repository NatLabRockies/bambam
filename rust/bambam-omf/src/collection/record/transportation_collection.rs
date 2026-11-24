use std::{collections::HashSet, fs::File, path::Path, sync::Arc};

use arrow::datatypes::{FieldRef, Schema};
use parquet::{
    arrow::{arrow_reader::ParquetRecordBatchReaderBuilder, ArrowWriter},
    basic::Compression,
    file::{metadata::KeyValue, properties::WriterProperties},
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_arrow::schema::SchemaLike;

use crate::collection::{
    record::record_schema_tracing_options, InfrastructureRecord, OvertureMapsCollectionError,
    OvertureMapsCollector, OvertureRecordType, OvertureSpatialFilter, ReleaseVersion,
    RowFilterConfig, TransportationConnectorRecord, TransportationSegmentRecord,
};

pub const RAW_DIRNAME: &str = "raw";
pub const RAW_SEGMENTS_PARQUET: &str = "segments.parquet";
pub const RAW_CONNECTORS_PARQUET: &str = "connectors.parquet";
pub const RAW_INFRASTRUCTURE_PARQUET: &str = "infrastructure.parquet";
pub const PARQUET_RELEASE_METADATA_KEY: &str = "overture_release";
pub const RAW_PARQUET_WRITE_CHUNK_SIZE: usize = 200_000;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TransportationCollection {
    pub release: String,
    pub connectors: Vec<TransportationConnectorRecord>,
    pub segments: Vec<TransportationSegmentRecord>,
    #[serde(default)]
    pub infrastructure: Vec<InfrastructureRecord>,
}

impl TransportationCollection {
    /// Use a pre-built collector and download configuration to
    /// retrieve connectors, segments, and transportation infrastructure for a specified query.
    pub fn try_from_collector(
        collector: OvertureMapsCollector,
        release: ReleaseVersion,
        row_filter_config: Option<RowFilterConfig>,
    ) -> Result<Self, OvertureMapsCollectionError> {
        Self::try_from_collector_with_spatial_filter(collector, release, row_filter_config, None)
    }

    /// Streams segments, connectors, and infrastructure from `collector`, applying the optional
    /// Stage 2 [`OvertureSpatialFilter`] (and connector-ID pruning) in-flight as each `RecordBatch`
    /// is deserialized so out-of-extent records and unreferenced connectors never accumulate in RAM.
    pub fn try_from_collector_with_spatial_filter(
        collector: OvertureMapsCollector,
        release: ReleaseVersion,
        row_filter_config: Option<RowFilterConfig>,
        spatial_filter: Option<OvertureSpatialFilter>,
    ) -> Result<Self, OvertureMapsCollectionError> {
        let uri = match &release {
            ReleaseVersion::Latest => collector.get_latest_release()?,
            other => String::from(other),
        };

        let seg_spatial = spatial_filter.clone();
        let segments: Vec<TransportationSegmentRecord> = collector
            .collect_typed_from_release_filtered(
                &uri,
                &OvertureRecordType::Segment,
                row_filter_config.clone(),
                move |seg: &TransportationSegmentRecord| match &seg_spatial {
                    Some(sf) => seg
                        .get_linestring()
                        .map(|ls| sf.matches_linestring(ls))
                        .unwrap_or(false),
                    None => true,
                },
            )?;

        // When a spatial filter is active, retain only connectors referenced by surviving segments
        let referenced_connector_ids: Option<Arc<HashSet<String>>> =
            spatial_filter.as_ref().map(|_| {
                let ids: HashSet<String> = segments
                    .iter()
                    .filter_map(|s| s.connectors.as_ref())
                    .flatten()
                    .map(|c| c.connector_id.clone())
                    .collect();
                Arc::new(ids)
            });

        let connectors: Vec<TransportationConnectorRecord> = collector
            .collect_typed_from_release_filtered(
                &uri,
                &OvertureRecordType::Connector,
                row_filter_config.clone(),
                move |conn: &TransportationConnectorRecord| match &referenced_connector_ids {
                    Some(ids) => ids.contains(&conn.id),
                    None => true,
                },
            )?;

        let infra_filter_config = build_infrastructure_row_filter(row_filter_config);
        let infra_spatial = spatial_filter;
        let infrastructure: Vec<InfrastructureRecord> = collector
            .collect_typed_from_release_filtered(
                &uri,
                &OvertureRecordType::Infrastructure,
                infra_filter_config,
                move |infra: &InfrastructureRecord| match &infra_spatial {
                    Some(sf) => infra
                        .get_geometry()
                        .map(|g| sf.matches_geometry(g))
                        .unwrap_or(false),
                    None => true,
                },
            )?;

        Ok(Self {
            release: uri,
            connectors,
            segments,
            infrastructure,
        })
    }

    /// Write this collection to disk as Parquet files in `<output_directory>/raw/`
    /// (`segments.parquet`, `connectors.parquet`, `infrastructure.parquet`).
    pub fn to_parquet(&self, output_directory: &Path) -> Result<(), OvertureMapsCollectionError> {
        let raw_dir = output_directory.join(RAW_DIRNAME);
        crate::util::fs::create_dirs(&raw_dir)?;

        write_records_to_parquet(
            &self.segments,
            &raw_dir.join(RAW_SEGMENTS_PARQUET),
            &self.release,
        )?;
        write_records_to_parquet(
            &self.connectors,
            &raw_dir.join(RAW_CONNECTORS_PARQUET),
            &self.release,
        )?;
        write_records_to_parquet(
            &self.infrastructure,
            &raw_dir.join(RAW_INFRASTRUCTURE_PARQUET),
            &self.release,
        )?;

        Ok(())
    }

    /// Read a `TransportationCollection` from a directory containing Parquet files
    /// (either `<path>/segments.parquet` or `<path>/raw/segments.parquet`).
    pub fn from_parquet_dir(path: &Path) -> Result<Self, OvertureMapsCollectionError> {
        let raw_dir = if path.join(RAW_SEGMENTS_PARQUET).exists() {
            path.to_path_buf()
        } else if path.join(RAW_DIRNAME).join(RAW_SEGMENTS_PARQUET).exists() {
            path.join(RAW_DIRNAME)
        } else {
            return Err(OvertureMapsCollectionError::ReadError {
                path: path.to_path_buf(),
                message: format!(
                    "expected '{RAW_SEGMENTS_PARQUET}' in '{}' or '{}/{RAW_DIRNAME}'",
                    path.display(),
                    path.display()
                ),
            });
        };

        let (segments, seg_release) = read_records_from_parquet::<TransportationSegmentRecord>(
            &raw_dir.join(RAW_SEGMENTS_PARQUET),
        )?;
        let (connectors, conn_release) = read_records_from_parquet::<TransportationConnectorRecord>(
            &raw_dir.join(RAW_CONNECTORS_PARQUET),
        )?;

        let infra_path = raw_dir.join(RAW_INFRASTRUCTURE_PARQUET);
        let (infrastructure, infra_release) = if infra_path.exists() {
            read_records_from_parquet::<InfrastructureRecord>(&infra_path)?
        } else {
            (vec![], None)
        };

        let release = seg_release
            .or(conn_release)
            .or(infra_release)
            .unwrap_or_else(|| format!("file://{}", raw_dir.to_string_lossy()));

        Ok(Self {
            release,
            connectors,
            segments,
            infrastructure,
        })
    }

    /// Load a `TransportationCollection` from a local source path (`raw/` Parquet directory or `.parquet` file).
    pub fn from_local_source(path: &Path) -> Result<Self, OvertureMapsCollectionError> {
        if path.is_dir() {
            return Self::from_parquet_dir(path);
        }
        if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("parquet"))
        {
            let parent = path.parent().unwrap_or_else(|| Path::new("."));
            return Self::from_parquet_dir(parent);
        }

        Err(OvertureMapsCollectionError::ReadError {
            path: path.to_owned(),
            message: "expected a directory containing raw Parquet files or a .parquet file"
                .to_string(),
        })
    }
}

fn build_infrastructure_row_filter(
    base_filter: Option<RowFilterConfig>,
) -> Option<RowFilterConfig> {
    let class_filter = RowFilterConfig::HasClassIn {
        classes: InfrastructureRecord::default_transportation_classes(),
    };
    match base_filter {
        None => Some(class_filter),
        Some(RowFilterConfig::HasClassIn { classes }) => {
            Some(RowFilterConfig::HasClassIn { classes })
        }
        Some(RowFilterConfig::Combined { mut filters }) => {
            let has_class_filter = filters.iter().any(|f| {
                matches!(
                    f.as_ref(),
                    RowFilterConfig::HasClass | RowFilterConfig::HasClassIn { .. }
                )
            });
            if !has_class_filter {
                filters.push(Box::new(class_filter));
            }
            Some(RowFilterConfig::Combined { filters })
        }
        Some(other) => Some(RowFilterConfig::Combined {
            filters: vec![Box::new(other), Box::new(class_filter)],
        }),
    }
}

fn write_records_to_parquet<T>(
    records: &[T],
    filepath: &Path,
    release: &str,
) -> Result<(), OvertureMapsCollectionError>
where
    T: Serialize + DeserializeOwned,
{
    let fields = Vec::<FieldRef>::from_type::<T>(record_schema_tracing_options()).map_err(|e| {
        OvertureMapsCollectionError::SerializationError(format!(
            "failed to trace Arrow schema for '{}': {e}",
            filepath.display()
        ))
    })?;
    let schema = Arc::new(Schema::new(fields.clone()));

    let file = File::create(filepath).map_err(|e| OvertureMapsCollectionError::WriteError {
        path: filepath.to_path_buf(),
        message: e.to_string(),
    })?;

    let props = WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .set_key_value_metadata(Some(vec![KeyValue::new(
            PARQUET_RELEASE_METADATA_KEY.to_string(),
            Some(release.to_string()),
        )]))
        .build();

    let mut writer = ArrowWriter::try_new(file, schema, Some(props)).map_err(|e| {
        OvertureMapsCollectionError::WriteError {
            path: filepath.to_path_buf(),
            message: format!("failed to initialize Parquet writer: {e}"),
        }
    })?;

    if records.is_empty() {
        let empty_batch = serde_arrow::to_record_batch::<&[T]>(&fields, &records).map_err(|e| {
            OvertureMapsCollectionError::SerializationError(format!(
                "failed to serialize empty Arrow RecordBatch for '{}': {e}",
                filepath.display()
            ))
        })?;
        writer.write(&empty_batch).map_err(|e| {
            OvertureMapsCollectionError::WriteError {
                path: filepath.to_path_buf(),
                message: format!("failed to write empty Parquet batch: {e}"),
            }
        })?;
    } else {
        for chunk in records.chunks(RAW_PARQUET_WRITE_CHUNK_SIZE) {
            let batch = serde_arrow::to_record_batch::<&[T]>(&fields, &chunk).map_err(|e| {
                OvertureMapsCollectionError::SerializationError(format!(
                    "failed to serialize records into Arrow RecordBatch for '{}': {e}",
                    filepath.display()
                ))
            })?;
            writer
                .write(&batch)
                .map_err(|e| OvertureMapsCollectionError::WriteError {
                    path: filepath.to_path_buf(),
                    message: format!("failed to write Parquet batch: {e}"),
                })?;
        }
    }

    writer
        .close()
        .map_err(|e| OvertureMapsCollectionError::WriteError {
            path: filepath.to_path_buf(),
            message: format!("failed to close Parquet writer: {e}"),
        })?;

    Ok(())
}

fn read_records_from_parquet<T>(
    filepath: &Path,
) -> Result<(Vec<T>, Option<String>), OvertureMapsCollectionError>
where
    T: DeserializeOwned,
{
    let file = File::open(filepath).map_err(|e| OvertureMapsCollectionError::ReadError {
        path: filepath.to_path_buf(),
        message: e.to_string(),
    })?;

    let builder = ParquetRecordBatchReaderBuilder::try_new(file).map_err(|e| {
        OvertureMapsCollectionError::ReadError {
            path: filepath.to_path_buf(),
            message: format!("failed to open Parquet reader: {e}"),
        }
    })?;

    let release = builder
        .metadata()
        .file_metadata()
        .key_value_metadata()
        .and_then(|kvs| {
            kvs.iter()
                .find(|kv| kv.key == PARQUET_RELEASE_METADATA_KEY)
                .and_then(|kv| kv.value.clone())
        });

    let reader = builder
        .build()
        .map_err(|e| OvertureMapsCollectionError::ReadError {
            path: filepath.to_path_buf(),
            message: format!("failed to build Parquet RecordBatch reader: {e}"),
        })?;

    let mut all_records = Vec::new();
    for batch_result in reader {
        let batch = batch_result.map_err(|e| OvertureMapsCollectionError::ReadError {
            path: filepath.to_path_buf(),
            message: format!("failed to read RecordBatch from Parquet: {e}"),
        })?;
        let mut rows: Vec<T> = serde_arrow::from_record_batch(&batch).map_err(|e| {
            OvertureMapsCollectionError::DeserializeError(format!(
                "failed to deserialize Parquet RecordBatch from '{}': {e}",
                filepath.display()
            ))
        })?;
        all_records.append(&mut rows);
    }

    Ok((all_records, release))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::record::{
        segment::{ConnectorReference, SpeedLimitWithUnit},
        SegmentClass, SegmentSpeedLimit, SegmentSpeedUnit, SegmentSubtype,
    };
    use geo::{Geometry, LineString, Point};
    use std::collections::HashMap;

    #[test]
    fn test_parquet_raw_roundtrip_and_pipeline_outputs() {
        let tmp_dir =
            std::env::temp_dir().join(format!("bambam_omf_phase1_test_{}", std::process::id()));
        if tmp_dir.exists() {
            let _ = std::fs::remove_dir_all(&tmp_dir);
        }
        std::fs::create_dir_all(&tmp_dir).unwrap();

        let c0 = TransportationConnectorRecord {
            id: "conn-0".to_string(),
            geometry: Some(Geometry::Point(Point::new(-105.2705, 40.0150))),
            version: 1,
            ..Default::default()
        };
        let c1 = TransportationConnectorRecord {
            id: "conn-1".to_string(),
            geometry: Some(Geometry::Point(Point::new(-105.2695, 40.0150))),
            version: 1,
            ..Default::default()
        };

        let seg = TransportationSegmentRecord {
            id: "seg-0".to_string(),
            geometry: Some(Geometry::LineString(LineString::from(vec![
                (-105.2705, 40.0150),
                (-105.2695, 40.0150),
            ]))),
            subtype: Some(SegmentSubtype::Road),
            class: Some(SegmentClass::Residential),
            version: 1,
            connectors: Some(vec![
                ConnectorReference {
                    connector_id: "conn-0".to_string(),
                    at: 0.0,
                },
                ConnectorReference {
                    connector_id: "conn-1".to_string(),
                    at: 1.0,
                },
            ]),
            speed_limits: Some(vec![SegmentSpeedLimit {
                min_speed: None,
                max_speed: Some(SpeedLimitWithUnit {
                    value: 25,
                    unit: SegmentSpeedUnit::Mph,
                }),
                is_max_speed_variable: Some(false),
                when: None,
                between: None,
            }]),
            ..Default::default()
        };

        let mut source_tags = HashMap::new();
        source_tags.insert("highway".to_string(), Some("traffic_signals".to_string()));
        let infra = InfrastructureRecord {
            id: "infra-0".to_string(),
            geometry: Some(Geometry::Point(Point::new(-105.27048, 40.0150))),
            subtype: Some("transportation".to_string()),
            class: Some("traffic_signals".to_string()),
            source_tags: Some(source_tags),
            ..Default::default()
        };

        let collection = TransportationCollection {
            release: "2026-03-18.0".to_string(),
            connectors: vec![c0, c1],
            segments: vec![seg],
            infrastructure: vec![infra],
        };

        // 1. Test Parquet round-trip
        collection.to_parquet(&tmp_dir).unwrap();
        assert!(tmp_dir.join("raw").join("segments.parquet").exists());
        assert!(tmp_dir.join("raw").join("connectors.parquet").exists());
        assert!(tmp_dir.join("raw").join("infrastructure.parquet").exists());

        let loaded = TransportationCollection::from_local_source(&tmp_dir).unwrap();
        assert_eq!(loaded.release, "2026-03-18.0");
        assert_eq!(loaded.connectors.len(), 2);
        assert_eq!(loaded.segments.len(), 1);
        assert_eq!(loaded.infrastructure.len(), 1);
        assert!(loaded.infrastructure[0].is_traffic_signals());

        // 2. Test full network import pipeline outputs from local Parquet source
        let out_dir = tmp_dir.join("compass_out");
        let modes = vec![crate::app::network::NetworkEdgeListConfiguration {
            mode: "drive".to_string(),
            filter: vec![],
            island_algorithm_config: None,
        }];
        crate::app::network::run(None, &modes, &out_dir, Some(&tmp_dir), None, Some(15.0)).unwrap();

        // Raw Parquet files must be retained in out_dir/raw
        assert!(out_dir.join("raw").join("segments.parquet").exists());
        assert!(out_dir.join("raw").join("connectors.parquet").exists());
        assert!(out_dir.join("raw").join("infrastructure.parquet").exists());

        // Standardized mappings and vertex infrastructure outputs must exist
        assert!(out_dir.join("vertices-compass.csv.gz").exists());
        assert!(out_dir.join("vertices-complete.csv.gz").exists());
        assert!(out_dir.join("vertices-mapping.csv.gz").exists());
        assert!(out_dir
            .join("vertices-traffic-signals-enumerated.txt.gz")
            .exists());
        assert!(out_dir.join("vertices-stop-enumerated.txt.gz").exists());
        assert!(out_dir.join("vertices-give-way-enumerated.txt.gz").exists());
        assert!(out_dir.join("vertices-crossing-enumerated.txt.gz").exists());
        assert!(out_dir.join("drive").join("edges-compass.csv.gz").exists());
        assert!(out_dir.join("drive").join("edges-mapping.csv.gz").exists());

        // Read back vertices-complete.csv.gz and edges-mapping.csv.gz using routee_compass_core
        let v_complete: Box<[crate::graph::VertexCompleteRecord]> =
            routee_compass_core::util::fs::read_utils::from_csv(
                &out_dir.join("vertices-complete.csv.gz"),
                true,
                None,
                None,
            )
            .unwrap();
        assert_eq!(v_complete.len(), 2);
        assert!(v_complete[0].traffic_signals);
        assert!(!v_complete[1].traffic_signals);

        let e_mappings: Box<[crate::graph::EdgeMappingRecord]> =
            routee_compass_core::util::fs::read_utils::from_csv(
                &out_dir.join("drive").join("edges-mapping.csv.gz"),
                true,
                None,
                None,
            )
            .unwrap();
        assert_eq!(e_mappings.len(), 2); // forward and backward splits
        assert_eq!(e_mappings[0].source_id, "seg-0");
        assert_eq!(e_mappings[0].start_lr, 0.0);
        assert_eq!(e_mappings[0].end_lr, 1.0);

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
