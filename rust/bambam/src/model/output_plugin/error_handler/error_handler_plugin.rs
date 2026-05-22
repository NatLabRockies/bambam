use routee_compass::app::{compass::CompassAppError, search::SearchAppResult};
use routee_compass::plugin::output::OutputPlugin;
use routee_compass::plugin::output::OutputPluginError;
use routee_compass::plugin::PluginError;
use routee_compass_core::algorithm::search::SearchInstance;
pub struct ErrorHandlerPlugin {}

impl OutputPlugin for ErrorHandlerPlugin {
    /// handles errors by injecting some default output
    fn process(
        &self,
        _output: &mut serde_json::Value,
        result: &Result<(SearchAppResult, SearchInstance), CompassAppError>,
    ) -> Result<(), OutputPluginError> {
        match result {
            Ok(_) => Ok(()),
            Err(e) => match e {
                CompassAppError::PluginError(pe) => match pe {
                    PluginError::BuildFailed(_) => todo!(),
                    PluginError::MissingExpectedQueryField(_, _) => todo!(),
                    PluginError::InputPluginFailed { source } => todo!(),
                    PluginError::OutputPluginFailed { source } => todo!(),
                    PluginError::JsonError { source } => todo!(),
                    PluginError::UnexpectedQueryStructure(_) => todo!(),
                    PluginError::InternalError(_) => todo!(),
                },
                _ => Ok(()),
            },
        }
    }

    fn name(&self) -> &str {
        "bambam_error"
    }
}
