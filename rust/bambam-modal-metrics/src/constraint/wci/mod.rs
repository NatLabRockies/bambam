//! Wci Constraint Model
//!
//! A stubbed version of a constraint model module that compiles. Used in codegen.
//! If code changes in Compass lead to compiler errors in this module, the changes
//! should get updated.

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
