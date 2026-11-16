// SPDX-License-Identifier: GPL-3.0-only
//! Error type of the asset readers.

use std::fmt;

/// What can go wrong reading an archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The input is not a BSA (wrong magic).
    NotAnArchive,
    /// A BSA version other than 104.
    UnsupportedVersion(u32),
    /// A valid v104 archive that uses something this reader does not handle.
    Unsupported(&'static str),
    /// The input ends inside the named part.
    Truncated {
        /// What was being read.
        what: &'static str,
    },
    /// The archive contradicts its own header.
    Malformed(&'static str),
    /// An entry would need more bytes than the caller allows.
    TooLarge {
        /// Bytes the entry claims.
        claimed: u64,
        /// Bytes the caller allows.
        cap: u64,
    },
    /// A compressed entry does not inflate to the size it claims.
    Decompress,
    /// The underlying reader failed.
    Io(std::io::ErrorKind),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAnArchive => f.write_str("not a BSA archive"),
            Self::UnsupportedVersion(v) => write!(f, "BSA version {v} is not supported (104 is)"),
            Self::Unsupported(what) => write!(f, "unsupported archive: {what}"),
            Self::Truncated { what } => write!(f, "the archive ends inside {what}"),
            Self::Malformed(why) => write!(f, "malformed archive: {why}"),
            Self::TooLarge { claimed, cap } => {
                write!(f, "entry needs {claimed} bytes, more than the cap of {cap}")
            }
            Self::Decompress => f.write_str("a compressed entry could not be inflated"),
            Self::Io(kind) => write!(f, "read failed: {kind}"),
        }
    }
}

impl std::error::Error for Error {}

impl Error {
    /// Map an I/O error: a short read is a truncation of `what`.
    pub(crate) fn from_io(error: &std::io::Error, what: &'static str) -> Self {
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            Self::Truncated { what }
        } else {
            Self::Io(error.kind())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Error;

    #[test]
    fn messages_name_the_problem() {
        assert_eq!(Error::NotAnArchive.to_string(), "not a BSA archive");
        assert_eq!(
            Error::UnsupportedVersion(105).to_string(),
            "BSA version 105 is not supported (104 is)"
        );
        assert_eq!(
            Error::Truncated { what: "the header" }.to_string(),
            "the archive ends inside the header"
        );
        assert_eq!(
            Error::TooLarge { claimed: 9, cap: 4 }.to_string(),
            "entry needs 9 bytes, more than the cap of 4"
        );
        assert_eq!(
            Error::Decompress.to_string(),
            "a compressed entry could not be inflated"
        );
        assert_eq!(
            Error::Unsupported("x").to_string(),
            "unsupported archive: x"
        );
        assert_eq!(Error::Malformed("y").to_string(), "malformed archive: y");
        assert_eq!(
            Error::Io(std::io::ErrorKind::NotFound).to_string(),
            "read failed: entity not found"
        );
    }

    #[test]
    fn a_short_read_maps_to_truncated() {
        let eof = std::io::Error::from(std::io::ErrorKind::UnexpectedEof);
        assert_eq!(
            Error::from_io(&eof, "a record"),
            Error::Truncated { what: "a record" }
        );
        let other = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        assert_eq!(
            Error::from_io(&other, "a record"),
            Error::Io(std::io::ErrorKind::PermissionDenied)
        );
    }
}
