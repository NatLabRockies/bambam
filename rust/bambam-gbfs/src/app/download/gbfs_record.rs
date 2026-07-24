use geo::Geometry;
use geozero::{ToGeo, geojson::GeoJson};
use itertools::Itertools;
use serde::Serialize;

use crate::{
    app::download::{GbfsVersion, ZoneConstraints},
    model::gbfs::GbfsZoneRecord,
};

pub enum GbfsRecord {
    V3_0(super::GbfsV3Import),
    V2_3(super::GbfsV2_3Import),
    V2_2(super::GbfsV2_2Import),
}

impl Serialize for GbfsRecord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            GbfsRecord::V3_0(record) => record.serialize(serializer),
            GbfsRecord::V2_3(record) => record.serialize(serializer),
            GbfsRecord::V2_2(record) => record.serialize(serializer),
        }
    }
}

impl GbfsRecord {
    /// downloads a dataset from a URL for a given GBFS version.
    pub async fn download_from_gbfs_endpoint(
        client: &reqwest::Client,
        url: &str,
        version: GbfsVersion,
    ) -> Result<Self, String> {
        match version {
            GbfsVersion::V3_0 => {
                let gbfs = super::gbfs_v3_0::run_v3_0_gbfs(client, url).await?;
                Ok(Self::V3_0(gbfs))
            }
            GbfsVersion::V2_3 => {
                let gbfs = super::gbfs_v2_3::run_v2_3_gbfs(client, url).await?;
                Ok(Self::V2_3(gbfs))
            }
            GbfsVersion::V2_2 => {
                let gbfs = super::gbfs_v2_2::run_v2_2_gbfs(client, url).await?;
                Ok(Self::V2_2(gbfs))
            }
        }
    }

    pub fn no_geofence(&self) -> bool {
        match self {
            GbfsRecord::V3_0(record) => record.geofence.data.geofencing_zones.features.is_empty(),
            GbfsRecord::V2_3(record) => record.geofence.data.geofencing_zones.features.is_empty(),
            GbfsRecord::V2_2(record) => record.geofence.data.geofencing_zones.features.is_empty(),
        }
    }

    pub fn system_id(&self) -> String {
        match self {
            GbfsRecord::V3_0(record) => record.info.data.system_id.clone(),
            GbfsRecord::V2_3(record) => record.info.data.system_id.clone(),
            GbfsRecord::V2_2(record) => record.info.data.system_id.clone(),
        }
    }

    pub fn n_features(&self) -> usize {
        match self {
            GbfsRecord::V3_0(record) => record.geofence.data.geofencing_zones.features.len(),
            GbfsRecord::V2_3(record) => record.geofence.data.geofencing_zones.features.len(),
            GbfsRecord::V2_2(record) => record.geofence.data.geofencing_zones.features.len(),
        }
    }

    /// gets the geometry for a feature.
    pub fn get_feature_geometry(&self, idx: usize) -> Result<Geometry, String> {
        match self {
            GbfsRecord::V3_0(record) => {
                let f = record
                    .geofence
                    .data
                    .geofencing_zones
                    .features
                    .get(idx)
                    .ok_or_else(|| format!("feature index {idx} not found"))?;
                let geom_str = serde_json::to_string(f)
                    .map_err(|e| format!("failure deserializing feature: {e}"))?;
                let geojson = GeoJson(&geom_str);
                let geometry = geojson
                    .to_geo()
                    .map_err(|e| format!("unable to read GeoJSON as MultiPolygon: {e}"))?;
                Ok(geometry)
            }
            GbfsRecord::V2_3(record) => {
                let f = record
                    .geofence
                    .data
                    .geofencing_zones
                    .features
                    .get(idx)
                    .ok_or_else(|| format!("feature index {idx} not found"))?;
                let geom_str = serde_json::to_string(f)
                    .map_err(|e| format!("failure deserializing feature: {e}"))?;
                let geojson = GeoJson(&geom_str);
                let geometry = geojson
                    .to_geo()
                    .map_err(|e| format!("unable to read GeoJSON as MultiPolygon: {e}"))?;
                Ok(geometry)
            }
            GbfsRecord::V2_2(record) => {
                let f = record
                    .geofence
                    .data
                    .geofencing_zones
                    .features
                    .get(idx)
                    .ok_or_else(|| format!("feature index {idx} not found"))?;
                let geom_str = serde_json::to_string(f)
                    .map_err(|e| format!("failure deserializing feature: {e}"))?;
                let geojson = GeoJson(&geom_str);
                let geometry = geojson
                    .to_geo()
                    .map_err(|e| format!("unable to read GeoJSON as MultiPolygon: {e}"))?;
                Ok(geometry)
            }
        }
    }

    /// converts a feature in the GBFS dataset into a [GbfsZoneRecord].
    pub fn get_feature_zone_record(&self, idx: usize) -> Result<GbfsZoneRecord, String> {
        match self {
            GbfsRecord::V3_0(gbfs) => {
                let system_id = gbfs.info.data.system_id.clone();
                let global_constraints =
                    ZoneConstraints::from_v3_0(gbfs.geofence.data.global_rules.as_ref());
                let feature = gbfs
                    .geofence
                    .data
                    .geofencing_zones
                    .features
                    .get(idx)
                    .ok_or_else(|| format!("feature index {idx} not found"))?;
                let found_constraints: Vec<ZoneConstraints> =
                    match feature.properties.rules.as_ref() {
                        Some(rs) => rs
                            .iter()
                            .filter(|r| r.vehicle_type_ids.is_none())
                            .map(|r| r.into())
                            .collect_vec(),
                        None => vec![],
                    };
                let start = feature.properties.start.clone();
                let end = feature.properties.end.clone();
                // merge global and feature-specific constraints
                let zone_constraints = ZoneConstraints::merge_constraints(
                    &global_constraints,
                    &found_constraints,
                    None,
                )
                .unwrap_or_else(|| ZoneConstraints::allow_all());
                Ok(GbfsZoneRecord::new(
                    system_id,
                    idx,
                    start,
                    end,
                    zone_constraints,
                ))
            }
            GbfsRecord::V2_3(gbfs) => {
                let system_id = gbfs.info.data.system_id.clone();
                let global_constraints = vec![];
                let feature = gbfs
                    .geofence
                    .data
                    .geofencing_zones
                    .features
                    .get(idx)
                    .ok_or_else(|| format!("feature index {idx} not found"))?;
                let found_constraints: Vec<ZoneConstraints> =
                    match feature.properties.rules.as_ref() {
                        Some(rs) => rs
                            .iter()
                            .filter(|r| r.vehicle_type_id.is_none())
                            .map(|r| r.into())
                            .collect_vec(),
                        None => vec![],
                    };
                let start = process_optional_ts_to_string(feature.properties.start)?;
                let end = process_optional_ts_to_string(feature.properties.end)?;
                // merge global and feature-specific constraints
                let zone_constraints = ZoneConstraints::merge_constraints(
                    &global_constraints,
                    &found_constraints,
                    None,
                )
                .unwrap_or_else(|| ZoneConstraints::allow_all());
                Ok(GbfsZoneRecord::new(
                    system_id,
                    idx,
                    start,
                    end,
                    zone_constraints,
                ))
            }
            GbfsRecord::V2_2(gbfs) => {
                let system_id = gbfs.info.data.system_id.clone();
                let global_constraints = vec![];
                let feature = gbfs
                    .geofence
                    .data
                    .geofencing_zones
                    .features
                    .get(idx)
                    .ok_or_else(|| format!("feature index {idx} not found"))?;
                let found_constraints: Vec<ZoneConstraints> =
                    match feature.properties.rules.as_ref() {
                        Some(rs) => rs
                            .iter()
                            .filter(|r| r.vehicle_type_id.is_none())
                            .map(|r| r.into())
                            .collect_vec(),
                        None => vec![],
                    };
                let start = process_optional_ts_to_string(feature.properties.start)?;
                let end = process_optional_ts_to_string(feature.properties.end)?;
                // merge global and feature-specific constraints
                let zone_constraints = ZoneConstraints::merge_constraints(
                    &global_constraints,
                    &found_constraints,
                    None,
                )
                .unwrap_or_else(|| ZoneConstraints::allow_all());
                Ok(GbfsZoneRecord::new(
                    system_id,
                    idx,
                    start,
                    end,
                    zone_constraints,
                ))
            }
        }
    }
}

fn process_optional_ts_to_string(s: Option<i64>) -> Result<Option<String>, String> {
    match s {
        None => Ok(None),
        Some(ts) => timestamp_from_int(ts).map(Some),
    }
}

fn timestamp_from_int(t: i64) -> Result<String, String> {
    chrono::DateTime::from_timestamp(t, 0)
        .ok_or_else(|| format!("could not parse timestamp '{t}'"))
        .map(|ts| ts.to_rfc3339())
}
