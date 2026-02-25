use super::{extent_format::ExtentFormat, grid_type::GridType};
use crate::model::input_plugin::population::population_source::PopulationSource;
use bambam_core::util::polygonal_rtree::PolygonalRTree;
use geo::{Area, Geometry};
use kdam::{Bar, BarExt};
use rayon::prelude::*;
use routee_compass::{
    app::search::SearchApp,
    plugin::input::{InputPlugin, InputPluginError},
};
use routee_compass_core::config::{CompassConfigurationError, ConfigJsonExtensions};
use serde_json::json;
use std::{
    collections::LinkedList,
    sync::{Arc, Mutex},
};
use wkt::TryFromWkt;

pub struct GridInputPlugin {
    pub population_source: Option<PopulationSource>,
    pub extent_format: ExtentFormat,
    pub grid_type: GridType,
}

impl GridInputPlugin {
    pub fn new(
        population_source: Option<PopulationSource>,
        extent_format: ExtentFormat,
        grid_type: GridType,
    ) -> GridInputPlugin {
        GridInputPlugin {
            population_source,
            extent_format,
            grid_type,
        }
    }
}

//process like for InputPlugin below but without the SearchApp parameter
pub fn process_grid_input(
    input: &mut serde_json::Value,
    extent_format: ExtentFormat,
    grid_type: GridType,
    population_source: &Option<PopulationSource>,
) -> Result<(), InputPluginError> {
    // check for correct and unambiguous fields on input
    validate_query(input)?;
    if input.get(super::EXTENT).is_none() {
        // no grid requested
        return Ok(());
    }

    // allow for user override of extent format and grid type.
    let extent_format: ExtentFormat = input
        .get_config_serde_optional(&super::EXTENT_FORMAT, &"")
        .map_err(|e| InputPluginError::InputPluginFailed(format!("failure reading extent: {e}")))?
        .unwrap_or(extent_format);
    let grid_type: GridType = input
        .get_config_serde_optional(&super::GRID_TYPE, &"")
        .map_err(|e| {
            InputPluginError::InputPluginFailed(format!("failure reading grid type: {e}"))
        })?
        .unwrap_or(grid_type);

    // load the geographical extent
    let extent = extent_format
        .get_extent(input)
        .map_err(InputPluginError::InputPluginFailed)?;

    // create a template for the Compass JSON queries from whatever was in the input JSON, minus
    // the arguments used here for building a grid.
    let mut template = input.clone();
    let output_map = template.as_object_mut().ok_or_else(|| {
        let msg = String::from(
            "internal error, cannot build template from user input that is not JSON mappable",
        );
        InputPluginError::InputPluginFailed(msg)
    })?;
    let _ = output_map.remove(super::EXTENT);
    let _ = output_map.remove(super::EXTENT_FORMAT);
    let _ = output_map.remove(super::GRID_TYPE);

    // build the grid using the extent and template
    let mut grid_queries: Vec<serde_json::Value> = grid_type
        .create_grid(&extent, &template.clone())
        .map_err(InputPluginError::InputPluginFailed)?;
    eprintln!(
        "finished creating {} grid with {} cells",
        grid_type,
        grid_queries.len()
    );

    if let Some(population_source) = population_source {
        eprintln!("adding population source");
        add_population_source(&mut grid_queries, &extent, population_source)?;
    }

    let mut replacement = serde_json::json![grid_queries];
    std::mem::swap(&mut replacement, input);
    Ok(())
}

impl InputPlugin for GridInputPlugin {
    /// process the user input to a MEP query into a grid.
    /// the user is expected to provide an extent, a grid_type, and an optional extent_format (assumed WKT).
    /// the grid is built over the extent using the grid_type chosen.
    /// any extra keys provided by user are copied into each resulting grid cell (for example, a batch identifier).   
    fn process(
        &self,
        input: &mut serde_json::Value,
        _: Arc<SearchApp>,
    ) -> Result<(), InputPluginError> {
        // check for correct and unambiguous fields on input
        process_grid_input(
            input,
            self.extent_format,
            self.grid_type,
            &self.population_source,
        );
        Ok(())
    }
}

/// explains invalid query spatial arguments to users.
fn validate_query(input: &serde_json::Value) -> Result<(), InputPluginError> {
    let malformed_msg = match (
        input.get(super::EXTENT),
        input.get(super::ORIGIN_X),
        input.get(super::ORIGIN_Y),
    ) {
        (None, Some(_), Some(_)) => return Ok(()), // x,y query (no processing required)
        (Some(_), None, None) => return Ok(()),    // extent query (process with grid plugin)
        (None, None, None) => format!(
            "invalid spatial arguments: neither of {} or ({}, {}) were provided.",
            super::EXTENT,
            super::ORIGIN_X,
            super::ORIGIN_Y,
        ),
        (None, Some(_), None) => format!(
            "invalid spatial arguments: only {} was provided, must also include {}",
            super::ORIGIN_X,
            super::ORIGIN_Y
        ),
        (None, None, Some(_)) => format!(
            "invalid spatial arguments: only {} was provided, must also include {}",
            super::ORIGIN_Y,
            super::ORIGIN_X
        ),
        (Some(_), Some(_), None) => format!(
            "ambiguous spatial arguments: {} was provided along with {}. please provide only an extent or a coordinate pair, not both.",
            super::EXTENT,
            super::ORIGIN_X
        ),
        (Some(_), None, Some(_)) => format!(
            "ambiguous spatial arguments: {} was provided along with {}. please provide only an extent or a coordinate pair, not both.",
            super::EXTENT,
            super::ORIGIN_Y
        ),
        (Some(_), Some(_), Some(_)) => format!(
            "ambiguous spatial arguments: all three of {}, {}, and {} were provided. please provide only an extent or a coordinate pair, not both.",
            super::EXTENT,
            super::ORIGIN_X,
            super::ORIGIN_Y
        ),
    };
    Err(InputPluginError::InputPluginFailed(malformed_msg))
}

/// helper function that loads a population dataset for this query and appends population
/// values to each row based on the areal intersection/proportioning technique.
///
/// # Arguments
/// * `queries` - JSON queries to append population data
/// * `population_source` - provider for population data
fn add_population_source(
    queries: &mut Vec<serde_json::Value>,
    extent: &Geometry,
    population_source: &PopulationSource,
) -> Result<(), InputPluginError> {
    let pop_data = population_source.create_dataset(extent).map_err(|e| {
        InputPluginError::InputPluginFailed(format!("failure creating population dataset: {e}"))
    })?;
    let rtree = Arc::new(PolygonalRTree::new(pop_data).map_err(|e| {
        InputPluginError::InputPluginFailed(format!("failure building spatial lookup: {e}"))
    })?);
    let mut bar = Arc::new(Mutex::new(
        Bar::builder()
            .desc("map match population")
            .total(queries.len())
            .build()
            .map_err(|e| {
                InputPluginError::InputPluginFailed(format!("failure building progress bar: {e}"))
            })?,
    ));

    // find the population values via spatial index (parallelized)
    let populations_result: LinkedList<Vec<Result<_, InputPluginError>>> = queries
        .into_par_iter()
        .enumerate()
        .map(|(idx, query)| {
            if let Ok(mut bar) = bar.clone().lock() {
                let _ = bar.update(1);
            }
            let population = get_query_population_proportion(query, &rtree).map_err(|e| {
                InputPluginError::InputPluginFailed(format!(
                    "failure matching query with population data: {e}"
                ))
            })?;
            Ok((idx, population))
        })
        .collect_vec_list();
    eprintln!();

    // update the input queries with (proportioned) population values
    for pop_chunk in populations_result.into_iter() {
        for row in pop_chunk.into_iter() {
            let (idx, pop) = row?;
            let row = queries[idx].to_owned();
            match row {
                serde_json::Value::Object(mut map) => {
                    map.insert(String::from(super::POPULATION), json![pop]);
                    let new_row = serde_json::Value::Object(map);
                    queries[idx] = new_row;
                    Ok(())
                }
                _ => Err(InputPluginError::InternalError(String::from(
                    "user input is not JSON object!",
                ))),
            }?
        }
    }

    Ok(())
}

/// to determine the area for this grid cell, we want to know what
/// percent of each intersecting geometry overlaps geographically
/// with the grid geometry, and then we use that percentage to
/// perform a uniform (dis) aggregation from the source data.
fn get_query_population_proportion(
    row: &serde_json::Value,
    population: &PolygonalRTree<f64>,
) -> Result<f64, String> {
    let wkt_string = row.get_config_string(&super::GEOMETRY, &"").map_err(|e| {
        format!(
            "internal error, expected {} on grid row: {}",
            super::GEOMETRY,
            e
        )
    })?;
    let geometry = Geometry::try_from_wkt_str(&wkt_string)
        .map_err(|e| format!("internal error, expected {} is WKT: {}", super::GEOMETRY, e))?;
    let intersecting = population.intersection_with_overlap_area(&geometry)?;

    let mut population = 0.0;
    for (node, overlap_area) in intersecting.iter() {
        let ratio = overlap_area / node.geometry.unsigned_area();
        let overlap_population = node.data * ratio;
        population += overlap_population;
    }
    Ok(population)
}
