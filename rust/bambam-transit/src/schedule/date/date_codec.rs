pub mod app {
    //! deserializers for dates provided within this application which use
    //! mm-dd-yyyy format.
    //!
    use chrono::NaiveDate;
    use serde::{de::Error, Deserialize, Deserializer};

    pub const APP_DATE_FORMAT: &str = "%m-%d-%Y";
    pub const APP_TIME_FORMAT: &str = "%H:%M:%S";

    pub fn deserialize_naive_date<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
    where
        D: Deserializer<'de>,
    {
        let date_str: String = String::deserialize(deserializer)?;
        chrono::NaiveDate::parse_from_str(&date_str, APP_DATE_FORMAT)
            .map_err(|e| D::Error::custom(format!("Invalid date format: {e}")))
    }

    pub fn deserialize_optional_naive_date<'de, D>(
        deserializer: D,
    ) -> Result<Option<NaiveDate>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let date_str: String = String::deserialize(deserializer)?;
        if date_str.is_empty() {
            return Ok(None);
        }
        chrono::NaiveDate::parse_from_str(&date_str, APP_DATE_FORMAT)
            .map(Some)
            .map_err(|e| D::Error::custom(format!("Invalid date format: {e}")))
    }
}

pub mod gtfs {
    //! deserializers for dates parsed from a GTFS archive which (should) have
    //! yyyymmdd format.
    use chrono::NaiveDate;
    use serde::{de::Error, Deserialize, Deserializer};

    pub const GTFS_DATE_FORMAT: &str = "%Y%m%d";

    pub fn deserialize_naive_date<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
    where
        D: Deserializer<'de>,
    {
        let date_str: String = String::deserialize(deserializer)?;
        chrono::NaiveDate::parse_from_str(&date_str, GTFS_DATE_FORMAT)
            .map_err(|e| D::Error::custom(format!("Invalid date format: {e}")))
    }

    pub fn deserialize_optional_naive_date<'de, D>(
        deserializer: D,
    ) -> Result<Option<NaiveDate>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let date_str: String = String::deserialize(deserializer)?;
        if date_str.is_empty() {
            return Ok(None);
        }
        chrono::NaiveDate::parse_from_str(&date_str, GTFS_DATE_FORMAT)
            .map(Some)
            .map_err(|e| D::Error::custom(format!("Invalid date format: {e}")))
    }
}
