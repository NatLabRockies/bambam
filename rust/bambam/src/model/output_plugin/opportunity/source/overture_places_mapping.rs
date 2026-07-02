use std::{collections::HashMap, path::PathBuf};

use csv::StringRecord;
use routee_compass_core::util::fs::read_utils;
use serde::{Deserialize, Serialize};

pub type PlacesMapping = HashMap<String, Vec<String>>;

/// sources a mapping from OvertureMaps Places categories into MEP categories.
///
/// # Serde
///
/// untagged deserialization. attempts to first deserialize directly as a HashMap.
/// if that fails, attempts to read as a FromCsv object.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum OverturePlacesMappingConfig {
    FromConfig(PlacesMapping),
    FromCsv {
        places_mapping_input_file: String,
        places_category_column: String,
        mep_category_column: String,
        mep_category_separator: String,
    },
}

impl OverturePlacesMappingConfig {
    pub fn build(&self) -> Result<PlacesMapping, String> {
        match self {
            OverturePlacesMappingConfig::FromConfig(hash_map) => Ok(hash_map.clone()),
            OverturePlacesMappingConfig::FromCsv {
                places_mapping_input_file,
                places_category_column,
                mep_category_column,
                mep_category_separator,
            } => {
                let mut reader = csv::ReaderBuilder::new()
                    .has_headers(true)
                    .from_path(places_mapping_input_file)
                    .map_err(|e| {
                        format!(
                            "failure reading mep mapping csv at {places_mapping_input_file}: {e}"
                        )
                    })?;
                let header_records = reader
                    .headers()
                    .map_err(|e| format!("mep mapping csv should have headers. {e}"))?;
                let header: HashMap<String, usize> = header_records
                    .iter()
                    .enumerate()
                    .map(|(col_idx, name)| (name.to_string(), col_idx))
                    .collect();

                let mut result = HashMap::new();
                for (row_idx, row_result) in reader.into_records().enumerate() {
                    let (omf_label, mep_labels) = process_row(
                        row_idx,
                        row_result,
                        &header,
                        places_category_column,
                        mep_category_column,
                        mep_category_separator,
                    )?;
                    let _ = result.insert(omf_label, mep_labels);
                }
                Ok(result)
            }
        }
    }
}

/// helper function to process a row of the MEP mapping CSV file.
fn process_row(
    row_idx: usize,
    row_result: Result<StringRecord, csv::Error>,
    header: &HashMap<String, usize>,
    places_category_column: &str,
    mep_category_column: &str,
    mep_category_separator: &str,
) -> Result<(String, Vec<String>), String> {
    let row = row_result
        .map_err(|e| format!("failure getting row {row_idx} from mep mapping csv: {e}"))?;
    let omf_label_col_idx = header.get(places_category_column).ok_or_else(|| {
        format!("mep mapping csv missing expected {places_category_column} column")
    })?;
    let mep_label_col_idx = header
        .get(mep_category_column)
        .ok_or_else(|| format!("mep mapping csv missing expected {mep_category_column} column"))?;

    let omf_label = row.get(*omf_label_col_idx)
        .ok_or_else(|| format!("mep mapping csv row {row_idx} missing expected {places_category_column} column index {omf_label_col_idx}"))?
        .to_string();
    let mep_labels: Vec<String> = row.get(*mep_label_col_idx)
        .ok_or_else(|| format!("mep mapping csv row {row_idx} missing expected {mep_category_column} column index {mep_label_col_idx}"))?
        .split(mep_category_separator)
        .map(String::from)
        .collect();

    Ok((omf_label, mep_labels))
}
