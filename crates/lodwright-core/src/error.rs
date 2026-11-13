// SPDX-License-Identifier: MPL-2.0
//! Error type shared by the core modules.

use std::fmt;

/// What can go wrong in the core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A block size of zero cells was requested.
    ZeroBlockSize,
    /// A VHGT payload did not have the one length the format allows.
    VhgtLength {
        /// Bytes found.
        found: usize,
    },
    /// The VHGT offset was NaN or so large that heights overflow.
    VhgtOffset,
    /// The input ended inside a header, record or group.
    Truncated {
        /// What was being read.
        what: &'static str,
    },
    /// The input is not shaped like a plugin (bad signature, impossible size, nesting too deep).
    Malformed(&'static str),
    /// A compressed record did not inflate, or claimed more than the allowed size.
    Decompress,
    /// A plugin names a master that is not in the load order.
    MissingMaster(String),
    /// A raster over the worldspace would need more samples than the allowed maximum.
    RasterTooLarge {
        /// Samples the raster would hold.
        samples: u128,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroBlockSize => f.write_str("a block must span at least one cell"),
            Self::VhgtLength { found } => write!(
                f,
                "a VHGT payload is {} bytes, found {found}",
                crate::land::VHGT_LEN
            ),
            Self::VhgtOffset => f.write_str("the VHGT offset is not a usable number"),
            Self::Truncated { what } => write!(f, "the plugin ends inside {what}"),
            Self::Malformed(why) => write!(f, "not a readable plugin: {why}"),
            Self::Decompress => f.write_str("a compressed record could not be inflated"),
            Self::MissingMaster(name) => {
                write!(f, "master {name} is not in the load order")
            }
            Self::RasterTooLarge { samples } => write!(
                f,
                "the height raster would need {samples} samples, more than {}",
                crate::raster::MAX_SAMPLES
            ),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::Error;

    #[test]
    fn zero_block_size_error_names_the_problem() {
        assert_eq!(
            Error::ZeroBlockSize.to_string(),
            "a block must span at least one cell"
        );
    }

    #[test]
    fn vhgt_length_error_names_both_lengths() {
        assert_eq!(
            Error::VhgtLength { found: 7 }.to_string(),
            "a VHGT payload is 1096 bytes, found 7"
        );
    }

    #[test]
    fn plugin_errors_name_what_went_wrong() {
        assert_eq!(
            Error::Truncated { what: "a record" }.to_string(),
            "the plugin ends inside a record"
        );
        assert_eq!(
            Error::Malformed("bad size").to_string(),
            "not a readable plugin: bad size"
        );
        assert_eq!(
            Error::Decompress.to_string(),
            "a compressed record could not be inflated"
        );
        assert_eq!(
            Error::MissingMaster("A.esm".into()).to_string(),
            "master A.esm is not in the load order"
        );
    }

    #[test]
    fn raster_too_large_names_the_sizes() {
        let text = Error::RasterTooLarge { samples: 1 << 40 }.to_string();
        assert!(text.contains("1099511627776"));
        assert!(text.contains("134217728"));
    }

    #[test]
    fn vhgt_offset_error_names_the_problem() {
        assert_eq!(
            Error::VhgtOffset.to_string(),
            "the VHGT offset is not a usable number"
        );
    }
}
