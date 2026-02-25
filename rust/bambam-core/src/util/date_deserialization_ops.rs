use chrono::{NaiveDate, NaiveDateTime, ParseResult};
use serde::de::Error;
use serde::Deserialize;
use serde::Deserializer;

pub const APP_DATETIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
pub const APP_DATE_FORMAT: &str = "%Y-%m-%d";

pub fn naive_date_to_str(date_str: &str) -> ParseResult<NaiveDate> {
    chrono::NaiveDate::parse_from_str(date_str, APP_DATE_FORMAT)
}

pub fn deserialize_naive_datetime<'de, D>(deserializer: D) -> Result<NaiveDateTime, D::Error>
where
    D: Deserializer<'de>,
{
    let date_str: String = String::deserialize(deserializer)?;
    chrono::NaiveDateTime::parse_from_str(&date_str, APP_DATETIME_FORMAT)
        .map_err(|e| D::Error::custom(format!("Invalid datetime format: {e}")))
}

pub fn deserialize_naive_date<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
where
    D: Deserializer<'de>,
{
    let date_str: String = String::deserialize(deserializer)?;
    naive_date_to_str(&date_str)
        .map_err(|e| D::Error::custom(format!("Invalid datetime format: {e}")))
}
