use std::{fs::File, path::Path};

use csv::QuoteStyle;
use flate2::{write::GzEncoder, Compression};
use geo::{Convert, LineString};
use geozero::ToWkt;
use kdam::tqdm;
use routee_compass_core::model::network::{EdgeConfig, Vertex};
use serde::Serialize;

use crate::{
    consts::*,
    error::NetworkImportError,
    serialize_ops::VectorizedEdgeList,
    vertex_serializable::{OmfVertexComplete, VertexMappingRecord, VertexSerializable},
};

/// helper function to "mkdir -p path" - make all directories along a path
pub fn create_dirs<P>(path: P) -> Result<(), NetworkImportError>
where
    P: AsRef<Path>,
{
    let dirspath = path.as_ref();
    if !dirspath.is_dir() {
        std::fs::create_dir_all(dirspath).map_err(|e| {
            let msg = format!(
                "error building output directory '{}': {e}",
                dirspath.to_str().unwrap_or_default()
            );
            NetworkImportError::InvalidUserInput(msg)
        })
    } else {
        Ok(())
    }
}

pub fn serialize_into_csv<I>(
    iterable: I,
    filename: &str,
    output_directory: &Path,
    overwrite: bool,
    desc: &str,
) -> Result<(), NetworkImportError>
where
    I: IntoIterator,
    I::IntoIter: ExactSizeIterator,
    I::Item: Serialize,
{
    let mut writer: Option<csv::Writer<GzEncoder<File>>> = create_writer(
        output_directory,
        filename,
        true,
        QuoteStyle::Necessary,
        overwrite,
    );
    let iter = iterable.into_iter();
    let total = iter.len();
    let bar_iter = tqdm!(iter, total = total, desc = desc);
    for element in bar_iter {
        if let Some(ref mut writer) = writer {
            writer.serialize(element).map_err(|e| {
                NetworkImportError::CsvWriteError(format!("Failed to write to {filename}: {e}"))
            })?;
        }
    }
    eprintln!();
    if let Some(ref mut writer) = writer {
        writer.flush().map_err(|e| {
            NetworkImportError::CsvWriteError(format!("Failed to flush {filename}: {e}"))
        })?;
    };

    Ok(())
}

pub fn serialize_into_enumerated_txt<I>(
    iterable: I,
    filename: &str,
    output_directory: &Path,
    overwrite: bool,
    desc: &str,
) -> Result<(), NetworkImportError>
where
    I: IntoIterator,
    I::IntoIter: ExactSizeIterator,
    I::Item: Serialize,
{
    let mut writer: Option<csv::Writer<GzEncoder<File>>> = create_writer(
        output_directory,
        filename,
        false,
        QuoteStyle::Never,
        overwrite,
    );
    let iter = iterable.into_iter();
    let total = iter.len();
    let bar_iter = tqdm!(iter, total = total, desc = desc);
    for element in bar_iter {
        if let Some(ref mut writer) = writer {
            writer.serialize(element).map_err(|e| {
                NetworkImportError::CsvWriteError(format!("Failed to write to {filename}: {e}"))
            })?;
        }
    }
    eprintln!();
    if let Some(ref mut writer) = writer {
        writer.flush().map_err(|e| {
            NetworkImportError::CsvWriteError(format!("Failed to flush {filename}: {e}"))
        })?;
    };

    Ok(())
}

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
    let file = File::create(filepath).ok()?;
    let buffer = GzEncoder::new(file, Compression::default());
    let writer = csv::WriterBuilder::new()
        .has_headers(has_headers)
        .quote_style(quote_style)
        .from_writer(buffer);
    Some(writer)
}

/// Shared writer that outputs RouteE Compass vertices, complete vertices, ID mappings,
/// enumerated vertex infrastructure files, and per-mode edge lists.
#[allow(clippy::too_many_arguments)]
pub fn write_compass_dataset<C, F>(
    vertices: &[Vertex],
    vertices_complete: &[OmfVertexComplete],
    vertex_mappings: &[VertexMappingRecord],
    edge_lists: &[VectorizedEdgeList<C>],
    mode_names: &[String],
    class_to_str: F,
    output_directory: &Path,
    overwrite: bool,
    export_legacy_omf_ids: bool,
) -> Result<(), NetworkImportError>
where
    F: Fn(&C) -> String,
{
    create_dirs(output_directory)?;

    serialize_into_csv(
        vertices.iter().map(|v| VertexSerializable::from(*v)),
        COMPASS_VERTEX_FILENAME,
        output_directory,
        overwrite,
        "write vertex dataset",
    )?;

    serialize_into_csv(
        vertices_complete.iter(),
        COMPLETE_VERTEX_FILENAME,
        output_directory,
        overwrite,
        "write complete vertex dataset",
    )?;

    serialize_into_csv(
        vertex_mappings.iter(),
        VERTEX_MAPPING_FILENAME,
        output_directory,
        overwrite,
        "write vertex ID mappings",
    )?;

    serialize_into_enumerated_txt(
        vertices_complete.iter().map(|v| v.traffic_signals),
        VERTEX_TRAFFIC_SIGNALS_FILENAME,
        output_directory,
        overwrite,
        "write vertex traffic signals",
    )?;
    serialize_into_enumerated_txt(
        vertices_complete.iter().map(|v| v.stop),
        VERTEX_STOP_FILENAME,
        output_directory,
        overwrite,
        "write vertex stop signs",
    )?;
    serialize_into_enumerated_txt(
        vertices_complete.iter().map(|v| v.give_way),
        VERTEX_GIVE_WAY_FILENAME,
        output_directory,
        overwrite,
        "write vertex give way signs",
    )?;
    serialize_into_enumerated_txt(
        vertices_complete.iter().map(|v| v.crossing),
        VERTEX_CROSSING_FILENAME,
        output_directory,
        overwrite,
        "write vertex crossings",
    )?;

    if export_legacy_omf_ids {
        let connectors_omf_ids = vertex_mappings
            .iter()
            .map(|m| m.source_id.clone())
            .collect::<Vec<String>>();

        serialize_into_enumerated_txt(
            &connectors_omf_ids,
            OMF_CONNECTOR_IDS_FILENAME,
            output_directory,
            overwrite,
            "write connector OMF ids",
        )?;
    }

    let edge_list_iter = tqdm!(
        edge_lists.iter().zip(mode_names.iter()),
        desc = "edge list",
        total = edge_lists.len(),
        position = 0
    );
    for (edge_list, mode_str) in edge_list_iter {
        let mode_dir = output_directory.join(mode_str);
        create_dirs(&mode_dir)?;

        serialize_into_csv(
            edge_list.edges.0.iter().map(|row| EdgeConfig {
                edge_id: row.edge_id,
                src_vertex_id: row.src_vertex_id,
                dst_vertex_id: row.dst_vertex_id,
                distance: row.distance.get::<uom::si::length::meter>(),
            }),
            COMPASS_EDGES_FILENAME,
            &mode_dir,
            overwrite,
            "write edges",
        )?;

        serialize_into_csv(
            edge_list.edge_mappings.iter(),
            EDGE_MAPPING_FILENAME,
            &mode_dir,
            overwrite,
            "write edge ID mappings",
        )?;

        serialize_into_enumerated_txt(
            edge_list.geometries.iter().map(|row| {
                let row_f64: LineString<f64> = row.convert();
                geo::Geometry::from(row_f64).to_wkt().unwrap_or_default()
            }),
            GEOMETRIES_FILENAME,
            &mode_dir,
            overwrite,
            "write geometries",
        )?;

        serialize_into_enumerated_txt(
            &edge_list.speeds,
            SPEEDS_FILENAME,
            &mode_dir,
            overwrite,
            "write speeds",
        )?;

        serialize_into_enumerated_txt(
            edge_list.classes.iter().map(&class_to_str),
            CLASSES_FILENAME,
            &mode_dir,
            overwrite,
            "write classes",
        )?;

        serialize_into_csv(
            edge_list.speed_lookup.iter(),
            SPEED_MAPPING_FILENAME,
            &mode_dir,
            overwrite,
            "write speed mapping",
        )?;

        serialize_into_enumerated_txt(
            &edge_list.bearings,
            BEARINGS_FILENAME,
            &mode_dir,
            overwrite,
            "write bearings",
        )?;

        if export_legacy_omf_ids {
            serialize_into_csv(
                &edge_list.omf_segment_ids,
                OMF_SEGMENT_IDS_FILENAME,
                &mode_dir,
                overwrite,
                "write omf ids",
            )?;
        }
    }
    eprintln!();

    Ok(())
}
