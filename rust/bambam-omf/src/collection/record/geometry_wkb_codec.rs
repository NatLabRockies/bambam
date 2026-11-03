use geo::{Geometry, MapCoords, TryConvert};
use geozero::{error::GeozeroError, wkb::Wkb, CoordDimensions, ToGeo, ToWkb};
use serde::{de::Visitor, Deserialize, Deserializer};

struct WkbBytesOrHexString(Vec<u8>);

struct WkbBytesOrHexVisitor;

impl<'de> Visitor<'de> for WkbBytesOrHexVisitor {
    type Value = WkbBytesOrHexString;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("WKB bytes or hex-encoded WKB string")
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        // If `v` is ASCII hex (e.g. from serde_json calling visit_bytes on a hex string),
        // decode it as hex; otherwise treat it as raw WKB bytes.
        if !v.is_empty() && v.len().is_multiple_of(2) && v.iter().all(|b| b.is_ascii_hexdigit()) {
            if let Ok(decoded) = hex::decode(v) {
                return Ok(WkbBytesOrHexString(decoded));
            }
        }
        Ok(WkbBytesOrHexString(v.to_vec()))
    }

    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_bytes(&v)
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let bytes = hex::decode(v).map_err(|e| {
            serde::de::Error::custom(format!("failure converting hex wkb string to bytes: {e}"))
        })?;
        Ok(WkbBytesOrHexString(bytes))
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_str(&v)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut bytes = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(b) = seq.next_element::<u8>()? {
            bytes.push(b);
        }
        Ok(WkbBytesOrHexString(bytes))
    }
}

impl<'de> Deserialize<'de> for WkbBytesOrHexString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_bytes(WkbBytesOrHexVisitor)
    }
}

/// deserialize geometries from WKB bytes or hex WKB strings
pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Geometry<f32>>, D::Error>
where
    D: Deserializer<'de>,
{
    let data = Option::<WkbBytesOrHexString>::deserialize(deserializer)?;

    match data {
        None => Ok(None),
        Some(WkbBytesOrHexString(bytes)) if bytes.is_empty() => Ok(None),
        Some(WkbBytesOrHexString(bytes)) => {
            let g = Wkb(bytes).to_geo().map_err(|e| {
                let msg = format!("unable to parse bytes as WKB: {e}");
                serde::de::Error::custom(msg)
            })?;

            let mapped = g
                .try_map_coords(|geo::Coord { x, y }| {
                    Ok(geo::Coord {
                        x: x as f32,
                        y: y as f32,
                    })
                })
                .map_err(|e: GeozeroError| {
                    serde::de::Error::custom(format!(
                        "Could not map coordinates for geometry {g:?}: {e}"
                    ))
                })?;
            Ok(Some(mapped))
        }
    }
}

pub fn serialize<S>(t: &Option<Geometry<f32>>, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match t {
        None => s.serialize_none(),
        Some(g) => {
            let geom: Geometry<f64> = g.try_convert().map_err(|e| {
                serde::ser::Error::custom(format!(
                    "unable to convert geometry from f32 to f64: {e}"
                ))
            })?;
            let out_bytes = geom.to_wkb(CoordDimensions::xy()).map_err(|e| {
                serde::ser::Error::custom(format!("failed to write geometry as WKB: {e}"))
            })?;

            let wkb_str = out_bytes
                .iter()
                .map(|b| format!("{b:02X?}"))
                .collect::<Vec<String>>()
                .join("");

            s.serialize_str(&wkb_str)
        }
    }
}
