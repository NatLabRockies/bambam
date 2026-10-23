use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SegmentVehicleDimension {
    AxleCount,
    Height,
    Length,
    Weight,
    Width,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SegmentVehicleComparator {
    GreaterThan,
    GreaterThanEqual,
    Equal,
    LessThan,
    LessThanEqual,
}

impl SegmentVehicleComparator {
    pub fn apply(&self, value: f64, restriction: f64) -> bool {
        match self {
            SegmentVehicleComparator::GreaterThan => value > restriction,
            SegmentVehicleComparator::GreaterThanEqual => value >= restriction,
            SegmentVehicleComparator::Equal => value == restriction,
            SegmentVehicleComparator::LessThan => value < restriction,
            SegmentVehicleComparator::LessThanEqual => value <= restriction,
        }
    }
}

/// units in vehicle restrictions which may be length or weight units.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SegmentUnit {
    Length(SegmentLengthUnit),
    Weight(SegmentWeightUnit),
}

impl SegmentUnit {
    pub fn as_str(&self) -> &'static str {
        match self {
            SegmentUnit::Length(SegmentLengthUnit::Inches) => "in",
            SegmentUnit::Length(SegmentLengthUnit::Feet) => "ft",
            SegmentUnit::Length(SegmentLengthUnit::Yard) => "yd",
            SegmentUnit::Length(SegmentLengthUnit::Mile) => "mi",
            SegmentUnit::Length(SegmentLengthUnit::Centimeter) => "cm",
            SegmentUnit::Length(SegmentLengthUnit::Meter) => "m",
            SegmentUnit::Length(SegmentLengthUnit::Kilometer) => "km",
            SegmentUnit::Weight(SegmentWeightUnit::Imperial(SegmentImperialWeightUnit::Ounce)) => {
                "oz"
            }
            SegmentUnit::Weight(SegmentWeightUnit::Imperial(SegmentImperialWeightUnit::Pound)) => {
                "lb"
            }
            SegmentUnit::Weight(SegmentWeightUnit::Imperial(SegmentImperialWeightUnit::Stone)) => {
                "st"
            }
            SegmentUnit::Weight(SegmentWeightUnit::Imperial(
                SegmentImperialWeightUnit::LongTon,
            )) => "lt",
            SegmentUnit::Weight(SegmentWeightUnit::Metric(SegmentMetricWeightUnit::Gram)) => "g",
            SegmentUnit::Weight(SegmentWeightUnit::Metric(SegmentMetricWeightUnit::Kilogram)) => {
                "kg"
            }
            SegmentUnit::Weight(SegmentWeightUnit::Metric(SegmentMetricWeightUnit::MetricTon)) => {
                "t"
            }
        }
    }
}

impl Serialize for SegmentUnit {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SegmentUnit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "in" => Ok(SegmentUnit::Length(SegmentLengthUnit::Inches)),
            "ft" => Ok(SegmentUnit::Length(SegmentLengthUnit::Feet)),
            "yd" => Ok(SegmentUnit::Length(SegmentLengthUnit::Yard)),
            "mi" => Ok(SegmentUnit::Length(SegmentLengthUnit::Mile)),
            "cm" => Ok(SegmentUnit::Length(SegmentLengthUnit::Centimeter)),
            "m" => Ok(SegmentUnit::Length(SegmentLengthUnit::Meter)),
            "km" => Ok(SegmentUnit::Length(SegmentLengthUnit::Kilometer)),
            "oz" => Ok(SegmentUnit::Weight(SegmentWeightUnit::Imperial(
                SegmentImperialWeightUnit::Ounce,
            ))),
            "lb" => Ok(SegmentUnit::Weight(SegmentWeightUnit::Imperial(
                SegmentImperialWeightUnit::Pound,
            ))),
            "st" => Ok(SegmentUnit::Weight(SegmentWeightUnit::Imperial(
                SegmentImperialWeightUnit::Stone,
            ))),
            "lt" => Ok(SegmentUnit::Weight(SegmentWeightUnit::Imperial(
                SegmentImperialWeightUnit::LongTon,
            ))),
            "g" => Ok(SegmentUnit::Weight(SegmentWeightUnit::Metric(
                SegmentMetricWeightUnit::Gram,
            ))),
            "kg" => Ok(SegmentUnit::Weight(SegmentWeightUnit::Metric(
                SegmentMetricWeightUnit::Kilogram,
            ))),
            "t" => Ok(SegmentUnit::Weight(SegmentWeightUnit::Metric(
                SegmentMetricWeightUnit::MetricTon,
            ))),
            "" => Ok(SegmentUnit::Length(SegmentLengthUnit::Meter)),
            other => Err(serde::de::Error::custom(format!(
                "unknown SegmentUnit '{other}'"
            ))),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum SegmentLengthUnit {
    #[serde(rename = "in")]
    Inches,
    #[serde(rename = "ft")]
    Feet,
    #[serde(rename = "yd")]
    Yard,
    #[serde(rename = "mi")]
    Mile,
    #[serde(rename = "cm")]
    Centimeter,
    #[serde(rename = "m")]
    Meter,
    #[serde(rename = "km")]
    Kilometer,
}

impl SegmentLengthUnit {
    pub fn to_uom(&self, value: f64) -> uom::si::f64::Length {
        match self {
            SegmentLengthUnit::Inches => uom::si::f64::Length::new::<uom::si::length::inch>(value),
            SegmentLengthUnit::Feet => uom::si::f64::Length::new::<uom::si::length::foot>(value),
            SegmentLengthUnit::Yard => uom::si::f64::Length::new::<uom::si::length::yard>(value),
            SegmentLengthUnit::Mile => uom::si::f64::Length::new::<uom::si::length::mile>(value),
            SegmentLengthUnit::Centimeter => {
                uom::si::f64::Length::new::<uom::si::length::centimeter>(value)
            }
            SegmentLengthUnit::Meter => uom::si::f64::Length::new::<uom::si::length::meter>(value),
            SegmentLengthUnit::Kilometer => {
                uom::si::f64::Length::new::<uom::si::length::kilometer>(value)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SegmentWeightUnit {
    Imperial(SegmentImperialWeightUnit),
    Metric(SegmentMetricWeightUnit),
}

impl SegmentWeightUnit {
    pub fn to_uom(&self, value: f64) -> uom::si::f64::Mass {
        use SegmentImperialWeightUnit as I;
        use SegmentMetricWeightUnit as M;
        use SegmentWeightUnit as SWU;

        match self {
            SWU::Imperial(I::Ounce) => uom::si::f64::Mass::new::<uom::si::mass::ounce>(value),
            SWU::Imperial(I::Pound) => uom::si::f64::Mass::new::<uom::si::mass::pound>(value),
            // Couldn't find "Stone" so we use the transformation to Kg
            SWU::Imperial(I::Stone) => {
                uom::si::f64::Mass::new::<uom::si::mass::kilogram>(value * 6.350288)
            }
            SWU::Imperial(I::LongTon) => uom::si::f64::Mass::new::<uom::si::mass::ton_long>(value),
            SWU::Metric(M::Kilogram) => uom::si::f64::Mass::new::<uom::si::mass::kilogram>(value),
            SWU::Metric(M::Gram) => uom::si::f64::Mass::new::<uom::si::mass::gram>(value),
            SWU::Metric(M::MetricTon) => uom::si::f64::Mass::new::<uom::si::mass::ton>(value),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SegmentImperialWeightUnit {
    #[serde(rename = "oz")]
    Ounce,
    #[serde(rename = "lb")]
    Pound,
    #[serde(rename = "st")]
    Stone,
    #[serde(rename = "lt")]
    LongTon,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SegmentMetricWeightUnit {
    #[serde(rename = "g")]
    Gram,
    #[serde(rename = "kg")]
    Kilogram,
    #[serde(rename = "t")]
    MetricTon,
}
