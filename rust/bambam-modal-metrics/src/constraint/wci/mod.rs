//! WCI Constraint Model
//!
//! Filters edges during walk search based on a Walk Comfort Index (WCI) threshold.
//! Edges with WCI values below the configured threshold are rejected.

mod builder;
mod config;
mod engine;
mod model;
mod params;
mod service;

pub use builder::WciBuilder;
pub use config::WciConfig;
pub use engine::WciEngine;
pub use model::WciModel;
pub use params::WciParams;
pub use service::WciService;
