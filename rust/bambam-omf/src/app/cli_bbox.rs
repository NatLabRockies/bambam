use std::str::FromStr;

use geo::Coord;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CliBoundingBox {
    pub xmin: f32,
    pub xmax: f32,
    pub ymin: f32,
    pub ymax: f32,
}

impl CliBoundingBox {
    pub fn new(xmin: f32, ymin: f32, xmax: f32, ymax: f32) -> Result<Self, String> {
        if xmin > xmax {
            return Err(format!(
                "invalid bbox: xmin {xmin} greater than xmax {xmax}"
            ));
        }
        if ymin > ymax {
            return Err(format!(
                "invalid bbox: ymin {ymin} greater than ymax {ymax}"
            ));
        }
        Ok(Self {
            xmin,
            xmax,
            ymin,
            ymax,
        })
    }

    pub fn from_coords(min: &Coord<f32>, max: &Coord<f32>) -> Result<Self, String> {
        Self::new(min.x, min.y, max.x, max.y)
    }
}

impl FromStr for CliBoundingBox {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(',').collect();
        if parts.len() != 4 {
            return Err(format!("expected format: xmin,xmax,ymin,ymax, got: {s}"));
        }

        let xmin = parse_lon(parts[0])?;
        let xmax = parse_lon(parts[1])?;

        let ymin = parse_lat(parts[2])?;
        let ymax = parse_lat(parts[3])?;

        let valid_lon = xmin < xmax;
        let valid_lat = ymin < ymax;

        if !valid_lon {
            Err(format!(
                "bbox: xmin must be less than xmax, but found [{xmin},{xmax}]"
            ))
        } else if !valid_lat {
            Err(format!(
                "bbox: ymin must be less than ymax, but found [{ymin},{ymax}]"
            ))
        } else {
            Ok(CliBoundingBox {
                xmin,
                xmax,
                ymin,
                ymax,
            })
        }
    }
}

impl std::fmt::Display for CliBoundingBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{},{},{},{}", self.xmin, self.xmax, self.ymin, self.ymax)
    }
}

fn parse_lat(lat: &str) -> Result<f32, String> {
    parse_num(lat, -90.0, 90.0).map_err(|e| format!("invalid latitude: {e}"))
}

fn parse_lon(lat: &str) -> Result<f32, String> {
    parse_num(lat, -180.0, 180.0).map_err(|e| format!("invalid longitude: {e}"))
}

fn parse_num(s: &str, min: f32, max: f32) -> Result<f32, String> {
    let v = s
        .trim()
        .parse::<f32>()
        .map_err(|_| format!("not a number: {s}"))?;
    if v < min || max < v {
        Err(format!(
            "number '{v}' is not valid, must be in range [{min},{max}]"
        ))
    } else {
        Ok(v)
    }
}
