// use bamsoda_core::model::identifier::Geoid;
use flate2::read::GzDecoder;
use geo::Geometry;
use geozero::{wkt, ToGeo};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs::File, io::BufReader};

/// source of overlay geometry dataset
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum OverlaySource {
    /// reads overlay geometries from a CSV file that contains geometry and id columns
    Csv {
        file: String,
        geometry_column: String,
        id_column: String,
    },
    /// reads overlay geometries from a shapefile with an id field
    Shapefile { file: String, id_field: String },
    /// uses bamsoda-tiger to retrieve the geometries associated with the given geoids from the web
    TigerLines { geoids: Vec<String> },
}

impl OverlaySource {
    pub fn build(&self) -> Result<Vec<(Geometry, String)>, String> {
        match self {
            OverlaySource::Csv {
                file,
                geometry_column,
                id_column,
            } => read_overlay_csv(file, geometry_column, id_column),
            OverlaySource::Shapefile { file, id_field } => read_overlay_shapefile(file, id_field),
            OverlaySource::TigerLines { .. } => {
                todo!("not yet implemented, requires improvements to bamcensus")
            }
        }
    }
}

/// reads geometries and Strings from a shapefile source
fn read_overlay_shapefile(
    overlay_filepath: &str,
    id_field: &str,
) -> Result<Vec<(Geometry, String)>, String> {
    let rows = shapefile::read(overlay_filepath)
        .map_err(|e| format!("failed reading '{overlay_filepath}': {e}"))?;

    let mut processed = vec![];
    for (idx, (shape, record)) in rows.into_iter().enumerate() {
        let geometry = match shape {
            shapefile::Shape::Polygon(generic_polygon) => {
                let mp: geo::MultiPolygon<f64> = generic_polygon.try_into().map_err(|e| {
                    format!("failed to convert shapefile polygon at row {idx}: {e}")
                })?;
                geo::Geometry::MultiPolygon(mp)
            }
            shapefile::Shape::PolygonM(generic_polygon) => {
                let mp: geo::MultiPolygon<f64> = generic_polygon.try_into().map_err(|e| {
                    format!("failed to convert shapefile polygon at row {idx}: {e}")
                })?;
                geo::Geometry::MultiPolygon(mp)
            }
            _ => {
                return Err(format!(
                    "unexpected shape type {} found at row {}, must be polygonal",
                    shape.shapetype(),
                    idx
                ))
            }
        };
        let field = record
            .get(id_field)
            .ok_or_else(|| format!("field {id_field} missing from shapefile record"))?;
        let geoid = match field {
            shapefile::dbase::FieldValue::Character(Some(s)) => Ok(s.clone()),
            shapefile::dbase::FieldValue::Numeric(Some(f)) => Ok(format!("{}", *f as i64)),
            _ => Err(format!(
                "field '{}' has unexpected field type '{}'",
                id_field,
                field.field_type()
            )),
        }?;
        processed.push((geometry, geoid));
    }
    Ok(processed)
}

/// reads geometries and Strings from a CSV source
fn read_overlay_csv(
    overlay_filepath: &str,
    geometry_column: &str,
    id_column: &str,
) -> Result<Vec<(Geometry, String)>, String> {
    // read in overlay geometries file
    // let overlay_path = Path::new(overlay_filepath);
    let overlay_file = File::open(overlay_filepath)
        .map_err(|e| format!("failure reading file {overlay_filepath}: {e}"))?;
    let r: Box<dyn std::io::Read> = if overlay_filepath.ends_with(".gz") {
        Box::new(BufReader::new(GzDecoder::new(overlay_file)))
    } else {
        Box::new(overlay_file)
    };
    let mut overlay_reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .trim(csv::Trim::Fields)
        .from_reader(r);

    // let mut overlay_reader = csv::Reader::from_path(overlay_path).map_err(|e| e.to_string())?;
    let overlay_header_record = overlay_reader.headers().map_err(|e| e.to_string())?.clone();
    let overlay_headers = overlay_header_record
        .into_iter()
        .enumerate()
        .map(|(i, s)| (s, i))
        .collect::<HashMap<_, _>>();
    let overlay_geom_idx = overlay_headers
        .get(geometry_column)
        .ok_or_else(|| format!("overlay file missing {geometry_column} column"))?;
    let overlay_id_idx = overlay_headers
        .get(id_column)
        .ok_or_else(|| format!("overlay file missing {id_column} column"))?;

    let overlay_data = overlay_reader
        .records()
        .enumerate()
        .map(|(idx, r)| {
            let row = r.map_err(|e| e.to_string())?;
            let geometry_str = row
                .get(*overlay_geom_idx)
                .ok_or_else(|| format!("row {idx} missing geometry index"))?;
            let geometry = wkt::Wkt(geometry_str).to_geo().map_err(|e| e.to_string())?;
            let id = row
                .get(*overlay_id_idx)
                .ok_or_else(|| format!("row {idx} missing id index"))?
                .to_string();

            match geometry {
                Geometry::Point(_) => Err(format!(
                    "unexpected Point geometry type for row {idx} with id {id}"
                )),
                Geometry::Line(_) => Err(format!(
                    "unexpected Line geometry type for row {idx} with id {id}"
                )),
                Geometry::LineString(_) => Err(format!(
                    "unexpected LineString geometry type for row {idx} with id {id}"
                )),
                Geometry::Polygon(_) => Ok(()),
                Geometry::MultiPoint(_) => Err(format!(
                    "unexpected MultiPoint geometry type for row {idx} with id {id}"
                )),
                Geometry::MultiLineString(_) => Err(format!(
                    "unexpected MultiLineString geometry type for row {idx} with id {id}"
                )),
                Geometry::MultiPolygon(_) => Ok(()),
                Geometry::GeometryCollection(_) => Err(format!(
                    "unexpected GeometryCollection geometry type for row {idx} with id {id}"
                )),
                Geometry::Rect(_) => Err(format!(
                    "unexpected Rect geometry type for row {idx} with id {id}"
                )),
                Geometry::Triangle(_) => Err(format!(
                    "unexpected Triangle geometry type for row {idx} with id {id}"
                )),
            }?;

            Ok((geometry, id))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(overlay_data)
}
