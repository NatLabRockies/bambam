mod destination;
mod flags;
mod record;
mod route;
mod speed_limit;
mod value_between;

pub use bambam_network::{
    SegmentAccessRestriction, SegmentAccessRestrictionWhen, SegmentAccessRestrictionWhenVehicle,
    SegmentAccessType, SegmentClass, SegmentFullType, SegmentHeading, SegmentImperialWeightUnit,
    SegmentLengthUnit, SegmentMetricWeightUnit, SegmentMode, SegmentRecognized, SegmentSubclass,
    SegmentSubtype, SegmentUnit, SegmentUsing, SegmentVehicleComparator, SegmentVehicleDimension,
    SegmentWeightUnit,
};
pub use destination::{
    SegmentDestination, SegmentDestinationLabel, SegmentDestinationLabelType,
    SegmentDestinationWhen, SegmentProhibitedTransitions, SegmentProhibitedTransitionsSequence,
    SegmentSymbol,
};
pub use flags::{SegmentRailFlags, SegmentRoadFlags, SegmentRoadSurfaceType};
pub use record::TransportationSegmentRecord;
pub use route::{ConnectorReference, SegmentRoute};
pub use speed_limit::{SegmentSpeedLimit, SegmentSpeedUnit, SpeedLimitWithUnit};
pub use value_between::SegmentValueBetween;
