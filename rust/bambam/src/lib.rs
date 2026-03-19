#![allow(unused)]
pub mod app;
pub mod model;

// dependency injection. adds access modeling extensions to compass.
use crate::model::builders;
inventory::submit! { builders::BUILDER_REGISTRATION }
