use crate::collection::TaxonomyModel;
use arrow::{
    array::{Array, BooleanArray, ListArray, StringArray, StructArray},
    error::ArrowError,
};
use parquet::arrow::arrow_reader::ArrowPredicate;
use std::sync::Arc;

pub struct TaxonomyRowPredicate {
    category_model: Arc<TaxonomyModel>,
    projection_mask: parquet::arrow::ProjectionMask,
}

impl TaxonomyRowPredicate {
    pub fn new(
        category_model: Arc<TaxonomyModel>,
        projection_mask: parquet::arrow::ProjectionMask,
    ) -> Self {
        Self {
            category_model,
            projection_mask,
        }
    }
}

impl ArrowPredicate for TaxonomyRowPredicate {
    fn projection(&self) -> &parquet::arrow::ProjectionMask {
        &self.projection_mask
    }

    fn evaluate(
        &mut self,
        batch: arrow::array::RecordBatch,
    ) -> Result<arrow::array::BooleanArray, arrow::error::ArrowError> {
        let relevant_categories = self.category_model.get_unique_categories();

        // Overture Places schema v2 (`2026-10-21.0`+): `taxonomy` (`primary`, `hierarchy`, `alternates`)
        // and optional top-level `basic_category`.
        if let Some(taxonomy_col) = batch.column_by_name("taxonomy") {
            let struct_array = taxonomy_col
                .as_any()
                .downcast_ref::<StructArray>()
                .ok_or_else(|| {
                    ArrowError::ParquetError(String::from(
                        "Cannot cast column `taxonomy` to StructArray type",
                    ))
                })?;

            let primary_col = struct_array
                .column_by_name("primary")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            let hierarchy_col = struct_array
                .column_by_name("hierarchy")
                .and_then(|c| c.as_any().downcast_ref::<ListArray>());

            let alternates_col = struct_array
                .column_by_name("alternates")
                .and_then(|c| c.as_any().downcast_ref::<ListArray>());

            let basic_col = batch
                .column_by_name("basic_category")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            let boolean_values: Vec<bool> = (0..batch.num_rows())
                .map(|i| {
                    if let Some(p) = primary_col {
                        if !p.is_null(i) && relevant_categories.contains(p.value(i)) {
                            return true;
                        }
                    }
                    if let Some(b) = basic_col {
                        if !b.is_null(i) && relevant_categories.contains(b.value(i)) {
                            return true;
                        }
                    }
                    if let Some(h) = hierarchy_col {
                        if !h.is_null(i) {
                            let vals = h.value(i);
                            if let Some(str_arr) = vals.as_any().downcast_ref::<StringArray>() {
                                if str_arr
                                    .into_iter()
                                    .flatten()
                                    .any(|s| relevant_categories.contains(s))
                                {
                                    return true;
                                }
                            }
                        }
                    }
                    if let Some(alt) = alternates_col {
                        if !alt.is_null(i) {
                            let vals = alt.value(i);
                            if let Some(str_arr) = vals.as_any().downcast_ref::<StringArray>() {
                                if str_arr
                                    .into_iter()
                                    .flatten()
                                    .any(|s| relevant_categories.contains(s))
                                {
                                    return true;
                                }
                            }
                        }
                    }
                    false
                })
                .collect();
            return Ok(BooleanArray::from(boolean_values));
        }

        // Legacy Overture Places schema (< `2026-10-21.0`): `categories` (`primary`, `alternate`)
        let struct_array = batch
            .column_by_name("categories")
            .ok_or(ArrowError::ParquetError(String::from(
                "Neither `taxonomy` nor `categories` column found in Overture Places batch",
            )))?
            .as_any()
            .downcast_ref::<StructArray>()
            .ok_or(ArrowError::ParquetError(String::from(
                "Cannot cast column `categories` to StructArray type",
            )))?;

        let primary_col = struct_array
            .column_by_name("primary")
            .ok_or(ArrowError::ParquetError(String::from(
                "`categories.primary` column not found",
            )))?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or(ArrowError::ParquetError(String::from(
                "Cannot cast column `categories.primary` to StringArray type",
            )))?;

        let alternate_col = struct_array
            .column_by_name("alternate")
            .ok_or(ArrowError::ParquetError(String::from(
                "`categories.alternate` column not found",
            )))?
            .as_any()
            .downcast_ref::<ListArray>()
            .ok_or(ArrowError::ParquetError(String::from(
                "Cannot cast column `categories.alternate` to StringArray type",
            )))?;

        let boolean_values: Vec<bool> = (0..struct_array.len())
            .map(|i| {
                if !primary_col.is_null(i) && relevant_categories.contains(primary_col.value(i)) {
                    return true;
                }
                if !alternate_col.is_null(i) {
                    let alternate_col_value = alternate_col.value(i);
                    if let Some(alternate_categories) =
                        alternate_col_value.as_any().downcast_ref::<StringArray>()
                    {
                        if alternate_categories
                            .into_iter()
                            .flatten()
                            .any(|category| relevant_categories.contains(category))
                        {
                            return true;
                        }
                    }
                }
                false
            })
            .collect();
        Ok(BooleanArray::from(boolean_values))
    }
}
