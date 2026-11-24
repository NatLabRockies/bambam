use arrow::array::RecordBatch;
use chrono::NaiveDate;
use futures::stream::{self, StreamExt, TryStreamExt};
use object_store::{path::Path, ListResult, ObjectMeta, ObjectStore};
use serde::de::DeserializeOwned;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::collection::collector_ops::{process_meta_obj_into_tasks, RowGroupTask};
use crate::collection::record::InfrastructureRecord;
use crate::collection::record::TransportationConnectorRecord;
use crate::collection::record::TransportationSegmentRecord;
use crate::collection::BuildingsRecord;
use crate::collection::PlacesRecord;

use super::record::{projected_columns_for_record, OvertureRecord, OvertureRecordType};
use super::OvertureMapsCollectionError;
use super::OvertureMapsCollectorConfig;
use super::RowFilter;
use super::RowFilterConfig;

/// Stores the initialized object store and allows to collect
/// records by `collect_from_release` and `collect_from_path`
#[derive(Debug)]
pub struct OvertureMapsCollector {
    obj_store: Arc<dyn ObjectStore>,
    rg_chunk_size: usize,
    file_concurrency_limit: usize,
}

impl TryFrom<OvertureMapsCollectorConfig> for OvertureMapsCollector {
    type Error = OvertureMapsCollectionError;

    fn try_from(value: OvertureMapsCollectorConfig) -> Result<Self, Self::Error> {
        value.build()
    }
}

impl OvertureMapsCollector {
    pub fn new(
        object_store: Arc<dyn ObjectStore>,
        rg_chunk_size: usize,
        file_concurrency_limit: usize,
    ) -> Self {
        Self {
            obj_store: object_store,
            rg_chunk_size,
            file_concurrency_limit,
        }
    }

    pub fn get_latest_release(&self) -> Result<String, OvertureMapsCollectionError> {
        // Get runtime to consume async functions
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                OvertureMapsCollectionError::TokioError(format!(
                    "failure creating async rust tokio runtime: {e}"
                ))
            })?;

        // Get the folder names for all releases
        let common_path = Path::from("release/");
        let ListResult {
            common_prefixes, ..
        } = runtime
            .block_on(self.obj_store.list_with_delimiter(Some(&common_path)))
            .map_err(|e| {
                OvertureMapsCollectionError::ConnectionError(format!(
                    "Could not retrieve list of folders to get latest OvertureMaps release: {e}"
                ))
            })?;

        // Process all common prefixes to find latest date
        let (_, latest) = common_prefixes
            .iter()
            .filter_map(|p| {
                let clean_str = p.to_string().strip_prefix("release/")?.to_string();

                let mut string_parts = clean_str.split(".");
                let date_part = NaiveDate::parse_from_str(string_parts.next()?, "%Y-%m-%d").ok()?;

                Some((date_part, clean_str))
            })
            .max() // compare by date then by string for tiebreaking
            .ok_or_else(|| {
                OvertureMapsCollectionError::MetadataError(String::from(
                    "latest /release not found",
                ))
            })?;

        Ok(latest)
    }

    pub fn collect_from_path(
        &self,
        path: Path,
        record_type: &OvertureRecordType,
        row_filter_config: Option<RowFilterConfig>,
    ) -> Result<Vec<OvertureRecord>, OvertureMapsCollectionError> {
        match record_type {
            OvertureRecordType::Places => self
                .collect_records_from_path_filtered::<PlacesRecord, _, _, _>(
                    path,
                    record_type,
                    row_filter_config,
                    |_| true,
                    OvertureRecord::from,
                ),
            OvertureRecordType::Buildings => self
                .collect_records_from_path_filtered::<BuildingsRecord, _, _, _>(
                    path,
                    record_type,
                    row_filter_config,
                    |_| true,
                    OvertureRecord::from,
                ),
            OvertureRecordType::Segment => self
                .collect_records_from_path_filtered::<TransportationSegmentRecord, _, _, _>(
                    path,
                    record_type,
                    row_filter_config,
                    |_| true,
                    OvertureRecord::from,
                ),
            OvertureRecordType::Connector => self
                .collect_records_from_path_filtered::<TransportationConnectorRecord, _, _, _>(
                    path,
                    record_type,
                    row_filter_config,
                    |_| true,
                    OvertureRecord::from,
                ),
            OvertureRecordType::Infrastructure => self
                .collect_records_from_path_filtered::<InfrastructureRecord, _, _, _>(
                    path,
                    record_type,
                    row_filter_config,
                    |_| true,
                    OvertureRecord::from,
                ),
        }
    }

    pub fn collect_from_release(
        &self,
        release_uri: &str,
        record_type: &OvertureRecordType,
        row_filter_config: Option<RowFilterConfig>,
    ) -> Result<Vec<OvertureRecord>, OvertureMapsCollectionError> {
        log::info!("Collecting OvertureMaps {record_type} records from release {release_uri}");
        let path = Path::from(record_type.format_url(release_uri));
        self.collect_from_path(path, record_type, row_filter_config)
    }

    /// Streams Arrow `RecordBatch`es from `release_uri`, deserializing each batch immediately
    /// into `R`, retaining only records matching `predicate`, and dropping the `RecordBatch`
    /// before reading subsequent batches so raw Arrow buffers never accumulate in RAM.
    pub fn collect_typed_from_release_filtered<R, P>(
        &self,
        release_uri: &str,
        record_type: &OvertureRecordType,
        row_filter_config: Option<RowFilterConfig>,
        predicate: P,
    ) -> Result<Vec<R>, OvertureMapsCollectionError>
    where
        R: DeserializeOwned + Send + 'static,
        P: Fn(&R) -> bool + Send + Sync + 'static,
    {
        log::info!("Collecting OvertureMaps {record_type} records from release {release_uri}");
        let path = Path::from(record_type.format_url(release_uri));
        self.collect_records_from_path_filtered(
            path,
            record_type,
            row_filter_config,
            predicate,
            std::convert::identity,
        )
    }

    /// Core streaming collection pipeline: fetches Parquet row-group streams, deserializes each
    /// `RecordBatch` on the blocking/Rayon pool as soon as it arrives, applies `predicate`
    /// in-flight, and drops the `RecordBatch` immediately.
    pub fn collect_records_from_path_filtered<R, T, P, M>(
        &self,
        path: Path,
        _record_type: &OvertureRecordType,
        row_filter_config: Option<RowFilterConfig>,
        predicate: P,
        mapper: M,
    ) -> Result<Vec<T>, OvertureMapsCollectionError>
    where
        R: DeserializeOwned + Send + 'static,
        T: Send + 'static,
        P: Fn(&R) -> bool + Send + Sync + 'static,
        M: Fn(R) -> T + Send + Sync + 'static,
    {
        let filemeta_stream = self.obj_store.list(Some(&path));

        let io_runtime = tokio::runtime::Runtime::new()
            .map_err(|e| OvertureMapsCollectionError::TokioError(e.to_string()))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                OvertureMapsCollectionError::TokioError(format!(
                    "failure creating async rust tokio runtime: {e}"
                ))
            })?;

        // Collect all metadata to create streams, synchronously
        let meta_objects = runtime
            .block_on(filemeta_stream.collect::<Vec<_>>())
            .into_iter()
            .collect::<Result<Vec<ObjectMeta>, _>>()
            .map_err(|e| OvertureMapsCollectionError::MetadataError(e.to_string()))?;

        // Prepare the filter predicates
        let opt_bbox_filter = row_filter_config
            .as_ref()
            .and_then(|f| f.get_bbox_filter_if_exists());

        // validate provided bbox
        if let Some(bbox) = opt_bbox_filter.as_ref() {
            bbox.validate()?
        };

        // build rest of the filters
        let row_filter = if let Some(row_filter_config) = &row_filter_config {
            row_filter_config.validate_unique_variant()?;
            Some(RowFilter::try_from(row_filter_config.clone())?)
        } else {
            None
        };

        // Derive Parquet column projection directly from the target Rust record type `R`
        let projected_cols = projected_columns_for_record::<R>()?;
        log::info!(
            "Started collection for {} (projecting {} columns: {:?})",
            std::any::type_name::<R>(),
            projected_cols.len(),
            projected_cols
        );
        let start_collection = Instant::now();
        // Process each metadata object into a flat vector of tasks that
        // each take a small number of row_groups. Inside `process_meta_obj_into_tasks`
        // we also prune based on the bounding box.
        let row_group_tasks: Vec<RowGroupTask> = runtime.block_on(async {
            Ok::<Vec<RowGroupTask>, OvertureMapsCollectionError>(
                stream::iter(meta_objects)
                    .map(|meta| {
                        process_meta_obj_into_tasks(
                            meta,
                            self.obj_store.clone(),
                            Some(io_runtime.handle().clone()),
                            opt_bbox_filter,
                            Some(self.rg_chunk_size),
                        )
                    })
                    .buffer_unordered(self.file_concurrency_limit)
                    .try_collect::<Vec<Vec<RowGroupTask>>>()
                    .await?
                    .into_iter()
                    .flatten()
                    .collect(),
            )
        })?;

        // Build row-group streams with column projection
        let streams = row_group_tasks
            .into_iter()
            .map(|rgt| {
                rgt.build_stream(
                    row_filter.as_ref(),
                    Some(projected_cols.as_slice()),
                    self.obj_store.clone(),
                    io_runtime.handle().clone(),
                )
            })
            .collect::<Result<Vec<_>, OvertureMapsCollectionError>>()?;

        let predicate = Arc::new(predicate);
        let mapper = Arc::new(mapper);
        let io_handle = io_runtime.handle().clone();
        let deser_concurrency = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(16)
            .max(1);

        // Stream batches with bounded concurrency, deserializing and filtering each batch
        // immediately so `RecordBatch` buffers are freed as soon as they are processed.
        let (records, total_deser_time) = runtime.block_on(async {
            let mut batch_result_stream = stream::iter(streams)
                .flatten_unordered(self.file_concurrency_limit)
                .map_err(|e| OvertureMapsCollectionError::RecordBatchRetrievalError { source: e })
                .map_ok(|batch: RecordBatch| {
                    let pred = Arc::clone(&predicate);
                    let map_fn = Arc::clone(&mapper);
                    let handle = io_handle.clone();
                    async move {
                        handle
                            .spawn_blocking(move || {
                                let t0 = Instant::now();
                                let rows: Vec<R> =
                                    serde_arrow::from_record_batch(&batch).map_err(|e| {
                                        OvertureMapsCollectionError::DeserializeError(format!(
                                            "Serde error: {e}"
                                        ))
                                    })?;
                                // Drop `batch` immediately to release Arrow buffers before filtering/mapping
                                drop(batch);
                                let kept: Vec<T> = rows
                                    .into_iter()
                                    .filter(|r| pred(r))
                                    .map(|r| map_fn(r))
                                    .collect();
                                Ok::<(Vec<T>, Duration), OvertureMapsCollectionError>((
                                    kept,
                                    t0.elapsed(),
                                ))
                            })
                            .await
                            .map_err(|e| {
                                OvertureMapsCollectionError::TokioError(format!(
                                    "blocking deserialization task failed: {e}"
                                ))
                            })?
                    }
                })
                .try_buffer_unordered(deser_concurrency);

            let mut out: Vec<T> = Vec::new();
            let mut deser_elapsed = Duration::ZERO;
            while let Some((mut batch_records, dt)) = batch_result_stream.try_next().await? {
                out.append(&mut batch_records);
                deser_elapsed += dt;
            }
            Ok::<(Vec<T>, Duration), OvertureMapsCollectionError>((out, deser_elapsed))
        })?;

        log::info!(
            "Streaming collection + deserialization completed in {:?} (cumulative CPU deserialization time {:?}, retained {} records)",
            start_collection.elapsed(),
            total_deser_time,
            records.len()
        );
        Ok(records)
    }
}

#[cfg(test)]
mod test {
    use crate::collection::{
        ObjectStoreSource, OvertureMapsCollector, OvertureMapsCollectorConfig, OvertureRecord,
        OvertureRecordType, RowFilterConfig,
    };

    fn get_collector() -> OvertureMapsCollector {
        OvertureMapsCollectorConfig::new(ObjectStoreSource::AmazonS3, Some(4), Some(64))
            .build()
            .unwrap()
    }

    #[test]
    #[ignore]
    fn test_deserialization() {
        let collector = get_collector();

        // Roughly Golden, CO
        let row_filter = RowFilterConfig::Bbox {
            xmin: -105.254,
            xmax: -105.197,
            ymin: 39.733,
            ymax: 39.784,
        };

        // this retrieval and external depenency seems too brittle for a unit test
        let latest_release = collector
            .get_latest_release()
            .expect("failed to retrieve latest OMF release");

        // Connectors
        let connector_records = collector
            .collect_from_release(
                &latest_release,
                &OvertureRecordType::Connector,
                Some(row_filter.clone()),
            )
            .unwrap();

        println!("Records Length: {}", connector_records.len());

        assert_eq!(connector_records.len(), 6436);
        assert!(matches!(
            connector_records[0],
            OvertureRecord::Connector(..)
        ));

        // Segment
        let segment_records = collector
            .collect_from_release(
                &latest_release,
                &OvertureRecordType::Segment,
                Some(row_filter),
            )
            .unwrap();

        println!("Records Length: {}", segment_records.len());

        assert_eq!(segment_records.len(), 3771);
        assert!(matches!(segment_records[0], OvertureRecord::Segment(..)));
    }
}
