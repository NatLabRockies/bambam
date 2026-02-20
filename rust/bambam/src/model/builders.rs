use super::input_plugin::grid::grid_input_plugin_builder::GridInputPluginBuilder;
use super::traversal::fixed_speed::FixedSpeedBuilder;
use super::traversal::time_delay::TripArrivalDelayBuilder;
use super::traversal::time_delay::TripDepartureDelayBuilder;
use crate::model::constraint::multimodal::MultimodalConstraintBuilder;
use crate::model::constraint::time_limit::TimeLimitConstraintBuilder;
use crate::model::label::multimodal::MultimodalLabelBuilder;
use crate::model::output_plugin::finalize::finalize_output_plugin_builder::FinalizeOutputPluginBuilder;
use crate::model::output_plugin::h3_util::H3UtilOutputPluginBuilder;
use crate::model::output_plugin::isochrone::isochrone_output_plugin_builder::IsochroneOutputPluginBuilder;
use crate::model::output_plugin::opportunity::OpportunityOutputPluginBuilder;
use crate::model::traversal::multimodal::MultimodalTraversalBuilder;
use crate::model::traversal::switch::switch_traversal_builder::SwitchTraversalBuilder;
use crate::model::traversal::transit::TransitTraversalBuilder;
use bambam_gbfs::model::constraint::boarding::BoardingConstraintBuilder;
use bambam_gbfs::model::constraint::geofence::GeofenceConstraintBuilder;
use bambam_gbfs::model::traversal::boarding::BoardingTraversalBuilder;
use inventory;
use routee_compass::app::compass::BuilderRegistration;
use routee_compass::app::compass::CompassAppError;
use routee_compass_core::model::constraint::ConstraintModelBuilder;
use routee_compass_core::model::traversal::TraversalModelBuilder;
use std::collections::HashMap;
use std::rc::Rc;

/// builders to inject into the CompassBuilderInventory on library load via the inventory crate
pub const BUILDER_REGISTRATION: BuilderRegistration = BuilderRegistration(|builders| {
    builders.add_label_model("multimodal".to_string(), Rc::new(MultimodalLabelBuilder {}));

    builders.add_traversal_model(String::from("fixed_speed"), Rc::new(FixedSpeedBuilder {}));
    builders.add_traversal_model(
        String::from("departure"),
        Rc::new(TripDepartureDelayBuilder {}),
    );
    builders.add_traversal_model(String::from("arrival"), Rc::new(TripArrivalDelayBuilder {}));
    builders.add_traversal_model(
        String::from("multimodal"),
        Rc::new(MultimodalTraversalBuilder {}),
    );

    builders.add_traversal_model(String::from("transit"), Rc::new(TransitTraversalBuilder {}));
    builders.add_constraint_model(
        "gbfs_geofence".to_string(),
        Rc::new(GeofenceConstraintBuilder {}),
    );
    builders.add_constraint_model(
        "gbfs_boarding".to_string(),
        Rc::new(BoardingConstraintBuilder {}),
    );
    builders.add_traversal_model(
        "gbfs_boarding".to_string(),
        Rc::new(BoardingTraversalBuilder {}),
    );

    builders.add_constraint_model(
        "multimodal".to_string(),
        Rc::new(MultimodalConstraintBuilder {}),
    );
    builders.add_constraint_model(
        String::from("time_limit"),
        Rc::new(TimeLimitConstraintBuilder {}),
    );

    builders.add_input_plugin(String::from("grid"), Rc::new(GridInputPluginBuilder {}));

    builders.add_output_plugin("h3".to_string(), Rc::new(H3UtilOutputPluginBuilder {}));
    builders.add_output_plugin(
        String::from("isochrone"),
        Rc::new(IsochroneOutputPluginBuilder {}),
    );
    builders.add_output_plugin(
        String::from("opportunity"),
        Rc::new(OpportunityOutputPluginBuilder {}),
    );
    builders.add_output_plugin(
        String::from("finalize"),
        Rc::new(FinalizeOutputPluginBuilder {}),
    );
    Ok(())
});
