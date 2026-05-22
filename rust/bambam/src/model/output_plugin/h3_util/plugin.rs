use jsonpath_rust::JsonPath;
use routee_compass::{
    app::{
        compass::CompassAppError,
        search::{SearchApp, SearchAppResult},
    },
    plugin::output::{OutputPlugin, OutputPluginError},
};
use routee_compass_core::algorithm::search::SearchInstance;
use serde_json::Value;
use std::sync::Arc;

use crate::model::output_plugin::h3_util::{DotDelimitedPath, H3Util, H3UtilOutputPluginConfig};

pub struct H3UtilOutputPlugin {
    util: H3Util,
}

impl H3UtilOutputPlugin {
    pub fn new(util: H3Util) -> H3UtilOutputPlugin {
        H3UtilOutputPlugin { util }
    }
}

impl OutputPlugin for H3UtilOutputPlugin {
    fn process(
        &self,
        output: &mut serde_json::Value,
        result: &Result<(SearchAppResult, SearchInstance), CompassAppError>,
    ) -> Result<(), OutputPluginError> {
        self.util.apply(output)
    }

    fn name(&self) -> &str {
        "h3"
    }
}
