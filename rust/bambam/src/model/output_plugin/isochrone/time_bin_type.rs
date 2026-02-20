use bambam_core::model::TimeBin;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum TimeBinType {
    List {
        times: Vec<u64>,
        skip_zero: Option<bool>,
    },
    Range {
        start: usize,
        end: usize,
        step: usize,
    },
}

impl TimeBinType {
    pub fn create_bins(&self) -> Result<Vec<TimeBin>, String> {
        match self {
            TimeBinType::List { times, skip_zero } => {
                let mut times_internal = times.clone();
                match skip_zero {
                    None => times_internal.insert(0, 0),
                    Some(skip) => {
                        if !skip {
                            times_internal.insert(0, 0)
                        }
                    }
                }
                let result: Vec<TimeBin> = times_internal
                    .iter()
                    .tuple_windows()
                    .map(|(start, end)| TimeBin {
                        min_time: *start,
                        max_time: *end,
                    })
                    .collect();
                Ok(result)
            }
            TimeBinType::Range { start, end, step } => {
                if *end == 0 || *step == 0 {
                    return Err(String::from("time bin end or step values cannot be zero"));
                }
                let range = (*start..*end).step_by(*step);
                let result: Vec<TimeBin> = range
                    .tuple_windows()
                    .map(|(s, e)| TimeBin {
                        min_time: s as u64,
                        max_time: e as u64,
                    })
                    .collect();
                Ok(result)
            }
        }
    }
}
