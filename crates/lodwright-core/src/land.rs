// SPDX-License-Identifier: GPL-3.0-only
//! LAND heightfield: decoding the VHGT subrecord of a LAND record into heights.
//!
//! A cell has a 33 x 33 vertex grid, stored as an `f32` offset and signed byte deltas. Adjacent cells share their
//! edge vertices (column 32 of one cell is column 0 of its eastern neighbour), which the raster module relies on.
//!
//! Derived from: the UESP LAND record page (`provenance/sources.toml`: `land-vhgt-layout`,
//! `land-vhgt-integration`).

use crate::Error;

/// Vertices per cell edge.
pub const SIDE: usize = 33;
/// Total bytes of a VHGT payload: offset, deltas, padding.
pub const VHGT_LEN: usize = 4 + SIDE * SIDE + 3;
/// World units per stored height step.
pub const SCALE: f32 = 8.0;

/// The heights of one cell in world units, row-major (`y * SIDE + x`), `SIDE` x `SIDE` values.
#[derive(Debug, Clone, PartialEq)]
pub struct CellHeights {
    heights: Vec<f32>,
}

impl CellHeights {
    /// A flat cell at `height`.
    #[must_use]
    pub fn flat(height: f32) -> Self {
        Self {
            heights: vec![height; SIDE * SIDE],
        }
    }

    /// Height at vertex (`x`, `y`), both below [`SIDE`].
    ///
    /// # Panics
    /// If `x` or `y` is [`SIDE`] or more.
    #[must_use]
    pub fn get(&self, x: usize, y: usize) -> f32 {
        assert!(x < SIDE && y < SIDE, "vertex outside the cell");
        self.heights[y * SIDE + x]
    }

    /// All heights, row-major.
    #[must_use]
    pub fn as_slice(&self) -> &[f32] {
        &self.heights
    }
}

/// Decode a VHGT payload.
///
/// # Errors
/// [`Error::VhgtLength`] when the payload is not exactly [`VHGT_LEN`] bytes, [`Error::VhgtOffset`] when the offset
/// is NaN or infinite or makes a height overflow.
pub fn decode_vhgt(payload: &[u8]) -> Result<CellHeights, Error> {
    if payload.len() != VHGT_LEN {
        return Err(Error::VhgtLength {
            found: payload.len(),
        });
    }
    // source: land-vhgt-layout
    let offset = f32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
    if !(offset * SCALE).is_finite() {
        return Err(Error::VhgtOffset);
    }
    // source: land-vhgt-integration
    let mut heights = Vec::with_capacity(SIDE * SIDE);
    let mut column_start = offset;
    for row in payload[4..4 + SIDE * SIDE].as_chunks::<SIDE>().0 {
        let mut running = column_start;
        for (x, byte) in row.iter().enumerate() {
            running += f32::from(i8::from_le_bytes([*byte]));
            if x == 0 {
                column_start = running;
            }
            heights.push(running * SCALE);
        }
    }
    Ok(CellHeights { heights })
}

#[cfg(test)]
mod tests {
    use super::{CellHeights, SCALE, SIDE, VHGT_LEN, decode_vhgt};
    use crate::Error;
    use lodwright_testkit::land::{Heights, encode_vhgt};
    use lodwright_testkit::rng::Rng;

    fn payload(offset: f32, deltas: impl Fn(usize, usize) -> i8) -> Vec<u8> {
        let mut bytes = offset.to_le_bytes().to_vec();
        for y in 0..SIDE {
            for x in 0..SIDE {
                bytes.push(deltas(x, y).to_le_bytes()[0]);
            }
        }
        bytes.extend_from_slice(&[0, 0, 0]);
        bytes
    }

    #[test]
    fn all_zero_deltas_give_a_flat_cell_at_the_offset_times_the_scale() {
        let cell = decode_vhgt(&payload(10.0, |_, _| 0)).unwrap();
        assert_eq!(cell, CellHeights::flat(10.0 * SCALE));
    }

    #[test]
    fn a_row_accumulates_along_x() {
        let cell = decode_vhgt(&payload(0.0, |x, y| i8::from(x > 0 && y == 0))).unwrap();
        assert_eq!(cell.get(0, 0), 0.0);
        assert_eq!(cell.get(1, 0), SCALE);
        assert_eq!(cell.get(32, 0), 32.0 * SCALE);
    }

    #[test]
    fn the_first_column_accumulates_down_y_and_seeds_each_row() {
        let cell = decode_vhgt(&payload(0.0, |x, _| i8::from(x == 0))).unwrap();
        assert_eq!(cell.get(0, 3), 4.0 * SCALE);
        assert_eq!(cell.get(5, 3), 4.0 * SCALE);
    }

    #[test]
    fn negative_deltas_lower_the_terrain() {
        let cell =
            decode_vhgt(&payload(0.0, |x, y| if x == 1 && y == 0 { -3 } else { 0 })).unwrap();
        assert_eq!(cell.get(2, 0), -3.0 * SCALE);
    }

    #[test]
    fn a_wrong_length_is_rejected_with_the_length_found() {
        assert_eq!(decode_vhgt(&[0; 10]), Err(Error::VhgtLength { found: 10 }));
        assert_eq!(decode_vhgt(&[]), Err(Error::VhgtLength { found: 0 }));
        let long = vec![0; VHGT_LEN + 1];
        assert_eq!(
            decode_vhgt(&long),
            Err(Error::VhgtLength {
                found: VHGT_LEN + 1
            })
        );
    }

    #[test]
    fn non_finite_offsets_are_rejected() {
        assert_eq!(
            decode_vhgt(&payload(f32::NAN, |_, _| 0)),
            Err(Error::VhgtOffset)
        );
        assert_eq!(
            decode_vhgt(&payload(f32::INFINITY, |_, _| 0)),
            Err(Error::VhgtOffset)
        );
        assert_eq!(
            decode_vhgt(&payload(f32::MAX, |_, _| 0)),
            Err(Error::VhgtOffset)
        );
    }

    #[test]
    fn the_padding_bytes_do_not_matter() {
        let mut a = payload(1.0, |x, _| i8::from(x < 3));
        let mut b = a.clone();
        let n = a.len();
        a[n - 1] = 0;
        b[n - 1] = 0xFF;
        assert_eq!(decode_vhgt(&a), decode_vhgt(&b));
    }

    #[test]
    fn random_walks_on_the_grid_round_trip_exactly() {
        let mut rng = Rng::new(0x5EED);
        for _ in 0..200 {
            let base = f32::from(i16::try_from(rng.below(2000)).unwrap() - 1000);
            let mut grid = vec![0.0_f32; SIDE * SIDE];
            for y in 0..SIDE {
                for x in 0..SIDE {
                    let step = f32::from(i8::try_from(rng.below(41)).unwrap() - 20);
                    grid[y * SIDE + x] = match (x, y) {
                        (0, 0) => base,
                        (0, _) => grid[(y - 1) * SIDE] + step,
                        _ => grid[y * SIDE + x - 1] + step,
                    };
                }
            }
            let heights = Heights(grid.iter().map(|s| s * SCALE).collect());
            let cell = decode_vhgt(&encode_vhgt(&heights)).unwrap();
            assert_eq!(cell.as_slice(), heights.0.as_slice());
        }
    }

    #[test]
    fn raising_every_height_by_whole_steps_changes_only_the_offset() {
        let mut rng = Rng::new(7);
        for _ in 0..50 {
            let low = Heights::from_fn(|x, y| {
                8.0 * f32::from(u8::try_from((x * 3 + y * 2) % 50).unwrap())
            });
            let shift = f32::from(i8::try_from(rng.below(10) + 1).unwrap());
            let high = Heights(low.0.iter().map(|h| h + shift * SCALE).collect());
            let (a, b) = (encode_vhgt(&low), encode_vhgt(&high));
            assert_eq!(a[4..], b[4..]);
            assert_ne!(a[..4], b[..4]);
        }
    }

    #[test]
    fn arbitrary_heights_decode_within_half_a_step() {
        let mut rng = Rng::new(99);
        for _ in 0..100 {
            let noise: Vec<f32> = (0..SIDE)
                .map(|_| f32::from(u8::try_from(rng.below(8)).unwrap()))
                .collect();
            let heights =
                Heights::from_fn(|x, _| f32::from(u8::try_from(x).unwrap()) * 5.0 + noise[x]);
            let cell = decode_vhgt(&encode_vhgt(&heights)).unwrap();
            for (a, b) in heights.0.iter().zip(cell.as_slice()) {
                assert!((a - b).abs() <= SCALE / 2.0 + f32::EPSILON, "{a} vs {b}");
            }
        }
    }

    #[test]
    #[should_panic(expected = "vertex outside the cell")]
    fn reading_outside_the_grid_panics() {
        let _ = CellHeights::flat(0.0).get(SIDE, 0);
    }
}
