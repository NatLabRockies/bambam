//! # Opportunity
//!
//! The modules below provide modeling for activities and processing
//! search destinations into opportunities.
pub mod opportunity_iterator;
mod opportunity_model;
mod opportunity_model_config;
mod opportunity_output_plugin;
mod opportunity_output_plugin_builder;
mod opportunity_plugin_config;
mod opportunity_record;
mod opportunity_source;
mod opportunity_spatial_row;
mod study_region;

pub mod source;
pub use opportunity_iterator::OpportunityIterator;
pub use opportunity_model::OpportunityModel;
pub use opportunity_model_config::OpportunityModelConfig;
pub use opportunity_output_plugin::OpportunityOutputPlugin;
pub use opportunity_output_plugin_builder::OpportunityOutputPluginBuilder;
pub use opportunity_plugin_config::OpportunityPluginConfig;
pub use opportunity_record::OpportunityRecord;
pub use opportunity_source::OpportunitySource;
pub use opportunity_spatial_row::OpportunitySpatialRow;
pub use study_region::StudyRegion;
