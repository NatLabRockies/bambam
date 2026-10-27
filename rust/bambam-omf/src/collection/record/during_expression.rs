#[allow(unused_imports)]
pub use bambam_network::DuringExpression;

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
