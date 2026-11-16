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
    /// A `.dlodsettings` file has the wrong size.
    BadSettings(&'static str),
    /// A compressed record did not inflate, or claimed more than the allowed size.
    Decompress,
    /// A plugin names a master that is not in the load order.
    MissingMaster(String),
    /// A worldspace name cannot be used as a folder or file name.
    BadWorldName(String),
    /// A LOD level of zero cells was requested.
    ZeroLevel,
    /// A LAND layer subrecord (VNML, VCLR, BTXT, ATXT or VTXT) has a length that subrecord cannot have.
    SubrecordLength {
        /// Subrecord signature.
        what: &'static str,
        /// Bytes found.
        found: usize,
    },
    /// A LAND layer names a quadrant above 3.
    LayerQuadrant(u8),
    /// A VTXT point is beyond position 288 of the 17 x 17 quadrant grid.
    LayerPosition(u16),
    /// A VTXT opacity is NaN or infinite.
    LayerOpacity,
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
            Self::BadSettings(why) => write!(f, "not a LOD settings file: {why}"),
            Self::Decompress => f.write_str("a compressed record could not be inflated"),
            Self::MissingMaster(name) => {
                write!(f, "master {name} is not in the load order")
            }
            Self::BadWorldName(name) => {
                write!(f, "worldspace name {name:?} cannot be used in a path")
            }
            Self::ZeroLevel => f.write_str("a LOD level must span at least one cell"),
            Self::SubrecordLength { what, found } => {
                write!(f, "a {what} subrecord cannot be {found} bytes long")
            }
            Self::LayerQuadrant(q) => write!(f, "a LAND layer names quadrant {q}, the last is 3"),
            Self::LayerPosition(p) => {
                write!(f, "a VTXT point is at position {p}, the last is 288")
            }
            Self::LayerOpacity => f.write_str("a VTXT opacity is not a finite number"),
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
    fn layout_errors_name_the_problem() {
        assert_eq!(
            Error::BadWorldName("a/b".into()).to_string(),
            "worldspace name \"a/b\" cannot be used in a path"
        );
        assert_eq!(
            Error::ZeroLevel.to_string(),
            "a LOD level must span at least one cell"
        );
    }

    #[test]
    fn layer_errors_name_the_problem() {
        assert_eq!(
            Error::SubrecordLength {
                what: "VNML",
                found: 5
            }
            .to_string(),
            "a VNML subrecord cannot be 5 bytes long"
        );
        assert_eq!(
            Error::LayerQuadrant(4).to_string(),
            "a LAND layer names quadrant 4, the last is 3"
        );
        assert_eq!(
            Error::LayerPosition(300).to_string(),
            "a VTXT point is at position 300, the last is 288"
        );
        assert_eq!(
            Error::LayerOpacity.to_string(),
            "a VTXT opacity is not a finite number"
        );
    }

    #[test]
    fn vhgt_offset_error_names_the_problem() {
        assert_eq!(
            Error::VhgtOffset.to_string(),
            "the VHGT offset is not a usable number"
        );
    }
}
