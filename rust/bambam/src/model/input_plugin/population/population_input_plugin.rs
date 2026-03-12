use crate::model::input_plugin::grid;
use geo::Geometry;
use geozero::{wkt::Wkt as WktReader, ToGeo};
use routee_compass::{
    app::search::SearchApp,
    plugin::input::{InputPlugin, InputPluginError},
};
use routee_compass_core::config::ConfigJsonExtensions;
use std::sync::Arc;

pub struct PopulationInputPlugin {}

impl InputPlugin for PopulationInputPlugin {
    fn process(
        &self,
        input: &mut serde_json::Value,
        _: Arc<SearchApp>,
    ) -> Result<(), InputPluginError> {
        let geom_wkt = input.get_config_string(&grid::GEOMETRY, &"").map_err(|e| {
            InputPluginError::InputPluginFailed(format!(
                "failure reading `geometry` from grid row: {e}"
            ))
        })?;
        let _geometry: Geometry = WktReader(geom_wkt.as_str()).to_geo().map_err(|e| {
            InputPluginError::InputPluginFailed(format!(
                "failure reading `geometry` from grid row: {e}"
            ))
        })?;

        todo!("not yet implemented, Population modeling is called from the Grid plugin")
    }
}
