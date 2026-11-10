use bambam_modal_metrics::{
    common::cycleway_tag::CyclewayTag,
    network_traits::{edge_for_modal_metric::EdgeForModalMetric, spatial_edge::SpatialEdge},
};
use geo::{Convert, LineString, MapCoords};
use geozero::{wkt::Wkt as WktReader, ToGeo, ToWkt};
use serde::{de, Deserialize, Serialize, Serializer};

use crate::{
    model::SegmentHeading,
    taxonomy::{SegmentClass, SegmentFullType, SegmentSubclass, SegmentSubtype},
};

/// Complete edge record combining RouteE Compass topology, source segment mapping,
/// Overture/Compass classification, speed limits, directionality, and WKT geometry
/// (`<mode>/edges-complete.csv.gz`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeCompleteRecord {
    pub edge_id: usize,
    pub source_id: String,
    pub start_lr: f64,
    pub end_lr: f64,
    pub heading: SegmentHeading,
    pub src_vertex_id: usize,
    pub dst_vertex_id: usize,
    pub distance: f64,
    pub class: SegmentFullType,
    pub posted_speed_mph: Option<f64>,
    pub speed_mph: f64,
    pub bearing: f64,
    pub oneway: bool,
    #[serde(
        serialize_with = "serialize_linestring_wkt",
        deserialize_with = "deserialize_linestring_wkt"
    )]
    pub linestring: LineString<f32>,
}

impl SpatialEdge for EdgeCompleteRecord {
    fn id(&self) -> String {
        self.source_id.clone()
    }

    fn linestring(&self) -> Option<&LineString<f32>> {
        Some(&self.linestring)
    }

    fn src_vertex_id(&self) -> usize {
        self.src_vertex_id
    }
}

impl EdgeForModalMetric for EdgeCompleteRecord {
    fn get_traffic_speed_limit(&self) -> Option<i32> {
        self.posted_speed_mph.map(|mph| mph.round() as i32)
    }

    fn get_cycleway_tag(&self) -> Option<CyclewayTag> {
        if matches!(self.class.1, SegmentClass::Cycleway) {
            return Some(CyclewayTag::DedicatedWithBuffer);
        }
        if matches!(self.class.2, Some(SegmentSubclass::CycleCrossing)) {
            return Some(CyclewayTag::NoDedicatedWithFacilities);
        }
        None
    }

    fn is_walkable(&self) -> bool {
        if !matches!(self.class.0, SegmentSubtype::Road) {
            return false;
        }
        self.is_sidewalk() || self.is_footway() || self.is_walkable_highway()
    }

    fn is_walkable_highway(&self) -> bool {
        if !matches!(self.class.0, SegmentSubtype::Road) {
            return false;
        }
        matches!(
            self.class.1,
            SegmentClass::Residential
                | SegmentClass::Unclassified
                | SegmentClass::LivingStreet
                | SegmentClass::Service
                | SegmentClass::Pedestrian
                | SegmentClass::Track
                | SegmentClass::Footway
                | SegmentClass::Bridleway
                | SegmentClass::Steps
                | SegmentClass::Path
                | SegmentClass::Cycleway
        )
    }

    fn is_sidewalk(&self) -> bool {
        matches!(
            self.class.2,
            Some(SegmentSubclass::Sidewalk | SegmentSubclass::Crosswalk)
        ) || matches!(&self.class.1, SegmentClass::Custom(s) if s == "sidewalk")
    }

    fn is_footway(&self) -> bool {
        matches!(self.class.1, SegmentClass::Footway)
    }

    fn is_pedestrian_priority(&self) -> bool {
        matches!(
            self.class.1,
            SegmentClass::LivingStreet | SegmentClass::Pedestrian
        )
    }

    fn is_unbikeable(&self) -> bool {
        if !matches!(self.class.0, SegmentSubtype::Road) {
            return true;
        }
        matches!(
            self.class.1,
            SegmentClass::Motorway | SegmentClass::Trunk | SegmentClass::Steps
        )
    }

    fn is_non_motorized(&self) -> bool {
        matches!(
            self.class.1,
            SegmentClass::Cycleway
                | SegmentClass::Path
                | SegmentClass::Footway
                | SegmentClass::Pedestrian
                | SegmentClass::LivingStreet
                | SegmentClass::Bridleway
        )
    }

    fn is_oneway(&self) -> bool {
        self.oneway
    }
}

/// Serializes a `LineString<f32>` as a WKT `LINESTRING(...)` string.
pub fn serialize_linestring_wkt<S>(row: &LineString<f32>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let row_f64: LineString<f64> = row.convert();
    let wkt = geo::Geometry::from(row_f64).to_wkt().unwrap_or_default();
    s.serialize_str(&wkt)
}

/// Deserializes a WKT `LINESTRING(...)` string into a `LineString<f32>`.
pub fn deserialize_linestring_wkt<'de, D>(d: D) -> Result<LineString<f32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct LineStringVisitor;

    impl<'de> de::Visitor<'de> for LineStringVisitor {
        type Value = LineString<f32>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a WKT LineString<f32>")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let cleaned = v.trim().trim_matches('"');
            let geometry_f64 = WktReader(cleaned)
                .to_geo()
                .map_err(|e| de::Error::custom(format!("failed to parse WKT string: {e}")))?;
            match geometry_f64 {
                geo::Geometry::LineString(ls) => Ok(ls.map_coords(|c| geo::Coord {
                    x: c.x as f32,
                    y: c.y as f32,
                })),
                _ => Err(de::Error::custom(
                    "expected a LINESTRING, got a different geometry type",
                )),
            }
        }

        fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.visit_str(&v)
        }
    }

    d.deserialize_str(LineStringVisitor)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::VertexCompleteRecord;
    use bambam_modal_metrics::{
        common::edge_rtree_entry::EdgeRTreeEntry,
        lts::{compute_lts::compute_lts, lts::Lts},
        wci::{compute_wci::compute_wci, wci::Wci},
    };
    use rstar::RTree;

    fn sample_edge(
        edge_id: usize,
        source_id: &str,
        class_str: &str,
        posted_speed_mph: Option<f64>,
        oneway: bool,
        coords: Vec<(f32, f32)>,
    ) -> EdgeCompleteRecord {
        EdgeCompleteRecord {
            edge_id,
            source_id: source_id.to_string(),
            start_lr: 0.0,
            end_lr: 1.0,
            heading: SegmentHeading::Forward,
            src_vertex_id: 0,
            dst_vertex_id: 1,
            distance: 100.0,
            class: SegmentFullType::from_str(class_str).unwrap(),
            posted_speed_mph,
            speed_mph: posted_speed_mph.unwrap_or(25.0),
            bearing: 90.0,
            oneway,
            linestring: LineString::from(coords),
        }
    }

    #[test]
    fn test_edge_complete_record_predicates_and_csv_roundtrip() {
        let sidewalk = sample_edge(
            0,
            "seg-sidewalk",
            "road-footway-sidewalk",
            None,
            false,
            vec![(-105.26, 40.015), (-105.259, 40.015)],
        );
        assert!(sidewalk.is_walkable());
        assert!(sidewalk.is_sidewalk());
        assert!(sidewalk.is_footway());
        assert!(sidewalk.is_non_motorized());
        assert!(!sidewalk.is_unbikeable());
        assert_eq!(sidewalk.id(), "seg-sidewalk");

        let mut wtr = csv::Writer::from_writer(vec![]);
        wtr.serialize(&sidewalk).unwrap();
        let csv_bytes = wtr.into_inner().unwrap();
        let mut rdr = csv::Reader::from_reader(csv_bytes.as_slice());
        let decoded: EdgeCompleteRecord = rdr.deserialize().next().unwrap().unwrap();
        assert_eq!(decoded, sidewalk);

        let motorway_link = sample_edge(
            1,
            "seg-mway-link",
            "road-motorway-link",
            Some(55.0),
            true,
            vec![(-105.26, 40.015), (-105.259, 40.015)],
        );
        assert!(!motorway_link.is_walkable());
        assert!(motorway_link.is_unbikeable());
        assert!(motorway_link.is_oneway());
        assert_eq!(motorway_link.get_traffic_speed_limit(), Some(55));
    }

    #[test]
    fn test_edge_complete_record_wci_and_lts() {
        let residential = sample_edge(
            0,
            "seg-res",
            "road-residential",
            Some(25.0),
            false,
            vec![(-105.170735, 39.773087), (-105.170445, 39.773137)],
        );
        let cycleway_neighbor = sample_edge(
            1,
            "seg-bike",
            "road-cycleway",
            None,
            false,
            vec![(-105.170612, 39.773116), (-105.170499, 39.773017)],
        );

        let src_vertex = VertexCompleteRecord {
            vertex_id: 0,
            connector_id: "conn-0".to_string(),
            x: -105.170735,
            y: 39.773087,
            traffic_signals: true,
            stop: false,
            give_way: false,
            crossing: true,
            infrastructure_ids: Some("infra-1".to_string()),
        };

        let res_entry = EdgeRTreeEntry::new(residential).unwrap();
        let bike_entry = EdgeRTreeEntry::new(cycleway_neighbor).unwrap();
        let rtree = RTree::bulk_load(vec![res_entry.clone(), bike_entry.clone()]);

        let wci = compute_wci(&rtree, &res_entry, Some(&src_vertex)).unwrap();
        assert_eq!(wci.walkability, Some(Wci::new(-2).unwrap()));
        assert_eq!(wci.traffic_speed_comfort, Some(Wci::new(2).unwrap()));
        assert_eq!(wci.cycleway_comfort, Some(Wci::new(2).unwrap()));
        assert_eq!(wci.traffic_signal_comfort, Some(Wci::new(2).unwrap()));
        assert_eq!(wci.total, Wci::new(4).unwrap());

        let lts_res = compute_lts(&rtree, &res_entry).unwrap();
        assert_eq!(lts_res, Lts::new(2).unwrap());

        let lts_bike = compute_lts(&rtree, &bike_entry).unwrap();
        assert_eq!(lts_bike, Lts::new(1).unwrap());
    }

    #[test]
    fn test_bulk_compute_modal_metric_with_gzipped_files_and_context() {
        use crate::fs::serialize_into_csv;
        use bambam_modal_metrics::common::{
            bulk_compute_modal_metric::bulk_compute_modal_metric_with_context,
            modal_metrics::ModalMetric,
        };
        use std::fs;
        use std::path::Path;

        let tmp_dir = Path::new("target/test_tmp_modal_metric_omf");
        let _ = fs::remove_dir_all(tmp_dir);
        let walk_dir = tmp_dir.join("walk");
        let drive_dir = tmp_dir.join("drive");
        let bike_dir = tmp_dir.join("bike");
        fs::create_dir_all(&walk_dir).unwrap();
        fs::create_dir_all(&drive_dir).unwrap();
        fs::create_dir_all(&bike_dir).unwrap();

        let vertices = vec![
            VertexCompleteRecord {
                vertex_id: 0,
                connector_id: "conn-0".to_string(),
                x: -105.170735,
                y: 39.773087,
                traffic_signals: false,
                stop: true,
                give_way: false,
                crossing: false,
                infrastructure_ids: None,
            },
            VertexCompleteRecord {
                vertex_id: 1,
                connector_id: "conn-1".to_string(),
                x: -105.170445,
                y: 39.773137,
                traffic_signals: false,
                stop: false,
                give_way: false,
                crossing: false,
                infrastructure_ids: None,
            },
        ];
        serialize_into_csv(
            vertices.iter(),
            "vertices-complete.csv.gz",
            tmp_dir,
            true,
            "write vertices",
        )
        .unwrap();

        // Walk edge without its own posted speed limit, adjacent to a 25 mph drive edge and a bike cycleway
        let walk_edges = vec![sample_edge(
            0,
            "seg-walk-res",
            "road-residential",
            None,
            false,
            vec![(-105.170735, 39.773087), (-105.170445, 39.773137)],
        )];
        let drive_edges = vec![sample_edge(
            0,
            "seg-drive-adj",
            "road-secondary",
            Some(25.0),
            false,
            vec![(-105.170735, 39.773090), (-105.170445, 39.773140)],
        )];
        let bike_edges = vec![sample_edge(
            0,
            "seg-bike-adj",
            "road-cycleway",
            None,
            false,
            vec![(-105.170612, 39.773116), (-105.170499, 39.773017)],
        )];

        serialize_into_csv(
            walk_edges.iter(),
            "edges-complete.csv.gz",
            &walk_dir,
            true,
            "write walk edges",
        )
        .unwrap();
        serialize_into_csv(
            drive_edges.iter(),
            "edges-complete.csv.gz",
            &drive_dir,
            true,
            "write drive edges",
        )
        .unwrap();
        serialize_into_csv(
            bike_edges.iter(),
            "edges-complete.csv.gz",
            &bike_dir,
            true,
            "write bike edges",
        )
        .unwrap();

        let vertices_path = tmp_dir
            .join("vertices-complete.csv.gz")
            .to_string_lossy()
            .into_owned();
        let walk_edges_path = walk_dir
            .join("edges-complete.csv.gz")
            .to_string_lossy()
            .into_owned();
        let drive_edges_path = drive_dir
            .join("edges-complete.csv.gz")
            .to_string_lossy()
            .into_owned();
        let bike_edges_path = bike_dir
            .join("edges-complete.csv.gz")
            .to_string_lossy()
            .into_owned();
        let wci_out_path = walk_dir.join("wci.csv").to_string_lossy().into_owned();

        bulk_compute_modal_metric_with_context::<EdgeCompleteRecord, VertexCompleteRecord>(
            ModalMetric::WalkingComfortIndex,
            &walk_edges_path,
            &vertices_path,
            &[drive_edges_path, bike_edges_path],
            &wci_out_path,
        )
        .unwrap();

        let wci_csv = fs::read_to_string(&wci_out_path).unwrap();
        let lines: Vec<&str> = wci_csv.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0],
            "wci_total,wci_walk,wci_speed,wci_cycle,wci_signal"
        );
        // walk (-2) + speed from 25 mph drive neighbor (+2) + cycle weighted across neighbors (+1) + stop sign (+1) = 2
        assert_eq!(lines[1], "2,-2,2,1,1");

        // Also verify writing compressed .csv.gz output directly
        let lts_out_gz_path = bike_dir.join("lts.csv.gz").to_string_lossy().into_owned();
        let bike_edges_path2 = bike_dir
            .join("edges-complete.csv.gz")
            .to_string_lossy()
            .into_owned();
        bulk_compute_modal_metric_with_context::<EdgeCompleteRecord, VertexCompleteRecord>(
            ModalMetric::LevelOfTrafficStress,
            &bike_edges_path2,
            &vertices_path,
            &[],
            &lts_out_gz_path,
        )
        .unwrap();
        assert!(Path::new(&lts_out_gz_path).exists());

        let _ = fs::remove_dir_all(tmp_dir);
    }
}
