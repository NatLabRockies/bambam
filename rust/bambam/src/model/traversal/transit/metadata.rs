use bambam_core::util::date_deserialization_ops::naive_date_to_str;
use chrono::{Duration, NaiveDate};
use routee_compass_core::model::traversal::TraversalModelError;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// fields from the metadata JSON file that are relevant for loading transit traversal models.
/// additional fields exist
#[derive(Serialize, Deserialize)]
pub struct GtfsArchiveMetadata {
    /// direct output of GTFS agencies.txt
    pub agencies: Vec<Value>,
    /// direct output of GTFS feed_info.txt
    pub feed_info: Vec<Value>,
    /// time required to read this archive using bambam-gtfs
    #[serde(deserialize_with = "deserialize_duration")]
    pub read_duration: Duration,
    /// direct output of GTFS calendar.txt by service_id
    pub calendar: Value,
    /// direct output of GTFS calendar_dates.txt by service_id
    pub calendar_dates: Value,
    /// Mapping from target date to available date for each route_id
    #[serde(deserialize_with = "deserialize_date_mapping")]
    pub date_mapping: HashMap<String, HashMap<NaiveDate, NaiveDate>>,
    /// List of unique (fully-qualified) route identifiers used in the schedules
    pub fq_route_ids: Vec<String>,
}

#[derive(Deserialize)]
struct DurationJson {
    pub secs: i64,
    pub nanos: u32,
}

fn deserialize_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let DurationJson { secs, nanos } = DurationJson::deserialize(deserializer)?;
    chrono::Duration::new(secs, nanos).ok_or_else(|| {
        D::Error::custom(format!(
            "invalid duration value with secs {secs}, nanos {nanos}"
        ))
    })
}

fn deserialize_date_mapping<'de, D>(
    deserializer: D,
) -> Result<HashMap<String, HashMap<NaiveDate, NaiveDate>>, D::Error>
where
    D: Deserializer<'de>,
{
    let original_map = HashMap::<String, HashMap<String, String>>::deserialize(deserializer)?;

    // Convert inner maps to NaiveDate keys/values
    let mut out: HashMap<String, HashMap<NaiveDate, NaiveDate>> =
        HashMap::with_capacity(original_map.len());
    for (route_id, inner) in original_map {
        let mut parsed_inner = HashMap::with_capacity(inner.len());
        for (k_str, v_str) in inner {
            let k = naive_date_to_str(&k_str)
                .map_err(|e| D::Error::custom(format!("failed to deserialize date mapping for route_id `{route_id}`: invalid date key `{k_str}`: {e}")))?;
            let v = naive_date_to_str(&v_str)
                .map_err(|e| D::Error::custom(format!("failed to deserialize date mapping for route_id `{route_id}`: invalid date value `{v_str}`: {e}")))?;
            parsed_inner.insert(k, v);
        }
        out.insert(route_id, parsed_inner);
    }

    Ok(out)
}
