use geo::algorithm::concave_hull::ConcaveHull;
use geo::concave_hull::ConcaveHullOptions;
use geo::Geometry;
use geo::KNearestConcaveHull;
use geo::MultiPoint;
use routee_compass::plugin::output::OutputPluginError;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum IsochroneAlgorithm {
    /// uses a concave hull algorithm to draw the isochrone
    ConcaveHull { concavity: f32 },
    /// uses the k-nearest concave hull algorithm. see
    /// [https://docs.rs/geo/latest/geo/algorithm/k_nearest_concave_hull/trait.KNearestConcaveHull.html]
    KNearestConcaveHull { k: u32 },
    /// uses the k-nearest concave hull algorithm but
    /// computes k dynamically via `k = log(b, n) * c` for base b (default 10), n
    /// destinations and some constant c (default 1.0).
    /// guards against dynamically-computed k < 3.
    KNearestLogScaled { base: Option<u8>, c: Option<f64> },
    /// uses the k-nearest concave hull algorithm but
    /// computes k dynamically via `k = sqrt(n)` for n
    /// destinations.
    /// guards against dynamically-computed k < 3.
    KNearestSqrtScaled,
}

impl IsochroneAlgorithm {
    pub fn run(&self, mp: MultiPoint<f32>) -> Result<Geometry<f32>, OutputPluginError> {
        match self {
            IsochroneAlgorithm::ConcaveHull { concavity } => {
                if mp.len() < 3 {
                    Ok(Geometry::Polygon(geo::polygon!()))
                } else {
                    let options = ConcaveHullOptions::default().concavity(*concavity);
                    let hull = mp.concave_hull_with_options(options);
                    Ok(Geometry::Polygon(hull))
                }
            }
            IsochroneAlgorithm::KNearestConcaveHull { k } => {
                if *k < 3 {
                    Err(OutputPluginError::OutputPluginFailed(format!(
                        "k-nearest concave hull 'k' value must be > 2, found {k}"
                    )))
                } else if mp.len() < 3 {
                    Ok(Geometry::Polygon(geo::polygon!()))
                } else {
                    let hull = mp.k_nearest_concave_hull(*k);
                    Ok(Geometry::Polygon(hull))
                }
            }
            IsochroneAlgorithm::KNearestLogScaled { base, c } => {
                // k = log(b, n) * c
                let n = mp.len() as f64;
                if n < 3.0 {
                    return Ok(Geometry::Polygon(geo::polygon!()));
                }
                let constant = c.unwrap_or(1.0);
                let log_n = match base {
                    Some(b) if *b < 2 => Err(OutputPluginError::OutputPluginFailed(format!(
                        "for k-nearest concave hull, base must be > 1, found '{b}'"
                    ))),
                    Some(b) => Ok(n.log((*b) as f64)),
                    None => Ok(n.log10()),
                }?;
                let k = if log_n < 3.0 {
                    3
                } else {
                    (log_n * constant) as u32
                };
                IsochroneAlgorithm::KNearestConcaveHull { k }.run(mp)
            }
            IsochroneAlgorithm::KNearestSqrtScaled => {
                // k = sqrt(n)
                let n = mp.len() as f64;
                if n < 3.0 {
                    return Ok(Geometry::Polygon(geo::polygon!()));
                }
                let sqrt_n = n.sqrt();
                let k = if sqrt_n < 3.0 { 3 } else { sqrt_n as u32 };
                IsochroneAlgorithm::KNearestConcaveHull { k }.run(mp)
            }
        }
    }
}
