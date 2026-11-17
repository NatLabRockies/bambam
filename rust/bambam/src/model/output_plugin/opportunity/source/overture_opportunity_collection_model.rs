use crate::model::output_plugin::opportunity::{GeoActivityMask, OpportunityDataset};
use bambam_omf::collection::{
    BuildingsRecord, OvertureMapsCollectionError, OvertureMapsCollector, OvertureRecordType,
    OvertureSpatialFilter, PlacesRecord,
};
use bambam_omf::collection::{
    OvertureMapsCollectorConfig, ReleaseVersion, RowFilterConfig, TaxonomyModel,
    TaxonomyModelBuilder,
};
use geo::{Centroid, Geometry};
use rayon::prelude::*;
use routee_compass_core::util::geo::PolygonalRTree;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

/// Validates that when `buildings_activity_mappings` is provided, its set of activity keys
/// matches `places_activity_mappings` exactly.
pub fn validate_overture_activity_keys(
    places_activity_mappings: &HashMap<String, Vec<String>>,
    buildings_activity_mappings: Option<&HashMap<String, Vec<String>>>,
) -> Result<(), OvertureMapsCollectionError> {
    if let Some(buildings_map) = buildings_activity_mappings {
        let places_keys: BTreeSet<&str> = places_activity_mappings
            .keys()
            .map(String::as_str)
            .collect();
        let buildings_keys: BTreeSet<&str> = buildings_map.keys().map(String::as_str).collect();
        if places_keys != buildings_keys {
            return Err(OvertureMapsCollectionError::InvalidUserInput(format!(
                "places_activity_mapping and buildings_activity_mapping must define the same activity keys; places={places_keys:?}, buildings={buildings_keys:?}"
            )));
        }
    }
    Ok(())
}

/// Model object that encapsulates the logic of
/// retrieving Places and Buildings data according
/// to the specified parameters, filtering, matching
/// and formatting into a collection of Opportunities.
///
/// The attributes of this struct are the realized instances
/// of the objects required for this operation, built from
/// the configuration file.
#[derive(Debug)]
pub struct OvertureOpportunityCollectionModel {
    collector: OvertureMapsCollector,
    release_version: ReleaseVersion,
    spatial_filter: OvertureSpatialFilter,
    places_row_filter_config: Option<RowFilterConfig>,
    buildings_row_filter_config: Option<RowFilterConfig>,
    places_taxonomy_model: Arc<TaxonomyModel>,
    buildings_activity_mappings: Option<HashMap<String, Vec<String>>>,
}

impl OvertureOpportunityCollectionModel {
    /// Create a new instance of [`OvertureOpportunityCollectionModel`] from serializable arguments
    ///
    /// # Arguments
    ///
    /// * collector_config - Configuration of the [`bambam_overturemaps::collection::OvertureMapsCollector`] to be used
    /// * release_version - OvertureMaps Release version (<https://docs.overturemaps.org/release/>),
    /// * spatial_filter - Two-stage [`OvertureSpatialFilter`] providing a Stage 1 `Bbox` for Parquet pushdown and Stage 2 geometry refinement,
    /// * places_activity_mappings - Mapping from MEP activity types to OvertureMaps categories for places dataset,
    /// * buildings_activity_mappings - Mapping from MEP activity types to OvertureMaps classes for buildings dataset. If `None`, buildings data is not used.
    ///
    /// # Returns
    ///
    /// A built instance of [`OvertureOpportunityCollectionModel`]
    pub fn new(
        collector_config: OvertureMapsCollectorConfig,
        release_version: ReleaseVersion,
        spatial_filter: OvertureSpatialFilter,
        places_activity_mappings: HashMap<String, Vec<String>>,
        buildings_activity_mappings: Option<HashMap<String, Vec<String>>>,
    ) -> Result<Self, OvertureMapsCollectionError> {
        validate_overture_activity_keys(
            &places_activity_mappings,
            buildings_activity_mappings.as_ref(),
        )?;
        let bbox = spatial_filter.bbox();
        let taxonomy_model =
            Arc::new(TaxonomyModelBuilder::new(places_activity_mappings.clone(), None).build()?);
        let places_row_filter_config = RowFilterConfig::Combined {
            filters: vec![
                Box::new(RowFilterConfig::from(bbox)),
                Box::new(RowFilterConfig::from(places_activity_mappings)),
            ],
        };
        let buildings_row_filter_config =
            buildings_activity_mappings
                .clone()
                .map(|mappings| RowFilterConfig::Combined {
                    filters: vec![
                        Box::new(RowFilterConfig::from(bbox)),
                        Box::new(RowFilterConfig::HasClassIn {
                            classes: HashSet::from_iter(mappings.values().flatten().cloned()),
                        }),
                    ],
                });

        Ok(Self {
            collector: OvertureMapsCollector::try_from(collector_config)?,
            release_version,
            spatial_filter,
            places_row_filter_config: Some(places_row_filter_config),
            buildings_row_filter_config,
            places_taxonomy_model: taxonomy_model,
            buildings_activity_mappings,
        })
    }

    /// Collect opportunities from Places and Buildings datasets and
    /// process them into Vec<[`Geometry`], f64> according to the configuration of the model
    pub fn collect(
        &self,
        activity_types: &[String],
    ) -> Result<OpportunityDataset, OvertureMapsCollectionError> {
        let release_uri = match self.release_version {
            ReleaseVersion::Latest => self.collector.get_latest_release()?,
            ReleaseVersion::Monthly { .. } => self.release_version.to_string(),
        };

        // Collect raw opportunities
        let mut places_opportunities =
            self.collect_places_opportunities(activity_types, &release_uri)?;

        if let Some(building_mappings) = &self.buildings_activity_mappings {
            let buildings_opportunities = self.collect_building_opportunities(
                activity_types,
                building_mappings,
                &release_uri,
            )?;

            // Build RTree for places
            let rtree = PolygonalRTree::new(
                places_opportunities
                    .iter()
                    .enumerate()
                    .map(|(i, (geom, _))| (geom.clone(), i))
                    .collect::<Vec<(Geometry<f32>, usize)>>(),
            )
            .map_err(OvertureMapsCollectionError::ProcessingError)?;

            // For each building, we are going to:
            //  1. Compute the intersection with places points
            //  2. Compare the MEP vectors
            //  3. If the building has a category not contained in the places data
            //     we return it as a new opportunity. Otherwise we skip it.
            let mut filtered_buildings: Vec<(Geometry<f32>, Vec<bool>)> = buildings_opportunities
                .into_iter()
                .map(|building| {
                    // Aggregate the values of all matching points into a single MEP vector
                    let places_mep_agg = rtree
                        .intersection(&building.0)?
                        // For each returned index in the intersection, find the corresponding opportunity tuple (Geometry, Vec<bool>)
                        .filter_map(|node| places_opportunities.get(node.data))
                        // Reduce them to a single Vec<bool> using an OR operation
                        .fold(vec![false; activity_types.len()], |mut acc, row| {
                            for (a, &b) in acc.iter_mut().zip(&row.1) {
                                *a |= b;
                            }
                            acc
                        });

                    // TODO: This logic potentially duplicates an opportunity, but was the logic implemented by the researchers
                    // Compare node (Places) MEP vector to building MEP vector
                    // We want to know if for any MEP category of the building is not contained in the points
                    let keep_building = building
                        .1
                        .iter()
                        .zip(places_mep_agg)
                        .any(|(b_flag, p_flag)| b_flag & !p_flag);

                    // Compute centroid if available
                    let centroid = building.0.centroid();

                    Ok::<_, String>(if keep_building {
                        centroid.map(|p| (p.into(), building.1))
                    } else {
                        None
                    })
                })
                .filter_map(Result::transpose)
                .collect::<Result<Vec<_>, String>>()
                .map_err(OvertureMapsCollectionError::ProcessingError)?;

            log::info!(
                "Number of useful building records: {}",
                filtered_buildings.len()
            );

            // Merge places_opportunities + buildings.centroid
            places_opportunities.extend(filtered_buildings);
        }

        Ok(places_opportunities
            .into_iter()
            .map(|(g, vec)| (g, vec.into_iter().map(|v| v as i16 as f64).collect()))
            .collect())
    }

    /// collect the places where opportunities can be found along with a mask
    /// for matching activity types per location.
    fn collect_places_opportunities(
        &self,
        activity_types: &[String],
        release_uri: &str,
    ) -> Result<GeoActivityMask, OvertureMapsCollectionError> {
        let places_records = self
            .collector
            .collect_from_release(
                release_uri,
                &OvertureRecordType::Places,
                self.places_row_filter_config.clone(),
            )?
            .into_iter()
            .map(PlacesRecord::try_from)
            .collect::<Result<Vec<PlacesRecord>, OvertureMapsCollectionError>>()?;
        log::info!("Total places records {}", places_records.len());

        // Compute MEP category vectors
        let mep_vectors = map_taxonomy_model(
            self.places_taxonomy_model.clone(),
            places_records
                .iter()
                .map(|record| record.get_categories().clone())
                .collect(),
            activity_types,
        )?;

        log::info!(
            "Total opportunities per category {:?}",
            (0..mep_vectors[0].len())
                .map(|i| mep_vectors.iter().map(|row| row[i] as i16 as f64).sum())
                .collect::<Vec<f64>>()
        );

        // Collect POI geometries
        let mep_geometries: Vec<Option<Geometry<f32>>> = places_records
            .into_iter()
            .map(|record| record.get_geometry())
            .collect();

        log::debug!(
            "Non-empty geometries: {:?}",
            mep_geometries
                .iter()
                .filter(|maybe_geometry| maybe_geometry.is_some())
                .collect::<Vec<_>>()
                .len()
        );

        Ok(self.filter_geometries(mep_geometries, mep_vectors))
    }

    fn collect_building_opportunities(
        &self,
        activity_types: &[String],
        buildings_activity_mappings: &HashMap<String, Vec<String>>,
        release_uri: &str,
    ) -> Result<GeoActivityMask, OvertureMapsCollectionError> {
        // Build the taxonomy model from the mapping by transforming the vectors into HashSets
        let buildings_taxonomy_model = TaxonomyModel::from_mapping(
            buildings_activity_mappings
                .clone()
                .into_iter()
                .map(|(key, vec)| (key, HashSet::from_iter(vec)))
                .collect(),
        );
        let arc_buildings_taxonomy = Arc::new(buildings_taxonomy_model);

        // Use the collector to retrieve buildings data
        let buildings_records = self
            .collector
            .collect_from_release(
                release_uri,
                &OvertureRecordType::Buildings,
                self.buildings_row_filter_config.clone(),
            )?
            .into_iter()
            .map(BuildingsRecord::try_from)
            .collect::<Result<Vec<BuildingsRecord>, OvertureMapsCollectionError>>()?;
        log::info!("Total buildings records {}", buildings_records.len());

        // Compute MEP category vectors
        let mep_vectors = map_taxonomy_model(
            arc_buildings_taxonomy,
            buildings_records
                .iter()
                .filter_map(|record| record.get_class())
                .map(|class| vec![class])
                .collect(),
            activity_types,
        )?;

        if !mep_vectors.is_empty() {
            log::info!(
                "Total opportunities per category {:?}",
                (0..mep_vectors[0].len())
                    .map(|i| mep_vectors.iter().map(|row| row[i] as i16 as f64).sum())
                    .collect::<Vec<f64>>()
            );
        }

        // Collect geometries
        let mep_geometries: Vec<Option<Geometry<f32>>> = buildings_records
            .iter()
            .map(|record| record.get_geometry())
            .collect();

        Ok(self.filter_geometries(mep_geometries, mep_vectors))
    }

    /// Zips candidate geometries with their MEP vectors, discarding `None` geometries
    /// and any geometry that does not pass the Stage 2 [`OvertureSpatialFilter`].
    fn filter_geometries(
        &self,
        geometries: Vec<Option<Geometry<f32>>>,
        vectors: Vec<Vec<bool>>,
    ) -> GeoActivityMask {
        geometries
            .into_par_iter()
            .zip(vectors)
            .filter_map(|(maybe_geometry, vector)| {
                maybe_geometry.and_then(|geometry| {
                    self.spatial_filter
                        .matches_geometry(&geometry)
                        .then_some((geometry, vector))
                })
            })
            .collect()
    }
}

/// Takes a taxonomy model and transform the vector of
/// string labels (categories) into a vector of MEP opportunity
/// categories.
fn map_taxonomy_model(
    taxonomy_model: Arc<TaxonomyModel>,
    categories: Vec<Vec<String>>,
    group_labels: &[String],
) -> Result<Vec<Vec<bool>>, OvertureMapsCollectionError> {
    categories
        .iter()
        .map(|category_vec| {
            Ok(
                taxonomy_model
                    .clone()
                    .reverse_map(category_vec, group_labels.to_vec())?
                    // Reduce Vec<Vec<bool>> to Vec<bool> applying OR logic
                    .into_iter()
                    .reduce(|mut acc, v| {
                        acc.iter_mut().zip(v.iter()).for_each(|(a, b)| *a |= b);
                        acc
                    })
                    .unwrap_or_default(), // Map bool to f64 - it is easier to merge different datasets like this
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_overture_activity_keys_rejects_mismatched_keys() {
        let places = HashMap::from([
            ("food".to_string(), vec!["restaurant".to_string()]),
            ("retail".to_string(), vec!["shopping".to_string()]),
        ]);
        let matching_buildings = HashMap::from([
            ("food".to_string(), vec![]),
            ("retail".to_string(), vec!["commercial".to_string()]),
        ]);
        let mismatched_buildings =
            HashMap::from([("healthcare".to_string(), vec!["medical".to_string()])]);

        assert!(validate_overture_activity_keys(&places, None).is_ok());
        assert!(validate_overture_activity_keys(&places, Some(&matching_buildings)).is_ok());
        assert!(validate_overture_activity_keys(&places, Some(&mismatched_buildings)).is_err());
    }
}
