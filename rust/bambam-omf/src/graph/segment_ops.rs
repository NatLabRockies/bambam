//! functions mapped onto [TransportationSegmentRecord] rows to create [SegmentSplit] values

use crate::{
    collection::{
        record::SegmentHeading, OvertureMapsCollectionError, SegmentAccessRestrictionWhen,
        TransportationSegmentRecord,
    },
    graph::{segment_split::SegmentSplit, ConnectorInSegment},
};
use itertools::Itertools;

/// creates simple connector splits from a record.
pub fn process_simple_connector_splits(
    segment: &TransportationSegmentRecord,
    when: Option<&SegmentAccessRestrictionWhen>,
) -> Result<Vec<SegmentSplit>, OvertureMapsCollectionError> {
    let headings = get_headings(segment, when)?;
    // sort by "at" values to ensure monotonicity
    let mut sorted_connectors = segment.connectors.clone().ok_or_else(|| {
        OvertureMapsCollectionError::InvalidUserInput(String::from(
            "no connectors in downloaded dataset",
        ))
    })?;
    sorted_connectors.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap_or(std::cmp::Ordering::Equal));

    // if let Some(first) = sorted_connectors.first() {
    //     if first.at > consts::F64_DISTANCE_TOLERANCE {
    //         sorted_connectors.insert(
    //             0,
    //             crate::collection::record::segment::ConnectorReference {
    //                 connector_id: format!("{}_start", segment.id),
    //                 at: 0.0,
    //             },
    //         );
    //     }
    // }
    // if let Some(last) = sorted_connectors.last() {
    //     if last.at < 1.0 - consts::F64_DISTANCE_TOLERANCE {
    //         sorted_connectors.push(crate::collection::record::segment::ConnectorReference {
    //             connector_id: format!("{}_end", segment.id),
    //             at: 1.0,
    //         });
    //     }
    // }

    let result = sorted_connectors
        .iter()
        .tuple_windows()
        .flat_map(|(src, dst)| {
            headings.iter().cloned().map(|heading| {
                let src =
                    ConnectorInSegment::new(segment.id.clone(), src.connector_id.clone(), src.at);
                let dst =
                    ConnectorInSegment::new(segment.id.clone(), dst.connector_id.clone(), dst.at);
                SegmentSplit { src, dst, heading }
            })
        })
        .collect::<Vec<SegmentSplit>>();
    Ok(result)
}

/// determines the headings over a segment that are supported. optionally matched to some
/// set of user-provided restrictions.
pub fn get_headings(
    segment: &TransportationSegmentRecord,
    when: Option<&SegmentAccessRestrictionWhen>,
) -> Result<Vec<SegmentHeading>, OvertureMapsCollectionError> {
    Ok(bambam_network::evaluate_segment_headings(
        segment.access_restrictions.as_deref(),
        when,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{
        filter::{MatchBehavior, TravelModeFilter},
        record::{
            OvertureMapsBbox, SegmentAccessRestriction, SegmentAccessRestrictionWhenVehicle,
            SegmentAccessType, SegmentLengthUnit, SegmentMode, SegmentRecognized, SegmentUnit,
            SegmentUsing, SegmentVehicleComparator, SegmentVehicleDimension,
        },
        SegmentClass,
    };

    #[test]
    fn test_segment_without_access_restrictions_both_headings() {
        // Test: A segment without access restrictions should produce both headings
        let segment = create_test_segment(None);
        let result = get_headings(&segment, None).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.contains(&SegmentHeading::Forward));
        assert!(result.contains(&SegmentHeading::Backward));
    }

    #[test]
    fn test_segment_with_empty_access_restrictions_both_headings() {
        // Test: A segment with empty access restrictions should produce both headings
        // when the user passes no constraints.
        let segment = create_test_segment(Some(vec![]));
        let result = get_headings(&segment, None).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.contains(&SegmentHeading::Forward));
        assert!(result.contains(&SegmentHeading::Backward));
    }

    #[test]
    fn test_segment_with_forward_only_restriction() {
        // Test: A segment with backward denied should only produce Forward heading
        // when the user passes no constraints.
        let segment = create_test_segment(Some(vec![create_restriction_heading_only(
            SegmentAccessType::Denied,
            SegmentHeading::Backward,
        )]));

        let result = get_headings(&segment, None).unwrap();

        assert_eq!(result, vec![SegmentHeading::Forward]);
    }

    #[test]
    fn test_segment_with_backward_only_restriction() {
        // Test: A segment with forward denied should only produce Backward heading
        // when the user passes no constraints.
        let segment = create_test_segment(Some(vec![create_restriction_heading_only(
            SegmentAccessType::Denied,
            SegmentHeading::Forward,
        )]));

        let result = get_headings(&segment, None).unwrap();

        assert_eq!(result, vec![SegmentHeading::Backward]);
    }

    #[test]
    fn test_segment_with_mode_restriction_matching_when() {
        // Test: Denied all modes for forward, then Allowed for Car/Bicycle
        // Query with Car should allow Forward
        let segment = create_test_segment(Some(create_denied_all_allowed_specific(
            SegmentHeading::Forward,
            Some(vec![SegmentMode::Car, SegmentMode::Bicycle]),
            None,
            None,
        )));

        let when = create_when(
            SegmentHeading::Forward,
            Some(vec![SegmentMode::Car]),
            None,
            None,
        );
        let result = get_headings(&segment, Some(&when)).unwrap();

        assert_eq!(result, vec![SegmentHeading::Forward]);
    }

    #[test]
    fn test_segment_with_mode_restriction_not_matching_when() {
        // Test: Denied all for forward + Allowed only Car
        // Query with Forward, Bicycle should deny both forward and backward
        let segment = create_test_segment(Some(create_denied_all_allowed_specific(
            SegmentHeading::Forward,
            Some(vec![SegmentMode::Car]),
            None,
            None,
        )));

        let when = create_when(
            SegmentHeading::Forward,
            Some(vec![SegmentMode::Bicycle]),
            None,
            None,
        );
        let result = get_headings(&segment, Some(&when)).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_segment_with_multiple_fields_matching() {
        // Test: Denied all + Allowed with multiple field constraints, all matching
        let segment = create_test_segment(Some(create_denied_all_allowed_specific(
            SegmentHeading::Forward,
            Some(vec![SegmentMode::Car, SegmentMode::Bicycle]),
            Some(vec![SegmentUsing::AsCustomer]),
            Some(vec![SegmentRecognized::AsEmployee]),
        )));

        let when = create_when(
            SegmentHeading::Forward,
            Some(vec![SegmentMode::Car]),
            Some(vec![SegmentUsing::AsCustomer]),
            Some(vec![SegmentRecognized::AsEmployee]),
        );
        let result = get_headings(&segment, Some(&when)).unwrap();

        assert_eq!(result, vec![SegmentHeading::Forward]); // Forward allowed with all conditions met
    }

    #[test]
    fn test_denied_all_then_allowed_specific() {
        // Test: "Denied all" followed by "Allowed for cars" should allow only cars
        // This is the classic "deny by default, allow exceptions" pattern
        let segment = create_test_segment(Some(create_denied_all_allowed_specific(
            SegmentHeading::Forward,
            Some(vec![SegmentMode::Car]),
            None,
            None,
        )));

        // Query without when - should match the Denied restriction, accept any valid heading
        let result_no_when = get_headings(&segment, None).unwrap();
        assert_eq!(result_no_when, vec![SegmentHeading::Backward]); // Only Backward is valid

        // Query with Car mode - should match the Allowed restriction
        let when_car = create_when(
            SegmentHeading::Forward,
            Some(vec![SegmentMode::Car]),
            None,
            None,
        );
        let result_car = get_headings(&segment, Some(&when_car)).unwrap();
        assert_eq!(result_car, vec![SegmentHeading::Forward]);

        // Query with Bicycle mode - should be denied
        let when_bicycle = create_when(
            SegmentHeading::Forward,
            Some(vec![SegmentMode::Bicycle]),
            None,
            None,
        );
        let result_bicycle = get_headings(&segment, Some(&when_bicycle)).unwrap();
        assert_eq!(result_bicycle.len(), 0);
    }

    #[test]
    fn test_allowed_overrides_denied_same_heading() {
        // Test: When both Denied and Allowed apply to the same conditions,
        // Allowed takes precedence (specific exception pattern)
        let segment = create_test_segment(Some(vec![
            SegmentAccessRestriction {
                access_type: SegmentAccessType::Denied,
                when: Some(SegmentAccessRestrictionWhen {
                    during: None,
                    heading: Some(SegmentHeading::Forward),
                    using: None,
                    recognized: None,
                    mode: Some(vec![SegmentMode::Car, SegmentMode::Bicycle]),
                    vehicle: None,
                }),
                vehicle: None,
            },
            SegmentAccessRestriction {
                access_type: SegmentAccessType::Allowed,
                when: Some(SegmentAccessRestrictionWhen {
                    during: None,
                    heading: Some(SegmentHeading::Forward),
                    using: None,
                    recognized: None,
                    mode: Some(vec![SegmentMode::Car]),
                    vehicle: None,
                }),
                vehicle: None,
            },
        ]));

        // Car should be allowed (Allowed overrides Denied)
        let when_car = create_when(
            SegmentHeading::Forward,
            Some(vec![SegmentMode::Car]),
            None,
            None,
        );
        let result = get_headings(&segment, Some(&when_car)).unwrap();
        assert_eq!(result, vec![SegmentHeading::Forward]);
    }

    #[test]
    fn test_multiple_denied_restrictions() {
        // Test: Multiple Denied restrictions - all should be respected
        let segment = create_test_segment(Some(vec![
            create_restriction_heading_only(SegmentAccessType::Denied, SegmentHeading::Forward),
            create_restriction_heading_only(SegmentAccessType::Denied, SegmentHeading::Backward),
        ]));

        let result = get_headings(&segment, None).unwrap();

        // Both directions denied
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_designated_treated_as_allowed() {
        // Test: Blanket denial with Designated mode access type: should be treated like Allowed
        let segment = create_test_segment(Some(vec![
            SegmentAccessRestriction {
                access_type: SegmentAccessType::Denied,
                when: Some(SegmentAccessRestrictionWhen {
                    during: None,
                    heading: Some(SegmentHeading::Forward),
                    using: None,
                    recognized: None,
                    mode: None,
                    vehicle: None,
                }),
                vehicle: None,
            },
            SegmentAccessRestriction {
                access_type: SegmentAccessType::Designated,
                when: Some(SegmentAccessRestrictionWhen {
                    during: None,
                    heading: Some(SegmentHeading::Forward),
                    using: None,
                    recognized: None,
                    mode: Some(vec![SegmentMode::Bicycle]),
                    vehicle: None,
                }),
                vehicle: None,
            },
        ]));

        let when_bicycle = create_when(
            SegmentHeading::Forward,
            Some(vec![SegmentMode::Bicycle]),
            None,
            None,
        );
        let result = get_headings(&segment, Some(&when_bicycle)).unwrap();

        assert_eq!(result, vec![SegmentHeading::Forward]);
    }

    #[test]
    fn test_restriction_with_mode_does_not_apply_when_query_has_no_mode() {
        // Test: A restriction that specifies a mode constraint should NOT apply
        // when the query doesn't specify a mode at all
        // This exercises the fix: we check if restriction.mode is Some, not if when.mode is Some
        let segment = create_test_segment(Some(vec![
            // Deny all forward traffic (no mode constraint)
            create_restriction_heading_only(SegmentAccessType::Denied, SegmentHeading::Forward),
            // Allow forward for bicycles only (mode constraint)
            SegmentAccessRestriction {
                access_type: SegmentAccessType::Allowed,
                when: Some(SegmentAccessRestrictionWhen {
                    during: None,
                    heading: Some(SegmentHeading::Forward),
                    using: None,
                    recognized: None,
                    mode: Some(vec![SegmentMode::Bicycle]),
                    vehicle: None,
                }),
                vehicle: None,
            },
        ]));

        // Query with when=None (no mode specified)
        // The Denied restriction applies (no mode constraint, applies broadly)
        // The Allowed restriction should NOT apply (has mode constraint, but query has none)
        // Expected: Forward is denied because only Denied applies
        let result = get_headings(&segment, None).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result.contains(&SegmentHeading::Backward));
        assert!(!result.contains(&SegmentHeading::Forward));
    }

    #[test]
    fn test_from_data_1() {
        // test case from OMF data, a segment with
        //   - blanket denial for heading backward
        //   - opts in designated HGV travel, no heading specified
        // when
        //   - heading forward with motor_vehicle, car, truck, motorcycle: reject
        //   - heading backward with "": reject
        //   - heading forward with HGV: accept
        let segment = create_test_segment(Some(vec![
            create_restriction_heading_only(SegmentAccessType::Denied, SegmentHeading::Backward),
            create_restriction_mode(SegmentAccessType::Designated, vec![SegmentMode::Hgv]),
        ]));

        let mode = vec![
            SegmentMode::MotorVehicle,
            SegmentMode::Car,
            SegmentMode::Truck,
            SegmentMode::Motorcycle,
        ];

        // forward has no blanket denial, only optional designation, so we accept
        let when1 = create_when(SegmentHeading::Forward, Some(mode.clone()), None, None);
        let result1 = get_headings(&segment, Some(&when1)).unwrap();
        assert_eq!(result1.len(), 1);

        // blanket backward denial on a backward-oriented when query -> empty result
        let when2 = create_when(SegmentHeading::Backward, Some(mode.clone()), None, None);
        let result2 = get_headings(&segment, Some(&when2)).unwrap();
        assert_eq!(result2.len(), 0);
    }

    #[test]
    fn test_from_data_2() {
        // a segment has a blanket denial of backward access, and a designation for bicycle.
        // this should allow any mode traveling forward but no mode traveling backward.

        let segment = create_test_segment(Some(vec![
            create_restriction_heading_only(SegmentAccessType::Denied, SegmentHeading::Backward),
            create_restriction_mode(SegmentAccessType::Designated, vec![SegmentMode::Bicycle]),
        ]));

        // case 1: forward traversal should be accepted
        let when1 = create_when(
            SegmentHeading::Forward,
            Some(vec![SegmentMode::Bicycle]),
            None,
            None,
        );
        let result1 = get_headings(&segment, Some(&when1)).unwrap();
        assert_eq!(result1, vec![SegmentHeading::Forward]);

        // case 2: backward traversal should be denied
        let when2 = create_when(
            SegmentHeading::Backward,
            Some(vec![SegmentMode::Bicycle]),
            None,
            None,
        );
        let result2 = get_headings(&segment, Some(&when2)).unwrap();

        assert_eq!(result2.len(), 0);
    }

    #[test]
    fn test_from_data_3() {
        let access_restrictions = vec![
            create_restriction_mode(SegmentAccessType::Designated, vec![SegmentMode::Hgv]),
            create_restriction_when_vehicle(
                SegmentAccessType::Denied,
                SegmentAccessRestrictionWhenVehicle {
                    dimension: SegmentVehicleDimension::Height,
                    comparison: SegmentVehicleComparator::GreaterThan,
                    value: 13.0,
                    unit: Some(SegmentUnit::Length(SegmentLengthUnit::Feet)),
                },
            ),
        ];

        let segment = create_test_segment(Some(access_restrictions));

        // Should pass simple drive filter
        let filter1 = create_simple_drive_filter();
        assert!(filter1.matches_filter(&segment));

        // Should be denied when vehicle height is greater than 13 ft
        let when1 = SegmentAccessRestrictionWhen {
            during: None,
            heading: Some(SegmentHeading::Forward),
            using: None,
            recognized: None,
            mode: None,
            vehicle: Some(vec![SegmentAccessRestrictionWhenVehicle {
                dimension: SegmentVehicleDimension::Height,
                comparison: SegmentVehicleComparator::GreaterThan,
                value: 100.0,
                unit: Some(SegmentUnit::Length(SegmentLengthUnit::Feet)),
            }]),
        };
        let result1 = get_headings(&segment, Some(&when1)).unwrap();
        assert_eq!(result1.len(), 0);

        // But should be allowed in any other case
        let when2 = SegmentAccessRestrictionWhen {
            during: None,
            heading: Some(SegmentHeading::Forward),
            using: None,
            recognized: None,
            mode: None,
            vehicle: Some(vec![SegmentAccessRestrictionWhenVehicle {
                dimension: SegmentVehicleDimension::Height,
                comparison: SegmentVehicleComparator::GreaterThan,
                value: 10.,
                unit: Some(SegmentUnit::Length(SegmentLengthUnit::Feet)),
            }]),
        };
        let result2 = get_headings(&segment, Some(&when2)).unwrap();
        assert_eq!(result2, vec![SegmentHeading::Forward]);

        // also when no vehicle is specified
        let when3 = SegmentAccessRestrictionWhen {
            during: None,
            heading: Some(SegmentHeading::Forward),
            using: None,
            recognized: None,
            mode: None,
            vehicle: None,
        };
        let result3 = get_headings(&segment, Some(&when3)).unwrap();
        assert_eq!(result3, vec![SegmentHeading::Forward]);
    }

    // Helper to represent a typical drive mode filter
    fn create_simple_drive_filter() -> TravelModeFilter {
        let classes = vec![
            SegmentClass::Motorway,
            SegmentClass::Trunk,
            SegmentClass::Primary,
            SegmentClass::Secondary,
            SegmentClass::Tertiary,
            SegmentClass::Residential,
            SegmentClass::LivingStreet,
            SegmentClass::Unclassified,
            SegmentClass::Service,
            SegmentClass::Unknown,
        ]
        .into_iter()
        .collect();

        TravelModeFilter::MatchesClasses {
            classes,
            behavior: MatchBehavior::Include,
            allow_unset: true,
        }
    }

    /// Helper to create a minimal segment for testing
    fn create_test_segment(
        access_restrictions: Option<Vec<SegmentAccessRestriction>>,
    ) -> TransportationSegmentRecord {
        // Create a minimal valid bbox using serde deserialization
        let bbox: OvertureMapsBbox =
            serde_json::from_str(r#"{"xmin": 0.0, "xmax": 1.0, "ymin": 0.0, "ymax": 1.0}"#)
                .expect("test invariant failed, unable to mock bbox of record");
        let mut record = TransportationSegmentRecord::default();
        record.access_restrictions = access_restrictions;
        record.bbox = bbox;
        record
    }

    /// Helper to create a simple access restriction with only heading constraint
    fn create_restriction_heading_only(
        access_type: SegmentAccessType,
        heading: SegmentHeading,
    ) -> SegmentAccessRestriction {
        SegmentAccessRestriction {
            access_type,
            when: Some(SegmentAccessRestrictionWhen {
                during: None,
                heading: Some(heading),
                using: None,
                recognized: None,
                mode: None,
                vehicle: None,
            }),
            vehicle: None,
        }
    }

    /// Helper to create a simple access restriction with only mode constraint
    fn create_restriction_mode(
        access_type: SegmentAccessType,
        modes: Vec<SegmentMode>,
    ) -> SegmentAccessRestriction {
        SegmentAccessRestriction {
            access_type,
            when: Some(SegmentAccessRestrictionWhen {
                during: None,
                heading: None,
                using: None,
                recognized: None,
                mode: Some(modes),
                vehicle: None,
            }),
            vehicle: None,
        }
    }

    /// Helper to create a simple access restriction with heading + modes constraint
    fn create_restriction_heading_mode(
        access_type: SegmentAccessType,
        heading: SegmentHeading,
        modes: Vec<SegmentMode>,
    ) -> SegmentAccessRestriction {
        SegmentAccessRestriction {
            access_type,
            when: Some(SegmentAccessRestrictionWhen {
                during: None,
                heading: Some(heading),
                using: None,
                recognized: None,
                mode: Some(modes),
                vehicle: None,
            }),
            vehicle: None,
        }
    }

    /// Helper to create a simple access restriction with vehicle detail
    fn create_restriction_when_vehicle(
        access_type: SegmentAccessType,
        vehicle: SegmentAccessRestrictionWhenVehicle,
    ) -> SegmentAccessRestriction {
        SegmentAccessRestriction {
            access_type,
            when: Some(SegmentAccessRestrictionWhen {
                during: None,
                heading: None,
                using: None,
                recognized: None,
                mode: None,
                vehicle: Some(vec![vehicle]),
            }),
            vehicle: None,
        }
    }

    /// Helper to create "Denied all + Allowed specific" pattern for a heading
    fn create_denied_all_allowed_specific(
        heading: SegmentHeading,
        allowed_modes: Option<Vec<SegmentMode>>,
        allowed_using: Option<Vec<SegmentUsing>>,
        allowed_recognized: Option<Vec<SegmentRecognized>>,
    ) -> Vec<SegmentAccessRestriction> {
        vec![
            SegmentAccessRestriction {
                access_type: SegmentAccessType::Denied,
                when: Some(SegmentAccessRestrictionWhen {
                    during: None,
                    heading: Some(heading.clone()),
                    using: None,
                    recognized: None,
                    mode: None,
                    vehicle: None,
                }),
                vehicle: None,
            },
            SegmentAccessRestriction {
                access_type: SegmentAccessType::Allowed,
                when: Some(SegmentAccessRestrictionWhen {
                    during: None,
                    heading: Some(heading),
                    using: allowed_using,
                    recognized: allowed_recognized,
                    mode: allowed_modes,
                    vehicle: None,
                }),
                vehicle: None,
            },
        ]
    }

    /// Helper to create a query when object
    fn create_when(
        heading: SegmentHeading,
        mode: Option<Vec<SegmentMode>>,
        using: Option<Vec<SegmentUsing>>,
        recognized: Option<Vec<SegmentRecognized>>,
    ) -> SegmentAccessRestrictionWhen {
        SegmentAccessRestrictionWhen {
            during: None,
            heading: Some(heading),
            using,
            recognized,
            mode,
            vehicle: None,
        }
    }

    #[test]
    fn test_general_denial_blocks_both_directions() {
        // Test: A denial with no heading specified should block both directions
        let segment = create_test_segment(Some(vec![SegmentAccessRestriction {
            access_type: SegmentAccessType::Denied,
            when: Some(SegmentAccessRestrictionWhen {
                during: None,
                heading: None, // No heading = applies to all
                using: None,
                recognized: None,
                mode: None,
                vehicle: None,
            }),
            vehicle: None,
        }]));

        let result = get_headings(&segment, None).unwrap();
        assert_eq!(
            result.len(),
            0,
            "General denial should block both directions"
        );
    }

    #[test]
    fn test_general_allowance_overrides_general_denial() {
        // Test: A general allowance (no heading) should override a general denial (no heading)
        // for a specific mode
        let segment = create_test_segment(Some(vec![
            // General denial for all
            SegmentAccessRestriction {
                access_type: SegmentAccessType::Denied,
                when: Some(SegmentAccessRestrictionWhen {
                    during: None,
                    heading: None,
                    using: None,
                    recognized: None,
                    mode: None,
                    vehicle: None,
                }),
                vehicle: None,
            },
            // General allowance for bicycles (no heading specified)
            SegmentAccessRestriction {
                access_type: SegmentAccessType::Allowed,
                when: Some(SegmentAccessRestrictionWhen {
                    during: None,
                    heading: None,
                    using: None,
                    recognized: None,
                    mode: Some(vec![SegmentMode::Bicycle]),
                    vehicle: None,
                }),
                vehicle: None,
            },
        ]));

        // Query with bicycle mode - should be allowed in both directions
        let when_fwd = create_when(
            SegmentHeading::Forward,
            Some(vec![SegmentMode::Bicycle]),
            None,
            None,
        );
        let result_fwd = get_headings(&segment, Some(&when_fwd)).unwrap();
        assert_eq!(result_fwd, vec![SegmentHeading::Forward]);

        let when_bwd = create_when(
            SegmentHeading::Backward,
            Some(vec![SegmentMode::Bicycle]),
            None,
            None,
        );
        let result_bwd = get_headings(&segment, Some(&when_bwd)).unwrap();
        assert_eq!(result_bwd, vec![SegmentHeading::Backward]);
    }

    #[test]
    fn test_heading_specific_allowance_without_denial() {
        // Test: A heading-specific allowance without any denial should allow that heading
        // (and the other heading should default to allowed since no denial)
        let segment = create_test_segment(Some(vec![SegmentAccessRestriction {
            access_type: SegmentAccessType::Allowed,
            when: Some(SegmentAccessRestrictionWhen {
                during: None,
                heading: Some(SegmentHeading::Forward),
                using: None,
                recognized: None,
                mode: Some(vec![SegmentMode::Bicycle]),
                vehicle: None,
            }),
            vehicle: None,
        }]));

        // Query with bicycle forward - should be allowed (explicit allowance)
        let when_fwd = create_when(
            SegmentHeading::Forward,
            Some(vec![SegmentMode::Bicycle]),
            None,
            None,
        );
        let result_fwd = get_headings(&segment, Some(&when_fwd)).unwrap();
        assert_eq!(result_fwd, vec![SegmentHeading::Forward]);

        // Query with bicycle backward - should also be allowed (no denial, defaults to allowed)
        let when_bwd = create_when(
            SegmentHeading::Backward,
            Some(vec![SegmentMode::Bicycle]),
            None,
            None,
        );
        let result_bwd = get_headings(&segment, Some(&when_bwd)).unwrap();
        assert_eq!(result_bwd, vec![SegmentHeading::Backward]);
    }

    #[test]
    fn test_restrictions_exist_but_none_apply() {
        // Test: Restrictions exist but none apply to the query (different mode)
        // Should default to allowed
        let segment = create_test_segment(Some(vec![create_restriction_heading_mode(
            SegmentAccessType::Denied,
            SegmentHeading::Forward,
            vec![SegmentMode::Car],
        )]));

        // Query with Bicycle - the Car denial shouldn't apply
        let when_bicycle = create_when(
            SegmentHeading::Forward,
            Some(vec![SegmentMode::Bicycle]),
            None,
            None,
        );
        let result = get_headings(&segment, Some(&when_bicycle)).unwrap();
        assert_eq!(
            result,
            vec![SegmentHeading::Forward],
            "Denial for Car shouldn't affect Bicycle"
        );
    }

    #[test]
    fn test_heading_specific_allowance_overrides_general_denial() {
        // Test: A heading-specific allowance should override a general (non-heading) denial
        let segment = create_test_segment(Some(vec![
            // General denial (no heading)
            SegmentAccessRestriction {
                access_type: SegmentAccessType::Denied,
                when: Some(SegmentAccessRestrictionWhen {
                    during: None,
                    heading: None,
                    using: None,
                    recognized: None,
                    mode: None,
                    vehicle: None,
                }),
                vehicle: None,
            },
            // Heading-specific allowance for forward + bicycle
            SegmentAccessRestriction {
                access_type: SegmentAccessType::Allowed,
                when: Some(SegmentAccessRestrictionWhen {
                    during: None,
                    heading: Some(SegmentHeading::Forward),
                    using: None,
                    recognized: None,
                    mode: Some(vec![SegmentMode::Bicycle]),
                    vehicle: None,
                }),
                vehicle: None,
            },
        ]));

        // Forward bicycle should be allowed (heading-specific allowance)
        let when_fwd = create_when(
            SegmentHeading::Forward,
            Some(vec![SegmentMode::Bicycle]),
            None,
            None,
        );
        let result_fwd = get_headings(&segment, Some(&when_fwd)).unwrap();
        assert_eq!(result_fwd, vec![SegmentHeading::Forward]);

        // Backward bicycle should be denied (general denial, no allowance)
        let when_bwd = create_when(
            SegmentHeading::Backward,
            Some(vec![SegmentMode::Bicycle]),
            None,
            None,
        );
        let result_bwd = get_headings(&segment, Some(&when_bwd)).unwrap();
        assert_eq!(result_bwd.len(), 0);
    }

    #[test]
    fn test_general_denial_with_heading_specific_denial_same_heading() {
        // Test: Both general denial and heading-specific denial for same heading
        // The heading-specific denial takes priority, requires heading-specific allowance
        let segment = create_test_segment(Some(vec![
            // General denial
            SegmentAccessRestriction {
                access_type: SegmentAccessType::Denied,
                when: Some(SegmentAccessRestrictionWhen {
                    during: None,
                    heading: None,
                    using: None,
                    recognized: None,
                    mode: None,
                    vehicle: None,
                }),
                vehicle: None,
            },
            // Heading-specific denial for forward
            create_restriction_heading_only(SegmentAccessType::Denied, SegmentHeading::Forward),
            // General allowance for bicycle (should NOT override heading-specific denial)
            SegmentAccessRestriction {
                access_type: SegmentAccessType::Allowed,
                when: Some(SegmentAccessRestrictionWhen {
                    during: None,
                    heading: None,
                    using: None,
                    recognized: None,
                    mode: Some(vec![SegmentMode::Bicycle]),
                    vehicle: None,
                }),
                vehicle: None,
            },
        ]));

        // Forward bicycle - general allowance should NOT override heading-specific denial
        let when_fwd = create_when(
            SegmentHeading::Forward,
            Some(vec![SegmentMode::Bicycle]),
            None,
            None,
        );
        let result_fwd = get_headings(&segment, Some(&when_fwd)).unwrap();
        assert_eq!(
            result_fwd.len(),
            0,
            "General allowance should not override heading-specific denial"
        );

        // Backward bicycle - only general denial, general allowance should override
        let when_bwd = create_when(
            SegmentHeading::Backward,
            Some(vec![SegmentMode::Bicycle]),
            None,
            None,
        );
        let result_bwd = get_headings(&segment, Some(&when_bwd)).unwrap();
        assert_eq!(result_bwd, vec![SegmentHeading::Backward]);
    }

    // #[test]
    // fn test_process_simple_connector_splits_dead_end() {
    //     let mut segment = create_test_segment(None);
    //     segment.id = "seg1".to_string();
    //     segment.connectors = Some(vec![
    //         crate::collection::record::segment::ConnectorReference {
    //             connector_id: "conn1".to_string(),
    //             at: 1.0,
    //         },
    //     ]);

    //     let splits = process_simple_connector_splits(&segment, None).unwrap();
    //     assert_eq!(splits.len(), 2); // 1 for fwd, 1 for bwd
    //     let fwd_split = splits
    //         .iter()
    //         .find(|s| s.heading == SegmentHeading::Forward)
    //         .unwrap();
    //     assert_eq!(fwd_split.src.connector_id, "seg1_start");
    //     assert_eq!(fwd_split.dst.connector_id, "conn1");
    // }

    // #[test]
    // fn test_process_simple_connector_splits_cul_de_sac() {
    //     let mut segment = create_test_segment(None);
    //     segment.id = "seg2".to_string();
    //     segment.connectors = Some(vec![
    //         crate::collection::record::segment::ConnectorReference {
    //             connector_id: "conn2".to_string(),
    //             at: 0.0,
    //         },
    //     ]);

    //     let splits = process_simple_connector_splits(&segment, None).unwrap();
    //     assert_eq!(splits.len(), 2); // 1 for fwd, 1 for bwd
    //     let fwd_split = splits
    //         .iter()
    //         .find(|s| s.heading == SegmentHeading::Forward)
    //         .unwrap();
    //     assert_eq!(fwd_split.src.connector_id, "conn2");
    //     assert_eq!(fwd_split.dst.connector_id, "seg2_end");
    // }
}
