use geo::algorithm::concave_hull::ConcaveHull;
use geo::concave_hull::ConcaveHullOptions;
use geo::{Geometry, KNearestConcaveHull, MultiPoint, Simplify};
use routee_compass::plugin::output::OutputPluginError;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum IsochroneAlgorithm {
    /// uses a concave hull agorithm to draw the isochrone. as this can lead to many vertices,
    /// the user can optionally specify an epsilon value for running the RDP Simplification
    /// algorithm, in lat lon. this value should roughly match what would be acceptable smoothing
    /// for the human eye. for example, 0.00135 is approx. 10-15 meters.
    ConcaveHull {
        concavity: f32,
        simplify_epsilon_latlon: Option<f32>,
    },
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
            IsochroneAlgorithm::ConcaveHull {
                concavity,
                simplify_epsilon_latlon,
            } => {
                if mp.len() < 3 {
                    Ok(Geometry::Polygon(geo::polygon!()))
                } else {
                    let options = ConcaveHullOptions::default().concavity(*concavity);
                    let mut hull = mp.concave_hull_with_options(options);
                    if let Some(epsilon) = simplify_epsilon_latlon {
                        hull = hull.simplify(*epsilon);
                    }
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
