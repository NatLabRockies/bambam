use opening_hours_syntax::rules::OpeningHoursExpression;
use serde::{Deserialize, Serialize};

/// the opening hours for an OvertureMaps record. because of the existence of
/// invalid data, this is provided with a fallback Unexpected variant when
/// parsing as opening hours fails. see documentation for the OSM hours for
/// more information.
///
/// <https://wiki.openstreetmap.org/wiki/Key:opening_hours>
#[derive(Debug, Clone)]
pub enum DuringExpression {
    Osm(OpeningHoursExpression),
    Unexpected(String),
}

impl Serialize for DuringExpression {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            DuringExpression::Osm(expr) => serializer.serialize_str(&expr.to_string()),
            DuringExpression::Unexpected(s) => serializer.serialize_str(s),
        }
    }
}

impl<'de> Deserialize<'de> for DuringExpression {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match opening_hours_syntax::parse(&s) {
            Ok(expr) => Ok(DuringExpression::Osm(expr)),
            Err(_) => Ok(DuringExpression::Unexpected(s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_opening_hours() {
        let json = r#""07:00-16:00""#;
        let result: DuringExpression = serde_json::from_str(json).unwrap();
        assert!(matches!(result, DuringExpression::Osm(_)));
    }

    #[test]
    fn test_parse_invalid_opening_hours_as_unexpected() {
        let json = r#""sunset""#;
        let result: DuringExpression = serde_json::from_str(json).unwrap();
        assert!(matches!(result, DuringExpression::Unexpected(s) if s == "sunset"));
    }
}
