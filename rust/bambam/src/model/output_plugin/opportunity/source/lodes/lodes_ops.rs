use crate::model::output_plugin::opportunity::{
    opportunity_source::OpportunitySource, study_region::StudyRegion, OpportunityDataset,
};
use bamcensus::app::lodes_tiger;
use bamcensus_core::model::identifier::{Geoid, GeoidType};
use bamcensus_lehd::model::{
    LodesDataset, LodesEdition, LodesJobType, WacSegment, WorkplaceSegment,
};
use geo::{Geometry, MapCoords};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// downloads LODES workplace statistics and applies a WAC segment to activity
/// type mapping.
///
/// # Returns
///
/// the collection of activities grouped by zone at the geoids or aggregated
/// to the specified data granularity.
pub fn collect_lodes_opportunities(
    dataset: &LodesDataset,
    segments: &[WacSegment],
    geoids: &Vec<Geoid>,
    data_granularity: &Option<GeoidType>,
    activity_types: &[String],
    activity_mapping: &HashMap<WacSegment, Vec<String>>,
) -> Result<OpportunityDataset, String> {
    // download LODES data paired with TIGER/Lines geometries
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failure creating async rust tokio runtime: {e}"))?;
    let future = lodes_tiger::run(geoids, data_granularity, segments, dataset);
    let res = runtime
        .block_on(future)
        .map_err(|e| format!("failure downloading LODES data: {e}"))?;
    if !res.join_errors.is_empty() || !res.tiger_errors.is_empty() {
        let msg = format!("failures downloading LODES data.\nTIGER ERRORS (top 5):\n  {}\nJOIN ERRORS (top 5):\n  {}",
            res.tiger_errors.iter().take(5).join("\n  "),
            res.join_errors.iter().take(5).join("\n  ")
        );
        return Err(msg);
    }

    // group opportunities by geometry in f32 precision.
    let chunk_iter = res
        .join_dataset
        .into_iter()
        .map(|r| {
            let g = r.geometry.map_coords(|c| geo::Coord {
                x: c.x as f32,
                y: c.y as f32,
            });
            (g, r)
        })
        .chunk_by(|(g, _)| g.clone());

    // lookup for a MEP activity's slot in the opportunity vector
    let idx_lookup: HashMap<String, usize> = HashMap::from_iter(
        activity_types
            .iter()
            .enumerate()
            .map(|(i, a)| (a.clone(), i)),
    );

    // map wac segment to MEP activity type via the activity_mapping.
    // sum together all counts by MEP activity type (dataset from long to wide format).
    let mut result = vec![];
    for (geometry, grouped) in &chunk_iter {
        let mut out_row = vec![0.0; activity_types.len()];
        for (_, row) in grouped {
            // this row's WAC segment may apply to multiple MEP categories. the convention is,
            // any mapping category is a "job" activity, and, possibly another activity.
            let mapped_acts = activity_mapping.get(&row.value.segment).ok_or_else(|| {
                format!(
                    "LODES WAC segment {} missing from activity mapping",
                    row.value.segment
                )
            })?;

            for mapped_act in mapped_acts.iter() {
                let index = idx_lookup.get(mapped_act).ok_or_else(|| {
                    format!("activity type {mapped_act} missing from expected activity types")
                })?;
                out_row[*index] += row.value.value;
            }
        }
        result.push((geometry, out_row));
    }
    Ok(result)
}
