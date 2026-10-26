use std::path::Path;

use bambam_network::{
    fs::{serialize_into_csv, serialize_into_enumerated_txt},
    VertexSerializable,
};
use geo::Convert;
use geozero::ToWkt;
use routee_compass_core::model::network::{EdgeConfig, EdgeId, Vertex};

use crate::model::{feature::highway::Highway, osm::OsmError};

use super::OsmGraphVectorized;

pub trait CompassWriter {
    /// vectorizes data into CSV and TXT files in a shared directory
    /// to be consumed by RouteE Compass.
    fn write_compass(&self, output_directory: &Path, overwrite: bool) -> Result<(), OsmError>;
}

mod filenames {
    pub const VERTICES_COMPLETE: &str = "vertices-complete.csv.gz";
    pub const VERTICES_COMPASS: &str = "vertices-compass.csv.gz";
    pub const VERTICES_MAPPING: &str = "vertices-mapping.csv.gz";
    pub const EDGES_COMPLETE: &str = "edges-complete.csv.gz";
    pub const EDGES_COMPASS: &str = "edges-compass.csv.gz";
    pub const EDGES_MAPPING: &str = "edges-mapping.csv.gz";
    pub const GEOMETRIES_ENUMERATED: &str = "edges-geometries-enumerated.txt.gz";
    pub const MAXSPEEDS_AVGFILL: &str = "speed-maxspeed-avgfill-enumerated.txt.gz";
    pub const HIGHWAY_TAG: &str = "edges-highway-tag-enumerated.txt.gz";
}

impl CompassWriter for OsmGraphVectorized {
    fn write_compass(&self, output_directory: &Path, overwrite: bool) -> Result<(), OsmError> {
        bambam_network::fs::create_dirs(output_directory)?;

        // Always retain raw OSM nodes and ways as Parquet files in `<output_directory>/raw/`
        if let Some(ref raw) = self.raw_collection {
            raw.to_parquet(output_directory)?;
        }

        // Write standardized vertex mappings (`vertices-mapping.csv.gz`)
        if !self.vertex_mappings.is_empty() {
            serialize_into_csv(
                self.vertex_mappings.iter(),
                filenames::VERTICES_MAPPING,
                output_directory,
                overwrite,
                "write vertex ID mappings",
            )?;
        }

        // Write enumerated vertex infrastructure files
        if !self.vertices_complete.is_empty() {
            serialize_into_enumerated_txt(
                self.vertices_complete.iter().map(|v| v.traffic_signals),
                bambam_network::VERTEX_TRAFFIC_SIGNALS_FILENAME,
                output_directory,
                overwrite,
                "write vertex traffic signals",
            )?;
            serialize_into_enumerated_txt(
                self.vertices_complete.iter().map(|v| v.stop),
                bambam_network::VERTEX_STOP_FILENAME,
                output_directory,
                overwrite,
                "write vertex stop signs",
            )?;
            serialize_into_enumerated_txt(
                self.vertices_complete.iter().map(|v| v.give_way),
                bambam_network::VERTEX_GIVE_WAY_FILENAME,
                output_directory,
                overwrite,
                "write vertex give way signs",
            )?;
            serialize_into_enumerated_txt(
                self.vertices_complete.iter().map(|v| v.crossing),
                bambam_network::VERTEX_CROSSING_FILENAME,
                output_directory,
                overwrite,
                "write vertex crossings",
            )?;
        }

        // Write standardized edge mappings (`edges-mapping.csv.gz`) and shared OMF-compatible edge files
        if !self.edge_mappings.is_empty() {
            serialize_into_csv(
                self.edge_mappings.iter(),
                filenames::EDGES_MAPPING,
                output_directory,
                overwrite,
                "write edge ID mappings",
            )?;
        }
        if let Some(ref edge_list) = self.edge_list {
            serialize_into_enumerated_txt(
                &edge_list.speeds,
                bambam_network::SPEEDS_FILENAME,
                output_directory,
                overwrite,
                "write speeds (mph)",
            )?;
            serialize_into_enumerated_txt(
                edge_list.classes.iter().map(|c| c.to_string()),
                bambam_network::CLASSES_FILENAME,
                output_directory,
                overwrite,
                "write classes",
            )?;
            serialize_into_csv(
                edge_list.speed_lookup.iter(),
                bambam_network::SPEED_MAPPING_FILENAME,
                output_directory,
                overwrite,
                "write speed mapping",
            )?;
            serialize_into_enumerated_txt(
                &edge_list.bearings,
                bambam_network::BEARINGS_FILENAME,
                output_directory,
                overwrite,
                "write bearings",
            )?;
            serialize_into_enumerated_txt(
                edge_list.speeds.iter().map(|mph| {
                    uom::si::f64::Velocity::new::<uom::si::velocity::mile_per_hour>(*mph)
                }),
                filenames::MAXSPEEDS_AVGFILL,
                output_directory,
                overwrite,
                "write maxspeed avgfill",
            )?;
        }

        // Write OSM vertices-complete.csv.gz and vertices-compass.csv.gz
        serialize_into_csv(
            self.nodes.iter(),
            filenames::VERTICES_COMPLETE,
            output_directory,
            overwrite,
            "write complete vertex dataset",
        )?;

        let compass_vertices: Vec<VertexSerializable> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(vid, n)| VertexSerializable::from(Vertex::new(vid, n.x, n.y)))
            .collect();
        serialize_into_csv(
            compass_vertices.iter(),
            filenames::VERTICES_COMPASS,
            output_directory,
            overwrite,
            "write compass vertex dataset",
        )?;

        // Write OSM edges-complete.csv.gz, edges-compass.csv.gz, geometries, and highway tags
        serialize_into_csv(
            self.ways.iter(),
            filenames::EDGES_COMPLETE,
            output_directory,
            overwrite,
            "write complete edges dataset",
        )?;

        serialize_into_csv(
            self.ways
                .iter()
                .enumerate()
                .map(|(edge_id, row)| EdgeConfig {
                    edge_id: EdgeId(edge_id),
                    src_vertex_id: row.src_vertex_id,
                    dst_vertex_id: row.dst_vertex_id,
                    distance: row.length_meters,
                }),
            filenames::EDGES_COMPASS,
            output_directory,
            overwrite,
            "write compass edges dataset",
        )?;

        serialize_into_enumerated_txt(
            self.ways.iter().map(|row| {
                let ls_f64: geo::LineString<f64> = row.linestring.convert();
                geo::Geometry::from(ls_f64).to_wkt().unwrap_or_default()
            }),
            filenames::GEOMETRIES_ENUMERATED,
            output_directory,
            overwrite,
            "write edge geometries",
        )?;

        serialize_into_enumerated_txt(
            self.ways.iter().map(|row| &row.highway),
            filenames::HIGHWAY_TAG,
            output_directory,
            overwrite,
            "write edge highway tags",
        )?;

        // If mode-specific edge lists were generated, write `<mode>/` subdirectories matching OMF layout
        for (mode_str, mode_list) in &self.mode_edge_lists {
            let mode_dir = output_directory.join(mode_str);
            bambam_network::fs::create_dirs(&mode_dir)?;

            serialize_into_csv(
                mode_list.edges.0.iter().map(|row| EdgeConfig {
                    edge_id: row.edge_id,
                    src_vertex_id: row.src_vertex_id,
                    dst_vertex_id: row.dst_vertex_id,
                    distance: row.distance.get::<uom::si::length::meter>(),
                }),
                bambam_network::COMPASS_EDGES_FILENAME,
                &mode_dir,
                overwrite,
                "write mode edges",
            )?;

            serialize_into_csv(
                mode_list.edge_mappings.iter(),
                bambam_network::EDGE_MAPPING_FILENAME,
                &mode_dir,
                overwrite,
                "write mode edge ID mappings",
            )?;

            serialize_into_enumerated_txt(
                mode_list.geometries.iter().map(|row| {
                    let row_f64: geo::LineString<f64> = row.convert();
                    geo::Geometry::from(row_f64).to_wkt().unwrap_or_default()
                }),
                bambam_network::GEOMETRIES_FILENAME,
                &mode_dir,
                overwrite,
                "write mode geometries",
            )?;

            serialize_into_enumerated_txt(
                &mode_list.speeds,
                bambam_network::SPEEDS_FILENAME,
                &mode_dir,
                overwrite,
                "write mode speeds",
            )?;

            serialize_into_enumerated_txt(
                mode_list.classes.iter().map(Highway::to_string),
                bambam_network::CLASSES_FILENAME,
                &mode_dir,
                overwrite,
                "write mode classes",
            )?;

            serialize_into_csv(
                mode_list.speed_lookup.iter(),
                bambam_network::SPEED_MAPPING_FILENAME,
                &mode_dir,
                overwrite,
                "write mode speed mapping",
            )?;

            serialize_into_enumerated_txt(
                &mode_list.bearings,
                bambam_network::BEARINGS_FILENAME,
                &mode_dir,
                overwrite,
                "write mode bearings",
            )?;
        }

        Ok(())
    }
}
