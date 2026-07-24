use gbfs_types::v3_0::files::{GbfsFile, GeofencingZonesFile, SystemInformationFile};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GbfsV3Import {
    /// contains the system-level metadata including opening hours.
    pub info: SystemInformationFile,
    /// contains the zonal geometries, global, and zone-specific traversal rules for this system.
    pub geofence: GeofencingZonesFile,
}

/// runs retrieval from a manifest file. allows mutli-system downloads.
pub async fn run_v3_0_manifest(
    client: &reqwest::Client,
    url: &str,
) -> Result<Vec<GbfsV3Import>, String> {
    let manifest: gbfs_types::v3_0::files::ManifestFile =
        super::ops::retrieve_file(client, url).await?;

    // find v3 datasets for each system in this manifest and run the inner retrieval function
    let mut results = vec![];
    for system in manifest.data.datasets.iter() {
        let system_id = system.system_id.clone();
        let v_search = system.versions.iter().find(|v| v.version == "3.0");
        match v_search {
            Some(v3) => {
                let gbfs_url = v3.url.value.clone();
                let result = run_v3_0_gbfs(client, &gbfs_url).await?;
                results.push(result);
            }
            None => return Err(format!("in system {system_id} no v3.0 was found.")),
        }
    }

    Ok(results)
}

/// runs retrieval at the gbfs.json level, retrieving from a single system.
pub async fn run_v3_0_gbfs(client: &reqwest::Client, url: &str) -> Result<GbfsV3Import, String> {
    let gbfs: GbfsFile = super::ops::retrieve_file(client, url).await?;

    let geofencing_zones_url = gbfs
        .data
        .get_geofencing_zones_url()
        .ok_or_else(|| format!("feed at {url} does not include geofencing_zones"))?;
    let geofence: GeofencingZonesFile =
        super::ops::retrieve_file(client, &geofencing_zones_url.value)
            .await
            .map_err(|e| {
                format!(
                    "while attempting HTTP GET '{}': {e}",
                    geofencing_zones_url.value
                )
            })?;

    let system_info_url = gbfs
        .data
        .get_system_information_url()
        .ok_or_else(|| format!("feed at {url} does not include geofencing_zones"))?;
    let info: SystemInformationFile = super::ops::retrieve_file(client, &system_info_url.value)
        .await
        .map_err(|e| format!("while attempting HTTP GET '{}': {e}", system_info_url.value))?;

    let result = GbfsV3Import { info, geofence };

    Ok(result)
}
