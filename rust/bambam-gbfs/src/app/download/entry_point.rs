use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// target of an initial HTTP call to a GBFS archive.
/// for an explanation, see <https://gbfs.org/get-started/#2-transform-your-data-into-gbfs-structure>.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, ValueEnum, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EntryPoint {
    /// manifest.json file
    Manifest,
    /// gbfs.json file for a specific GBFS version.
    Gbfs,
}

impl std::fmt::Display for EntryPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryPoint::Manifest => write!(f, "manifest"),
            EntryPoint::Gbfs => write!(f, "gbfs"),
        }
    }
}
