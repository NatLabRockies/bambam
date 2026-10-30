pub use bambam_network::SegmentSplit;

use crate::collection::{record::SegmentHeading, SegmentSpeedLimit};

/// helper function which confirms that speed data exists and that it matches the current heading
pub(crate) fn has_max_speed_for_heading(s: &SegmentSpeedLimit, heading: &SegmentHeading) -> bool {
    // no max speed? return early
    if s.max_speed.as_ref().is_none() {
        return false;
    }

    let when = match s.when.as_ref() {
        Some(w) => w,
        None => return true, // no access restrictions to apply
    };

    match when.heading.as_ref() {
        None => true, // no heading restrictions to apply
        Some(h) if h == heading => true,
        _ => false,
    }
}

#[cfg(test)]
mod test {
    use crate::collection::{
        record::{SegmentHeading, SpeedLimitWithUnit},
        SegmentAccessRestrictionWhen, SegmentSpeedLimit, SegmentSpeedUnit,
    };

    #[test]
    fn no_maxspeed_entry() {
        // unexpected case where the record has a min speed but no max speed
        let record = SegmentSpeedLimit {
            min_speed: Some(SpeedLimitWithUnit {
                value: 35,
                unit: SegmentSpeedUnit::Mph,
            }),
            max_speed: None,
            is_max_speed_variable: None,
            when: Some(SegmentAccessRestrictionWhen {
                during: None,
                heading: Some(SegmentHeading::Backward),
                using: None,
                recognized: None,
                mode: None,
                vehicle: None,
            }),
            between: Some(vec![0.0, 0.418244116]),
        };
        let heading = SegmentHeading::Forward;
        let result = super::has_max_speed_for_heading(&record, &heading);
        assert!(!result)
    }
}
