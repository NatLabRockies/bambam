use std::path::Path;

use bambam_network::{fs::write_compass_dataset, write_summary, SegmentFullType};

use crate::model::osm::OsmError;

use super::OsmGraphVectorized;

pub trait CompassWriter {
    /// vectorizes data into CSV and TXT files in a shared directory
    /// to be consumed by RouteE Compass, matching the `bambam-omf` output layout.
    fn write_compass(&self, output_directory: &Path, overwrite: bool) -> Result<(), OsmError>;
}

impl CompassWriter for OsmGraphVectorized {
    fn write_compass(&self, output_directory: &Path, overwrite: bool) -> Result<(), OsmError> {
        bambam_network::fs::create_dirs(output_directory)?;

        // Always retain raw OSM nodes and ways as Parquet files in `<output_directory>/raw/`
        if let Some(ref raw) = self.raw_collection {
            raw.to_parquet(output_directory)?;
        }

        // Write `summary.toml` matching `bambam-omf` (echoing `output_directory` as `study_region`)
        let mut summary = self.to_summary();
        summary.source.study_region = output_directory.to_string_lossy().into_owned();
        write_summary(output_directory, &summary)?;

        let mode_names: Vec<String> = self
            .edge_list_config
            .iter()
            .map(|c| c.mode.clone())
            .collect();

        write_compass_dataset(
            &self.vertices,
            &self.vertices_complete,
            &self.vertex_mappings,
            &self.edge_lists,
            &mode_names,
            |class: &SegmentFullType| class.as_str(),
            output_directory,
            overwrite,
        )?;

        Ok(())
    }
}
