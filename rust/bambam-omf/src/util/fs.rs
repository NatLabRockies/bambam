use std::path::Path;

use serde::Serialize;

use crate::collection::OvertureMapsCollectionError;

/// copies the default `bambam.toml` configuration to the directory of an OMF import.
pub fn copy_default_config(output_directory: &Path) -> Result<(), OvertureMapsCollectionError> {
    bambam_network::fs::copy_default_config(output_directory).map_err(Into::into)
}

/// helper function to "mkdir -p path" - make all directories along a path
pub fn create_dirs<P>(path: P) -> Result<(), OvertureMapsCollectionError>
where
    P: AsRef<Path>,
{
    bambam_network::fs::create_dirs(path).map_err(Into::into)
}

pub fn serialize_into_csv<I>(
    iterable: I,
    filename: &str,
    output_directory: &Path,
    overwrite: bool,
    desc: &str,
) -> Result<(), OvertureMapsCollectionError>
where
    I: IntoIterator,
    I::IntoIter: ExactSizeIterator,
    I::Item: Serialize,
{
    bambam_network::fs::serialize_into_csv(iterable, filename, output_directory, overwrite, desc)
        .map_err(Into::into)
}

pub fn serialize_into_enumerated_txt<I>(
    iterable: I,
    filename: &str,
    output_directory: &Path,
    overwrite: bool,
    desc: &str,
) -> Result<(), OvertureMapsCollectionError>
where
    I: IntoIterator,
    I::IntoIter: ExactSizeIterator,
    I::Item: Serialize,
{
    bambam_network::fs::serialize_into_enumerated_txt(
        iterable,
        filename,
        output_directory,
        overwrite,
        desc,
    )
    .map_err(Into::into)
}
