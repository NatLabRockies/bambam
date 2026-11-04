use clap::ValueEnum;
use geo::{line_string, Haversine, Length, LineString, Point};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, ValueEnum, Clone)]
pub enum DistanceCalculationPolicy {
    Haversine,
    Shape,
    Fallback,
}

pub fn compute_haversine(src_point: Point<f64>, dst_point: Point<f64>) -> uom::si::f64::Length {
    let line: LineString<f64> = line_string![src_point.0, dst_point.0];
    uom::si::f64::Length::new::<uom::si::length::meter>(Haversine.length(&line))
}
