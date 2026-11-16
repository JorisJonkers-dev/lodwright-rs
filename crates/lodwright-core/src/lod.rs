// SPDX-License-Identifier: MPL-2.0
//! The per-world LOD settings file (`lodsettings/<world>.dlodsettings`): 24 bytes, little endian.
//!
//! Layout from the row `game-dlodsettings-layout` (measured on two files of the owner's install).

use crate::Error;

/// Size of the file in bytes.
pub const SETTINGS_LEN: usize = 24;

/// The contents of a `.dlodsettings` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LodSettings {
    /// Lowest terrain level (4).
    pub lowest_level: u32,
    /// Highest terrain level (32, or 16 for a small world).
    pub highest_level: u32,
    /// Width of the world in cells.
    pub width_cells: u32,
    /// South-west cell, x then y.
    pub south_west: (i16, i16),
    /// North-east cell, x then y.
    pub north_east: (i16, i16),
    /// The last field; equals the object LOD level (4) in every file measured.
    pub object_level: u32,
}

impl LodSettings {
    /// Settings for a world spanning `south_west` to `north_east` (inclusive cells) with terrain levels up to
    /// `highest_level`.
    #[must_use]
    pub fn for_bounds(south_west: (i16, i16), north_east: (i16, i16), highest_level: u32) -> Self {
        let width = i32::from(north_east.0) - i32::from(south_west.0) + 1;
        Self {
            lowest_level: 4,
            highest_level,
            width_cells: u32::try_from(width).unwrap_or(0),
            south_west,
            north_east,
            object_level: 4,
        }
    }

    // source: game-dlodsettings-layout
    /// The file bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; SETTINGS_LEN] {
        let mut out = [0_u8; SETTINGS_LEN];
        out[0..4].copy_from_slice(&self.lowest_level.to_le_bytes());
        out[4..8].copy_from_slice(&self.highest_level.to_le_bytes());
        out[8..12].copy_from_slice(&self.width_cells.to_le_bytes());
        out[12..14].copy_from_slice(&self.south_west.0.to_le_bytes());
        out[14..16].copy_from_slice(&self.south_west.1.to_le_bytes());
        out[16..18].copy_from_slice(&self.north_east.0.to_le_bytes());
        out[18..20].copy_from_slice(&self.north_east.1.to_le_bytes());
        out[20..24].copy_from_slice(&self.object_level.to_le_bytes());
        out
    }

    // source: game-dlodsettings-layout
    /// Parse a file.
    ///
    /// # Errors
    /// [`Error::BadSettings`] unless `bytes` is exactly [`SETTINGS_LEN`] bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() != SETTINGS_LEN {
            return Err(Error::BadSettings("a .dlodsettings file is 24 bytes"));
        }
        let u32_at =
            |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        let i16_at = |i: usize| i16::from_le_bytes([bytes[i], bytes[i + 1]]);
        Ok(Self {
            lowest_level: u32_at(0),
            highest_level: u32_at(4),
            width_cells: u32_at(8),
            south_west: (i16_at(12), i16_at(14)),
            north_east: (i16_at(16), i16_at(18)),
            object_level: u32_at(20),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{LodSettings, SETTINGS_LEN};

    #[test]
    fn a_world_of_128_cells_has_the_measured_bytes() {
        let s = LodSettings::for_bounds((-64, -64), (63, 63), 32);
        assert_eq!(
            s.to_bytes(),
            [
                4, 0, 0, 0, 32, 0, 0, 0, 128, 0, 0, 0, 0xc0, 0xff, 0xc0, 0xff, 0x3f, 0, 0x3f, 0, 4,
                0, 0, 0
            ]
        );
    }

    #[test]
    fn parse_inverts_write() {
        let s = LodSettings::for_bounds((-32, -10), (31, 20), 16);
        assert_eq!(LodSettings::parse(&s.to_bytes()), Ok(s));
    }

    #[test]
    fn a_wrong_size_is_refused() {
        assert!(LodSettings::parse(&[0; SETTINGS_LEN - 1]).is_err());
        assert!(LodSettings::parse(&[0; SETTINGS_LEN + 1]).is_err());
    }
}
