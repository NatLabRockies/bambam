use std::cmp::Ordering;

use crate::model::osm::graph::OsmNodeId;
use geo::LineString;
use geo::{Convert, MapCoords};
use geozero::{wkt::Wkt as WktReader, ToGeo, ToWkt};
use itertools::Itertools;
use routee_compass_core::model::unit::SpeedUnit;
use serde::{de, Serializer};
use uom::si::f64::Velocity;
use uom::si::velocity;

pub const DEFAULT_WALK_SPEED_KPH: f64 = 5.0;

/// deals with the various ways that speed keys can appear. handles
/// valid cases such as:
///   - 45        (45 kph)
///   - 45 mph    (72.4203 kph)
///   - walk      (5 kph)
///
/// and invalid cases that are documented, such as:
///   - 45; 80    (takes the smaller of the two, so, 45 kph)
///
/// see https://wiki.openstreetmap.org/wiki/Key:maxspeed
pub fn deserialize_speed(
    s: &str,
    separator: Option<&str>,
    ignore_invalid_entries: bool,
) -> Result<Option<uom::si::f64::Velocity>, String> {
    let separated_entries = match separator {
        Some(sep) => s.split(sep).collect_vec(),
        None => vec![s],
    };
    match separated_entries[..] {
        [] => Err(format!(
            "internal error: attempting to unpack empty maxspeed value '{s}'"
        )),
        [entry] => {
            match entry.split(" ").collect_vec()[..] {
                // see https://wiki.openstreetmap.org/wiki/Key:maxspeed#Possible_tagging_mistakes
                // for list of some values we should ignore that are known.
                ["unposted"] => Ok(None),
                ["unknown"] => Ok(None),
                ["default"] => Ok(None),
                ["variable"] => Ok(None),
                ["national"] => Ok(None),
                ["25mph"] => Ok(Some(Velocity::new::<velocity::mile_per_hour>(25.0))),

                // todo! handle all default speed limits
                // see https://wiki.openstreetmap.org/wiki/Default_speed_limits
                ["walk"] => {
                    // Austria + Germany's posted "walking speed". i found a reference that
                    // suggests this is 4-7kph:
                    // https://en.wikivoyage.org/wiki/Driving_in_Germany#Speed_limits
                    Ok(Some(Velocity::new::<velocity::kilometer_per_hour>(
                        DEFAULT_WALK_SPEED_KPH,
                    )))
                }
                [speed_str] => {
                    let speed_result = speed_str
                        .parse::<i64>()
                        .map(|i| i as f64)
                        .map_err(|e| format!("speed value {speed_str} not a valid number: {e}"))
                        .or_else(|e1| {
                            speed_str.parse::<f64>().map_err(|e2| {
                                format!("speed value {speed_str} not a valid number: {e1} {e2}")
                            })
                        });

                    let speed = match speed_result {
                        Ok(speed) => speed,
                        Err(e) if !ignore_invalid_entries => {
                            return Err(e);
                        }
                        Err(_) => return Ok(None),
                    };
                    if speed == 0.0 || speed.is_nan() {
                        Ok(None)
                    } else {
                        Ok(Some(Velocity::new::<velocity::kilometer_per_hour>(speed)))
                    }
                }
                [speed_str, unit_str] => {
                    let speed_result = speed_str
                        .parse::<f64>()
                        .map_err(|e| format!("speed value {speed_str} not a valid number: {e}"));

                    let speed = match speed_result {
                        Ok(speed) => speed,
                        Err(e) if !ignore_invalid_entries => {
                            return Err(e);
                        }
                        Err(_) => return Ok(None),
                    };
                    if speed == 0.0 || speed.is_nan() {
                        return Ok(None);
                    }
                    let speed_unit = match unit_str {
                        "kph" => SpeedUnit::KPH,
                        "mph" => SpeedUnit::MPH,
                        _ if !ignore_invalid_entries => {
                            return Err(format!(
                                "unknown speed unit {unit_str} with value {speed}"
                            ));
                        }
                        _ => {
                            // some garbage or uncommon unit type like feet per minute, we can skip this entry.
                            return Ok(None);
                        }
                    };
                    let result = speed_unit.to_uom(speed);
                    Ok(Some(result))
                }
                _ => Err(format!("unexpected maxspeed entry '{s}'")),
            }
        }
        _ => {
            let maxspeeds = separated_entries
                .to_vec()
                .iter()
                .map(|e| deserialize_speed(e, separator, ignore_invalid_entries))
                .collect::<Result<Vec<_>, _>>()?;
            let min = maxspeeds
                .into_iter()
                .min_by(|a, b| match (a, b) {
                    (None, None) => Ordering::Equal,
                    (None, Some(_)) => Ordering::Greater,
                    (Some(_), None) => Ordering::Less,
                    (Some(a), Some(b)) => a.partial_cmp(b).unwrap_or(Ordering::Greater),
                })
                .flatten();
            Ok(min)
        }
    }
}

/// deserializes a CSV string, which should be enquoted, into a LineString<f32>.
pub fn csv_string_to_linestring(v: &str) -> Result<LineString<f32>, String> {
    // Remove surrounding double quotes if present
    let cleaned_v = if v.starts_with('"') && v.ends_with('"') && v.len() > 1 {
        &v[1..v.len() - 1]
    } else {
        v
    };

    let geometry_f64 = WktReader(cleaned_v)
        .to_geo()
        .map_err(|e| format!("failed to parse WKT string: {e}"))?;
    let linestring: LineString<f32> = match geometry_f64 {
        geo::Geometry::LineString(ls) => ls.map_coords(|c| geo::Coord {
            x: c.x as f32,
            y: c.y as f32,
        }),
        _ => return Err("expected a LINESTRING, got a different geometry type".to_string()),
    };
    Ok(linestring)
}

/// uses a WKT geometry representation to serialize geo::LineString types
pub fn serialize_linestring<S>(row: &LineString<f32>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let row_f64: geo::LineString<f64> = row.convert();
    let wkt = geo::Geometry::from(row_f64).to_wkt().unwrap_or_default();
    s.serialize_str(&wkt)
}

/// writes geo::LineString types as a WKT
pub fn deserialize_linestring<'de, D>(d: D) -> Result<LineString<f32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct LineStringVisitor;

    impl<'de> de::Visitor<'de> for LineStringVisitor {
        type Value = LineString<f32>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("an (optionally double-quoted) WKT LineString<f32>")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            csv_string_to_linestring(v).map_err(serde::de::Error::custom)
        }

        fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.visit_str(&v)
        }
    }

    d.deserialize_str(LineStringVisitor {})
}

/// takes all node ids found between src an dst in a list of nodes.
/// node list is not required to start with src, end with dst.
pub fn extract_between_nodes<'a>(
    src: &'a OsmNodeId,
    dst: &'a OsmNodeId,
    nodes: &'a [OsmNodeId],
) -> Option<&'a [OsmNodeId]> {
    let start = nodes.iter().position(|x| x == src)?; // Using ? for early return
    let end = nodes[start..].iter().position(|x| x == dst)?; // Search after 'a'

    if start <= start + end {
        Some(&nodes[start..=start + end])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::model::osm::graph::{osm_way_ops, OsmNodeId};

    #[test]
    fn test_extract() {
        let nodes = vec![
            OsmNodeId(1),
            OsmNodeId(2),
            OsmNodeId(3),
            OsmNodeId(4),
            OsmNodeId(5),
            OsmNodeId(6),
        ];
        let result = osm_way_ops::extract_between_nodes(&OsmNodeId(2), &OsmNodeId(4), &nodes);
        println!("{result:?}");
        match result {
            Some([a, b, c]) => {
                assert_eq!(a, &nodes[1]);
                assert_eq!(b, &nodes[2]);
                assert_eq!(c, &nodes[3]);
            }
            _ => panic!("not as expected"),
        }
    }

    #[test]
    fn deserialize_speed_1() {
        //   - 45        (45 kph)
        match osm_way_ops::deserialize_speed("45", None, false) {
            Ok(Some(speed)) => {
                let result = speed.get::<uom::si::velocity::kilometer_per_hour>();
                let diff_from_expected = 45.0 - result;
                assert!(
                    diff_from_expected < 0.001,
                    "value {result} should be within 0.001 of 45.0"
                );
            }
            Ok(None) => panic!("should parse valid speed"),
            Err(e) => panic!("{e}"),
        }
    }
    #[test]
    fn deserialize_speed_2() {
        //   - 45 mph    (72.4203 kph)
        match osm_way_ops::deserialize_speed("45 mph", None, false) {
            Ok(Some(speed)) => {
                let result = speed.get::<uom::si::velocity::mile_per_hour>();
                let diff_from_expected = 45.0 - result;
                assert!(
                    diff_from_expected < 0.001,
                    "value {result} should be within 0.001 of 45.0"
                );
            }
            Ok(None) => panic!("should parse valid speed"),
            Err(e) => panic!("{e}"),
        }
    }
    #[test]
    fn deserialize_speed_3() {
        //   - walk      (5 kph)
        match osm_way_ops::deserialize_speed("5 kph", None, false) {
            Ok(Some(speed)) => {
                let result = speed.get::<uom::si::velocity::kilometer_per_hour>();
                let diff_from_expected = 5.0 - result;
                assert!(
                    diff_from_expected < 0.001,
                    "value {result} should be within 0.001 of 5.0"
                );
            }
            Ok(None) => panic!("should parse valid speed"),
            Err(e) => panic!("{e}"),
        }
    }

    #[test]
    fn deserialize_speed_sep_1() {
        //   - a few speed values, where 3 kph is the minimum
        match super::deserialize_speed("3.1415 kph;3;2 mph", Some(";"), false) {
            Ok(Some(speed)) => {
                assert_eq!(speed.get::<uom::si::velocity::kilometer_per_hour>(), 3.0);
            }
            Ok(None) => panic!("should parse valid speed"),
            Err(e) => panic!("{e}"),
        }
    }

    #[test]
    fn deserialize_csv_linestring_01() {
        let wkt = "\"LINESTRING (0 0, 1 1)\"";
        let expected = geo::line_string![
            geo::coord! { x: 0.0f32, y: 0.0f32},
            geo::coord! { x: 1.0f32, y: 1.0f32},
        ];
        match super::csv_string_to_linestring(wkt) {
            Ok(result) => assert_eq!(result, expected),
            Err(e) => panic!("{e}"),
        }
    }

    #[test]
    fn deserialize_csv_linestring_no_quotes() {
        let wkt = "LINESTRING (0 0, 1 1)";
        match super::csv_string_to_linestring(wkt) {
            Ok(_) => {}
            Err(e) => panic!("{e}"),
        }
    }

    #[test]
    fn linestring_from_csv_01() {
        #[allow(unused)]
        #[derive(serde::Deserialize, Debug)]
        struct Row {
            index: usize,
            #[serde(deserialize_with = "super::deserialize_linestring")]
            geometry: geo::LineString<f32>,
        }
        let path = Path::new("src/model/osm/graph/test/linestring_01.csv");
        let mut row_reader =
            csv::Reader::from_path(path).expect("test invariant: file should exist");
        let rows = row_reader
            .deserialize()
            .collect::<Result<Vec<Row>, _>>()
            .expect("deserialization failed");
        match &rows[..] {
            [row] => println!("{row:?}"),
            _ => panic!("unexpected rows result {rows:?}"),
        }
    }
}
