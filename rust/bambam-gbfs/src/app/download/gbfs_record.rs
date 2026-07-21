use geo::Geometry;
use geozero::{ToGeo, geojson::GeoJson};
use serde::Serialize;

use crate::app::download::FeatureRules;

pub enum GbfsRecord {
    V3_0(super::GbfsV3Import),
    V2_3(super::GbfsV2_3Import),
}

impl Serialize for GbfsRecord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            GbfsRecord::V3_0(record) => record.serialize(serializer),
            GbfsRecord::V2_3(record) => record.serialize(serializer),
        }
    }
}

impl GbfsRecord {
    pub fn no_geofence(&self) -> bool {
        match self {
            GbfsRecord::V3_0(record) => record.geofence.data.geofencing_zones.features.is_empty(),
            GbfsRecord::V2_3(record) => record.geofence.data.geofencing_zones.features.is_empty(),
        }
    }

    pub fn system_id(&self) -> String {
        match self {
            GbfsRecord::V3_0(record) => record.info.data.system_id.clone(),
            GbfsRecord::V2_3(record) => record.info.data.system_id.clone(),
        }
    }

    pub fn n_features(&self) -> usize {
        match self {
            GbfsRecord::V3_0(record) => record.geofence.data.geofencing_zones.features.len(),
            GbfsRecord::V2_3(record) => record.geofence.data.geofencing_zones.features.len(),
        }
    }

    /// gets the geometry from a feature
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
        }
    }

    pub fn get_feature_rules(&self) -> Result<FeatureRules, String> {
        todo!()
    }
}
