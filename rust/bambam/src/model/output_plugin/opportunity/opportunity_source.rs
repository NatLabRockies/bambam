use crate::model::output_plugin::opportunity::OpportunityDataset;

use super::{
    source::lodes::lodes_ops,
    source::overture_opportunity_collection_model::OvertureOpportunityCollectionModel,
    study_region::StudyRegion,
};
use bambam_omf::collection::{Bbox, OvertureMapsCollectorConfig, ReleaseVersion};
use bamcensus::app::lodes_tiger;
use bamcensus_core::model::identifier::GeoidType;
use bamcensus_lehd::model::{
    LodesDataset, LodesEdition, LodesJobType, WacSegment, WorkplaceSegment,
};
use geo::Geometry;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// an API data source for opportunities.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum OpportunitySource {
    /// collects opportunities from a Longitudinal Employer-Household Dynamics (LODES)
    /// Workplace Area Characteristics (WAC) dataset paired with it's corresponding
    /// TIGER/Line Shapefile. the user provides a mapping from each WacSegment to a list of
    /// activity types (at least one) which it represents.
    #[serde(rename = "lodes")]
    UsCensusLehdLodes {
        activity_mapping: HashMap<WacSegment, Vec<String>>,
        study_region: StudyRegion,
        data_granularity: Option<GeoidType>,
        edition: LodesEdition,
        job_type: LodesJobType,
        segment: WorkplaceSegment,
        year: u64,
    },
    /// collects opportunities from <https://docs.overturemaps.org/guides/places/>.
    #[serde(rename = "overture")]
    OvertureMapsPlaces {
        collector_config: OvertureMapsCollectorConfig,
        bbox_boundary: Bbox,
        places_activity_mapping: HashMap<String, Vec<String>>,
        buildings_activity_mapping: Option<HashMap<String, Vec<String>>>,
        #[serde(default)]
        release_version: ReleaseVersion,
    },
}

impl OpportunitySource {
    /// generates a collection of Geometries paired with activity counts
    /// from some data source API. Configurations for a given API are
    /// provided by this [`OpportunitySource`] instance.
    ///
    /// # Arguments
    ///
    /// * `activity_types` - the types of activities expected
    ///
    /// # Returns
    ///
    /// A collection of Geometries tagged with activity rows.
    pub fn generate_dataset(
        &self,
        activity_types: &[String],
    ) -> Result<OpportunityDataset, String> {
        match self {
            OpportunitySource::OvertureMapsPlaces {
                collector_config,
                bbox_boundary,
                places_activity_mapping,
                buildings_activity_mapping,
                release_version,
            } => {
                // Instantiate Collection Model Object which re-structures activity mapping
                // information into a fully functional collection pipeline. This step allows
                // to reduce repetition in the configuration file by making some assumptions
                // about the filters being used.
                let colletor_model = OvertureOpportunityCollectionModel::new(
                    *collector_config,
                    release_version.clone(),
                    *bbox_boundary,
                    places_activity_mapping.clone(),
                    buildings_activity_mapping.clone(),
                )
                .map_err(|e| format!("Error creating Overture OpportunityCollectionModel: {e}"))?;

                colletor_model
                    .collect(activity_types)
                    .map_err(|e| format!("Error during overturemaps collection: {e}"))
            }
            OpportunitySource::UsCensusLehdLodes {
                activity_mapping,
                study_region,
                data_granularity,
                edition,
                job_type,
                segment,
                year,
            } => {
                //
                let geoids = study_region.get_geoids()?;
                let dataset = LodesDataset::WAC {
                    edition: *edition,
                    job_type: *job_type,
                    segment: *segment,
                    year: *year,
                };
                let wac_segments = activity_mapping.keys().cloned().collect_vec();
                lodes_ops::collect_lodes_opportunities(
                    &dataset,
                    &wac_segments,
                    &geoids,
                    data_granularity,
                    activity_types,
                    activity_mapping,
                )
            }
        }
    }
}
