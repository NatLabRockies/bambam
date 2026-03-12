use std::{fs::File, path::Path};

use csv::QuoteStyle;
use flate2::{write::GzEncoder, Compression};
use geo::Convert;
use geozero::ToWkt;
use kdam::tqdm;
use routee_compass_core::model::network::{EdgeConfig, EdgeId};

use crate::model::osm::{
    graph::{
        fill_value_lookup::FillValueLookup, osm_way_data_serializable::OsmWayDataSerializable,
        vertex_serializable::VertexSerializable,
    },
    OsmError,
};

use super::OsmGraphVectorized;

pub trait CompassWriter {
    /// vectorizes data into CSV and TXT files in a shared directory
    /// to be consumed by RouteE Compass.
    fn write_compass(&self, output_directory: &Path, overwrite: bool) -> Result<(), OsmError>;
}

mod filenames {
    pub const VERTICES_COMPLETE: &str = "vertices-complete.csv.gz";
    pub const VERTICES_COMPASS: &str = "vertices-compass.csv.gz";
    pub const EDGES_COMPLETE: &str = "edges-complete.csv.gz";
    pub const EDGES_COMPASS: &str = "edges-compass.csv.gz";
    pub const GEOMETRIES_ENUMERATED: &str = "edges-geometries-enumerated.txt.gz";
    pub const MAXSPEEDS_AVGFILL: &str = "speed-maxspeed-avgfill-enumerated.txt.gz";
    pub const HIGHWAY_TAG: &str = "edges-highway-tag-enumerated.txt.gz";
}

impl CompassWriter for OsmGraphVectorized {
    fn write_compass(&self, output_directory: &Path, overwrite: bool) -> Result<(), OsmError> {
        if !output_directory.is_dir() && std::fs::create_dir(output_directory).is_err() {
            let dirname = output_directory.as_os_str().to_string_lossy();
            return Err(OsmError::InternalError(format!(
                "unable to create directory {}",
                &dirname
            )));
        }

        let mut node_writer = create_writer(
            output_directory,
            filenames::VERTICES_COMPLETE,
            true,
            QuoteStyle::Necessary,
            overwrite,
        );
        let mut vertex_writer = create_writer(
            output_directory,
            filenames::VERTICES_COMPASS,
            true,
            QuoteStyle::Necessary,
            overwrite,
        );
        let mut way_writer = create_writer(
            output_directory,
            filenames::EDGES_COMPLETE,
            true,
            QuoteStyle::Necessary,
            overwrite,
        );
        let mut edge_writer = create_writer(
            output_directory,
            filenames::EDGES_COMPASS,
            true,
            QuoteStyle::Necessary,
            overwrite,
        );
        let mut geometries_writer = create_writer(
            output_directory,
            filenames::GEOMETRIES_ENUMERATED,
            false,
            QuoteStyle::Never,
            overwrite,
        );
        let mut highway_writer = create_writer(
            output_directory,
            filenames::HIGHWAY_TAG,
            false,
            QuoteStyle::Never,
            overwrite,
        );

        let mut maxspeed_writer = create_writer(
            output_directory,
            filenames::MAXSPEEDS_AVGFILL,
            false,
            QuoteStyle::Necessary,
            overwrite,
        );

        let v_iter = tqdm!(
            self.nodes.iter().enumerate(),
            total = self.nodes.len(),
            desc = "write vertex dataset"
        );
        for (_, node) in v_iter {
            if let Some(ref mut writer) = node_writer {
                writer.serialize(node).map_err(|e| {
                    OsmError::CsvWriteError(String::from(filenames::VERTICES_COMPLETE), e)
                })?;
            }
            if let Some(ref mut writer) = vertex_writer {
                let (_, vertex) = self.vertex_lookup.get(&node.osmid).ok_or_else(|| {
                    OsmError::InternalError(format!(
                        "node '{}' missing from vertex lookup",
                        node.osmid
                    ))
                })?;
                let vertex_ser = VertexSerializable::from(vertex);
                writer.serialize(vertex_ser).map_err(|e| {
                    OsmError::CsvWriteError(String::from(filenames::VERTICES_COMPASS), e)
                })?;
            }
        }
        eprintln!();

        // construct maxspeed fill value lookup
        let maxspeed_cb = |r: &OsmWayDataSerializable| {
            r.get_speed("maxspeed", true)
                .map_err(OsmError::InternalError)
                .map(|maxspeed_opt| {
                    maxspeed_opt
                        .map(|maxspeed| maxspeed.get::<uom::si::velocity::kilometer_per_hour>())
                })
        };
        let speed_lookup = FillValueLookup::new(&self.ways, "highway", "maxspeed", maxspeed_cb)?;

        let e_iter = tqdm!(
            self.ways.iter().enumerate(),
            total = self.ways.len(),
            desc = "write edges dataset"
        );
        for (edge_id, row) in e_iter {
            // COMPLETE
            if let Some(ref mut writer) = way_writer {
                writer.serialize(row).map_err(|e| {
                    OsmError::CsvWriteError(String::from(filenames::EDGES_COMPLETE), e)
                })?;
            }
            // COMPASS
            if let Some(ref mut writer) = edge_writer {
                let edge = EdgeConfig {
                    edge_id: EdgeId(edge_id),
                    src_vertex_id: row.src_vertex_id,
                    dst_vertex_id: row.dst_vertex_id,
                    distance: row.length_meters,
                };
                writer.serialize(edge).map_err(|e| {
                    OsmError::CsvWriteError(String::from(filenames::EDGES_COMPASS), e)
                })?;
            }
            // GEOMETRY
            if let Some(ref mut writer) = geometries_writer {
                writer
                    .serialize({
                        let ls_f64: geo::LineString<f64> = row.linestring.convert();
                        geo::Geometry::from(ls_f64).to_wkt().unwrap_or_default()
                    })
                    .map_err(|e| {
                        OsmError::CsvWriteError(String::from(filenames::GEOMETRIES_ENUMERATED), e)
                    })?;
            }

            // SPEED
            if let Some(ref mut writer) = maxspeed_writer {
                let speed = get_fill_value(row, &speed_lookup)?;
                writer.serialize(speed).map_err(|e| {
                    OsmError::CsvWriteError(String::from(filenames::MAXSPEEDS_AVGFILL), e)
                })?;
            }

            // HIGHWAY
            if let Some(ref mut writer) = highway_writer {
                writer.serialize(&row.highway).map_err(|e| {
                    OsmError::CsvWriteError(String::from(filenames::HIGHWAY_TAG), e)
                })?;
            }
        }
        eprintln!();

        Ok(())
    }
}

/// helper function to build a filewriter for writing either .csv.gz or
/// .txt.gz files for compass datasets while respecting the user's overwrite
/// preferences and properly formatting WKT outputs.
fn create_writer(
    directory: &Path,
    filename: &str,
    has_headers: bool,
    quote_style: QuoteStyle,
    overwrite: bool,
) -> Option<csv::Writer<GzEncoder<File>>> {
    let filepath = directory.join(filename);
    if filepath.exists() && !overwrite {
        return None;
    }
    let file = File::create(filepath).unwrap();
    let buffer = GzEncoder::new(file, Compression::default());
    let writer = csv::WriterBuilder::new()
        .has_headers(has_headers)
        .quote_style(quote_style)
        .from_writer(buffer);
    Some(writer)
}

fn get_fill_value(
    way: &OsmWayDataSerializable,
    maxspeeds_fill_lookup: &FillValueLookup,
) -> Result<uom::si::f64::Velocity, OsmError> {
    let highway_class = way
        .get_string_at_field("highway")
        .map_err(OsmError::GraphConsolidationError)?;
    let avg_speed = maxspeeds_fill_lookup.get(&highway_class);
    Ok(uom::si::f64::Velocity::new::<
        uom::si::velocity::kilometer_per_hour,
    >(avg_speed))
}
