// use bamsoda_core::model::identifier::Geoid;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Grouping {
    pub identifier: String,
    pub mode: String,
}

impl Grouping {
    pub fn new(geoid: String, mode: String) -> Grouping {
        Grouping {
            identifier: geoid,
            mode,
        }
    }
}
