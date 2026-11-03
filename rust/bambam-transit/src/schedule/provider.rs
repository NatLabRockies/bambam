use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Serialize, Deserialize)]
pub struct GtfsProvider {
    pub data_type: String,
    pub provider: String,
    pub name: Option<String>,
    #[serde(rename = "urls.latest")]
    pub url: Option<String>,
    #[serde(rename = "location.country_code")]
    pub country_code: String,
    #[serde(rename = "location.subdivision_name")]
    pub state_code: Option<String>,
    #[serde(rename = "location.municipality")]
    pub municipality: Option<String>,
}

impl GtfsProvider {
    pub fn filename(&self) -> String {
        let filename_merged = [
            self.state_code.clone(),
            self.municipality.clone(),
            Some(self.provider.clone()),
            self.name.clone(),
        ]
        .iter()
        .flatten()
        .join("-");
        let filename_cleaned = filename_merged
            .replace(" ", "_")
            .replace("(", "")
            .replace(")", "");
        format!("{filename_cleaned}.zip")
    }
}

impl Display for GtfsProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let url_opt = self.url.as_ref();
        let url = url_opt.cloned().unwrap_or_default();
        let name = self.filename();
        write!(f, "\"{name}\"{url}")
    }
}
