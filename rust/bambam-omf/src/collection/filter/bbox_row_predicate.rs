use super::Bbox;
use arrow::{
    array::{Array, BooleanArray, Float64Array, StructArray},
    error::ArrowError,
};
use parquet::arrow::arrow_reader::ArrowPredicate;

/// tests if a row's bounding box intersects with a query bounding box.
pub struct BboxRowPredicate {
    bbox: Bbox,
    projection_mask: parquet::arrow::ProjectionMask,
}

impl BboxRowPredicate {
    pub fn new(bbox: Bbox, projection_mask: parquet::arrow::ProjectionMask) -> Self {
        Self {
            bbox,
            projection_mask,
        }
    }
}

impl ArrowPredicate for BboxRowPredicate {
    fn projection(&self) -> &parquet::arrow::ProjectionMask {
        &self.projection_mask
    }

    /// tests the bounding box of each row in the record batch, filtering entries
    /// that do not intersect the query bounding box.
    fn evaluate(
        &mut self,
        batch: arrow::array::RecordBatch,
    ) -> Result<arrow::array::BooleanArray, arrow::error::ArrowError> {
        let bbox_struct = batch
            .column_by_name("bbox")
            .ok_or(ArrowError::ParquetError(String::from(
                "`bbox` column not found",
            )))?
            .as_any()
            .downcast_ref::<StructArray>()
            .ok_or(ArrowError::ParquetError(String::from(
                "Cannot cast column `bbox` to StructArray type",
            )))?;

        let xmins = get_column::<Float64Array>("xmin", bbox_struct)?;
        let ymins = get_column::<Float64Array>("ymin", bbox_struct)?;
        let xmaxs = get_column::<Float64Array>("xmax", bbox_struct)?;
        let ymaxs = get_column::<Float64Array>("ymax", bbox_struct)?;

        let boolean_values: Vec<bool> = (0..bbox_struct.len())
            .map(|i| intersects_box(i, xmins, ymins, xmaxs, ymaxs, &self.bbox))
            .collect();
        Ok(BooleanArray::from(boolean_values))
    }
}

/// helper function to get a column by name from a struct array and return it as
/// the expected type.
fn get_column<'b, T>(col: &str, struct_array: &'b StructArray) -> Result<&'b T, ArrowError>
where
    T: 'static,
{
    struct_array
        .column_by_name(col)
        .ok_or(ArrowError::ParquetError(format!(
            "'bbox.{col}' column not found"
        )))?
        .as_any()
        .downcast_ref::<T>()
        .ok_or(ArrowError::ParquetError(format!(
            "Cannot cast column 'bbox.{col}' to expected type"
        )))
}

/// helper function to test whether a given row's bounding box intersects the query bounding box.
fn intersects_box(
    index: usize,
    xmins: &Float64Array,
    ymins: &Float64Array,
    xmaxs: &Float64Array,
    ymaxs: &Float64Array,
    bbox: &Bbox,
) -> bool {
    let bbox_xmin = bbox.xmin as f64;
    let bbox_xmax = bbox.xmax as f64;
    let bbox_ymin = bbox.ymin as f64;
    let bbox_ymax = bbox.ymax as f64;

    bbox_xmin <= xmaxs.value(index)
        && xmins.value(index) <= bbox_xmax
        && bbox_ymin <= ymaxs.value(index)
        && ymins.value(index) <= bbox_ymax
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arrow::{
        array::{Float64Array, RecordBatch, StructArray},
        datatypes::{DataType, Field, Fields, Schema},
    };
    use parquet::arrow::{arrow_reader::ArrowPredicate, ProjectionMask};

    use super::{Bbox, BboxRowPredicate};

    fn make_bbox_batch(rows: &[(f64, f64, f64, f64)]) -> RecordBatch {
        let xmins = Float64Array::from(rows.iter().map(|r| r.0).collect::<Vec<_>>());
        let xmaxs = Float64Array::from(rows.iter().map(|r| r.1).collect::<Vec<_>>());
        let ymins = Float64Array::from(rows.iter().map(|r| r.2).collect::<Vec<_>>());
        let ymaxs = Float64Array::from(rows.iter().map(|r| r.3).collect::<Vec<_>>());

        let fields = Fields::from(vec![
            Field::new("xmin", DataType::Float64, false),
            Field::new("xmax", DataType::Float64, false),
            Field::new("ymin", DataType::Float64, false),
            Field::new("ymax", DataType::Float64, false),
        ]);
        let bbox_struct = StructArray::new(
            fields.clone(),
            vec![
                Arc::new(xmins),
                Arc::new(xmaxs),
                Arc::new(ymins),
                Arc::new(ymaxs),
            ],
            None,
        );
        let schema = Arc::new(Schema::new(vec![Field::new(
            "bbox",
            DataType::Struct(fields),
            false,
        )]));
        RecordBatch::try_new(schema, vec![Arc::new(bbox_struct)]).unwrap()
    }

    #[test]
    fn test_bbox_row_predicate_intersects() {
        let query_bbox = Bbox::new(0.0, 10.0, 0.0, 10.0);
        let batch = make_bbox_batch(&[
            // Fully contained inside query bbox
            (2.0, 4.0, 2.0, 4.0),
            // Crossing the right/top boundary (partially inside, not fully contained)
            (8.0, 12.0, 8.0, 12.0),
            // Enclosing the query bbox completely
            (-5.0, 15.0, -5.0, 15.0),
            // Touching edge
            (10.0, 12.0, 0.0, 5.0),
            // Completely disjoint (to the right)
            (11.0, 15.0, 2.0, 4.0),
            // Completely disjoint (below)
            (2.0, 4.0, -5.0, -1.0),
        ]);

        let mut predicate = BboxRowPredicate::new(query_bbox, ProjectionMask::all());
        let result = predicate.evaluate(batch).unwrap();

        let expected: Vec<bool> = vec![true, true, true, true, false, false];
        let actual: Vec<bool> = (0..result.len()).map(|i| result.value(i)).collect();
        assert_eq!(actual, expected);
    }
}
