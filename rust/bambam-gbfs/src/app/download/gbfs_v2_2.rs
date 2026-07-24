use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GbfsV2_2Import {
    /// contains the system-level metadata including opening hours.
    pub info: types::SystemInformationFile,
    /// contains the zonal geometries, global, and zone-specific traversal rules for this system.
    pub geofence: types::GeofencingZonesFile,
}

/// runs retrieval at the gbfs.json level, retrieving from a single system.
pub async fn run_v2_2_gbfs(client: &reqwest::Client, url: &str) -> Result<GbfsV2_2Import, String> {
    let gbfs: types::GbfsFile = super::ops::retrieve_file(client, url)
        .await
        .map_err(|e| format!("while downloading gbfs.json file, {e}"))?;

    let geofencing_zones_url = gbfs
        .get_geofencing_zones_url("en")
        .ok_or_else(|| format!("feed at {url} does not include geofencing_zones"))?;
    let geofence: types::GeofencingZonesFile =
        super::ops::retrieve_file(client, &geofencing_zones_url)
            .await
            .map_err(|e| format!("while attempting HTTP GET '{}': {e}", geofencing_zones_url))?;

    let system_info_url = gbfs
        .get_system_information_url("en")
        .ok_or_else(|| format!("feed at {url} does not include geofencing_zones"))?;
    let info: types::SystemInformationFile = super::ops::retrieve_file(client, &system_info_url)
        .await
        .map_err(|e| format!("while attempting HTTP GET '{}': {e}", system_info_url))?;

    let result = GbfsV2_2Import { info, geofence };

    Ok(result)
}

pub mod types {
    //! GBFS 2.2 is not implemented in gbfs_types but it is used by Lime.
    use std::collections::HashMap;

    use serde::{Deserialize, Serialize};

    pub type GbfsFile = GbfsMetadata<Feeds>;
    pub type SystemInformationFile = GbfsMetadata<SystemInformation>;
    pub type GeofencingZonesFile = GbfsMetadata<GeofencingZones>;

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct GbfsMetadata<T> {
        /// Indicates the last time data in the feed was updated. This timestamp represents the publisher's knowledge of the current state of the system at this point in time.
        pub last_updated: Timestamp,
        /// Number of seconds before the data in the feed will be updated again (0 if the data should always be refreshed).
        pub ttl: u32,
        /// GBFS version number to which the feed conforms, according to the versioning framework.
        pub version: String,
        /// Response data.
        pub data: T,
    }

    pub type Feeds = HashMap<Language, GbfsLanguageFeeds>;

    impl GbfsMetadata<Feeds> {
        pub fn get_geofencing_zones_url(&self, language: &str) -> Option<Url> {
            self.get_file_url(language, "geofencing_zones")
        }

        pub fn get_system_information_url(&self, language: &str) -> Option<Url> {
            self.get_file_url(language, "system_information")
        }

        fn get_file_url(&self, language: &str, feed_type: &str) -> Option<Url> {
            self.data.get(language).and_then(|feed| {
                feed.feeds.iter().find_map(|f| {
                    if f.name == feed_type {
                        Some(f.url.clone())
                    } else {
                        None
                    }
                })
            })
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct GbfsLanguageFeeds {
        pub feeds: Vec<GbfsDataFeed>,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct GbfsDataFeed {
        /// Key identifying the type of feed this is. The key MUST be the base file name defined in the spec for the corresponding feed type
        pub name: FeedType,
        /// URL for the feed. Note that the actual feed endpoints (urls) may not be defined in the `file_name.json` format.
        /// For example, a valid feed endpoint could end with `station_info` instead of `station_information.json`.
        pub url: Url,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct SystemInformation {
        /// Identifier for this vehicle share system. This should be globally unique (even between different systems).
        pub system_id: String,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct GeofencingZones {
        pub geofencing_zones: GeofenceCollection,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct GeofenceCollection {
        pub r#type: String,
        pub features: Vec<GeofenceFeature>,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct GeofenceFeature {
        pub r#type: String,
        pub geometry: GeofenceGeometry,
        pub properties: GeofenceProperties,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    #[serde(tag = "type")]
    pub enum GeofenceGeometry {
        #[serde(rename = "MultiPolygon")]
        MultiPolygon(Vec<Vec<Vec<Vec<f64>>>>),
        #[serde(rename = "Polygon")]
        Polygon(Vec<Vec<Vec<Vec<f64>>>>),
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct GeofenceProperties {
        /// Start time of the geofencing zone. If the geofencing zone is always active, this can be omitted.
        pub start: Option<Timestamp>,
        /// End time of the geofencing zone. If the geofencing zone is always active, this can be omitted.
        pub end: Option<Timestamp>,
        /// Array that contains one object per rule.
        pub rules: Option<Vec<GeofenceRules>>,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct GeofenceRules {
        pub vehicle_type_id: Option<Vec<String>>,
        pub ride_allowed: bool,
        pub ride_through_allowed: bool,
        pub maximum_speed_kph: Option<i32>,
    }

    pub type Timestamp = i64;

    /// A fully qualified URL that includes `http://` or `https://`. Any special characters in the URL MUST be correctly escaped. See the following <https://www.w3.org/Addressing/URL/4_URI_Recommentations.html> for a description of how to create fully qualified URL values.
    pub type Url = String; // url::URL;

    /// An IETF BCP 47 language code. For an introduction to IETF BCP 47, refer to <https://www.rfc-editor.org/rfc/bcp/bcp47.txt> and <https://www.w3.org/International/articles/language-tags/>. Examples: `en` for English, `en-US`
    pub type Language = String;

    /// Type of a GBFS feed.
    /// Current values are :
    /// - `gbfs` for [GbfsFile],
    /// - `gbfs_versions` for [GbfsVersionsFile],
    /// - `system_information` for [SystemInformationFile],
    /// - `vehicle_types` for [VehicleTypesFile],
    /// - `station_information` for [StationInformationFile],
    /// - `station_status` for [StationStatusFile],
    /// - `free_bike_status` for [FreeBikeStatusFile],
    /// - `system_hours` for [SystemHoursFile],
    /// - `system_calendar` for [SystemCalendarFile],
    /// - `system_regions` for [SystemRegionsFile],
    /// - `system_pricing_plans` for [SystemPricingPlansFile],
    /// - `system_alerts` for [SystemAlertsFile],
    /// - `geofencing_zones` for [GeofencingZonesFile]
    pub type FeedType = String;
}
