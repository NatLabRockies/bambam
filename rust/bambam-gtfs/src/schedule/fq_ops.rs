pub const EMPTY_FEED_PLACEHOLDER: &str = "()";
pub const EMPTY_AGENCY_PLACEHOLDER: &str = "()";

pub const FQ_METADATA_FIELDNAME: &str = "fq_route_ids";

pub const FQ_ROUTE_ID_SEPARATOR: &str = "->";

/// the concatenation of the edge list, feed, agency, route, and service id.
///
/// names are cleaned of commas for CSV compatibility.
///
/// in order to allow for deconstruction of this fully-qualified name,
/// we use a non-standard separator of multiple characters, as per the
/// GTFS specification, ID types can contain any UTF-8 characters. see
/// [https://gtfs.org/documentation/schedule/reference/#field-types].
pub fn get_fully_qualified_route_id(
    feed_id: Option<&str>,
    agency_id: Option<&str>,
    route_id: &str,
    service_id: &str,
    edge_list_id: usize,
) -> String {
    let feed_id = match &feed_id {
        Some(id) if !id.is_empty() => id,
        _ => EMPTY_FEED_PLACEHOLDER,
    };
    let agency_id = match &agency_id {
        Some(id) if !id.is_empty() => id,
        _ => EMPTY_AGENCY_PLACEHOLDER,
    };
    let name = format!(
        "{edge_list_id}{FQ_ROUTE_ID_SEPARATOR}{feed_id}{FQ_ROUTE_ID_SEPARATOR}{agency_id}{FQ_ROUTE_ID_SEPARATOR}{route_id}{FQ_ROUTE_ID_SEPARATOR}{service_id}"
    );

    name.replace(",", "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fully_qualified_route_id_with_feed_and_agency() {
        let fq_id = get_fully_qualified_route_id(
            Some("denver_rtd"),
            Some("RTD"),
            "15",
            "weekday",
            1,
        );
        assert_eq!(fq_id, "1->denver_rtd->RTD->15->weekday");
    }

    #[test]
    fn test_fully_qualified_route_id_empty_feed_and_agency() {
        let fq_id = get_fully_qualified_route_id(None, None, "15", "weekday", 0);
        assert_eq!(fq_id, "0->()->()->15->weekday");
    }

    #[test]
    fn test_fully_qualified_route_id_cleans_commas() {
        let fq_id = get_fully_qualified_route_id(
            Some("feed,1"),
            Some("agency,1"),
            "route,1",
            "service,1",
            2,
        );
        assert_eq!(fq_id, "2->feed_1->agency_1->route_1->service_1");
    }
}
