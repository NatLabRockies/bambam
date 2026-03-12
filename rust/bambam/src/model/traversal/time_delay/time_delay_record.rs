use geo::{ConvexHull, Geometry, MapCoords, MultiPolygon, Point, Polygon};
use geozero::{wkt::Wkt as WktReader, ToGeo};
use rstar::{RTreeObject, AABB};
use serde::de;
use serde::Serialize;
use uom::si::f64::Time;

#[derive(Serialize, Clone, Debug)]
pub struct TimeDelayRecord {
    pub geometry: Geometry<f32>,
    pub time: Time,
}

impl RTreeObject for TimeDelayRecord {
    type Envelope = AABB<Point<f32>>;
    fn envelope(&self) -> Self::Envelope {
        match &self.geometry {
            Geometry::Polygon(p) => p.envelope(),
            Geometry::MultiPolygon(mp) => mp.convex_hull().envelope(),
            Geometry::GeometryCollection(gc) => gc.convex_hull().envelope(),
            _ => panic!("only polygon, multipolygon, and geometry collection are supported"),
        }
    }
}

/// custom deserializer for access records which expects a
/// geometry and time field. the geometry should be a WKT POLYGON
/// or MULTIPOLYGON, and the time value should be a real number.
impl<'de> de::Deserialize<'de> for TimeDelayRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RecordVisitor;

        impl<'de> de::Visitor<'de> for RecordVisitor {
            type Value = TimeDelayRecord;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an enquoted WKT string, a comma, and a number")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Self::Value, V::Error>
            where
                V: de::MapAccess<'de>,
            {
                let mut geometry_option: Option<Geometry<f32>> = None;
                let mut time_option: Option<Time> = None;
                let mut next: Option<(&str, &str)> = map.next_entry()?;
                while next.is_some() {
                    if let Some((key, value)) = next {
                        match key {
                            "geometry" => {
                                // should be one of POLYGON | MULTIPOLYGON
                                let value_trim = value.replace('\"', "");
                                let geo_f64 = WktReader(value_trim.as_str())
                                    .to_geo()
                                    .or_else(|_| WktReader(value).to_geo())
                                    .map_err(|e| {
                                        de::Error::custom(format!(
                                            "unable to parse WKT geometry '{}': {}",
                                            &value, e
                                        ))
                                    })?;
                                let row_geometry: Geometry<f32> = match geo_f64 {
                                    Geometry::Polygon(p) => {
                                        Geometry::Polygon(p.map_coords(|c| geo::Coord {
                                            x: c.x as f32,
                                            y: c.y as f32,
                                        }))
                                    }
                                    Geometry::MultiPolygon(mp) => {
                                        Geometry::MultiPolygon(mp.map_coords(|c| geo::Coord {
                                            x: c.x as f32,
                                            y: c.y as f32,
                                        }))
                                    }
                                    _ => {
                                        return Err(de::Error::custom(format!(
                                            "expected Polygon or MultiPolygon geometry, found unexpected type in '{}'",
                                            &value
                                        )));
                                    }
                                };
                                geometry_option = Some(row_geometry);
                            }
                            "time" => {
                                let row_time =
                                    serde_json::from_str::<Time>(value).map_err(|e| {
                                        de::Error::custom(format!(
                                            "unable to parse time value '{}': {}",
                                            &value, e
                                        ))
                                    })?;
                                time_option = Some(row_time);
                            }
                            &_ => {}
                        }
                    } else {
                        return Err(de::Error::custom("internal error"));
                    }
                    next = map.next_entry()?;
                }

                match (geometry_option, time_option) {
                    (None, None) => Err(de::Error::missing_field("geometry,time")),
                    (None, Some(_)) => Err(de::Error::missing_field("geometry")),
                    (Some(_), None) => Err(de::Error::missing_field("time")),
                    (Some(geometry), Some(time)) => Ok(TimeDelayRecord { geometry, time }),
                }
            }
        }

        deserializer.deserialize_map(RecordVisitor {})
    }
}
