// SPDX-License-Identifier: GPL-3.0-only
//! Errors of the generator.

use std::fmt;

/// Why generation failed.
#[derive(Debug)]
pub enum GenError {
    /// Planning or raster building failed.
    Core(lodwright_core::Error),
    /// A NIF could not be written.
    Nif(lodwright_nif::NifError),
    /// A texture could not be encoded.
    Dds(lodwright_dds::DdsError),
    /// A file or folder could not be written; `what` names the path relative to the output folder.
    Io {
        /// The relative path.
        what: String,
        /// The operating system's error.
        source: std::io::Error,
    },
    /// A worker thread panicked.
    Worker,
    /// A path under the output folder is not plain (absolute, or with `..`) or passes a folder that is a link;
    /// nothing was written there. `what` names the path relative to the output folder.
    LinkInOutput {
        /// The relative path.
        what: String,
    },
}

impl fmt::Display for GenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(e) => write!(f, "{e}"),
            Self::Nif(e) => write!(f, "writing a block mesh: {e}"),
            Self::Dds(e) => write!(f, "encoding a texture: {e}"),
            Self::Io { what, source } => write!(f, "cannot write {what}: {source}"),
            Self::Worker => f.write_str("a worker thread failed"),
            Self::LinkInOutput { what } => write!(
                f,
                "refusing to write {what}: it leaves the output folder or passes a link inside it"
            ),
        }
    }
}

impl std::error::Error for GenError {}

impl From<lodwright_core::Error> for GenError {
    fn from(e: lodwright_core::Error) -> Self {
        Self::Core(e)
    }
}

impl From<lodwright_nif::NifError> for GenError {
    fn from(e: lodwright_nif::NifError) -> Self {
        Self::Nif(e)
    }
}

impl From<lodwright_dds::DdsError> for GenError {
    fn from(e: lodwright_dds::DdsError) -> Self {
        Self::Dds(e)
    }
}
