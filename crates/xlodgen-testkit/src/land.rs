// SPDX-License-Identifier: MPL-2.0
//! Synthetic LAND heights: encoding a 33x33 vertex grid as VHGT bytes and decoding it again.
//!
//! Layout used: a little-endian `f32` offset, then 33 x 33 signed byte deltas, then three padding bytes. Row
//! `y`, column `x`: the first column integrates downwards (each row's first delta is added to the previous
//! row's first height), every other column integrates along the row. Heights are the running sum plus the
//! offset, times a scale of 8.

/// Vertices per cell edge.
pub const SIDE: usize = 33;
/// Total bytes of a VHGT payload.
pub const VHGT_LEN: usize = 4 + SIDE * SIDE + 3;
/// World units per stored height step.
pub const SCALE: f32 = 8.0;

/// A cell's heights in world units, row-major, `SIDE` x `SIDE`.
#[derive(Debug, Clone, PartialEq)]
pub struct Heights(pub Vec<f32>);

impl Heights {
    /// A flat grid at `height`.
    #[must_use]
    pub fn flat(height: f32) -> Self {
        Self(vec![height; SIDE * SIDE])
    }

    /// A grid whose height at (`x`, `y`) is `f(x, y)`.
    #[must_use]
    pub fn from_fn(f: impl Fn(usize, usize) -> f32) -> Self {
        Self((0..SIDE * SIDE).map(|i| f(i % SIDE, i / SIDE)).collect())
    }
}

/// Encode heights as VHGT bytes. Heights are quantised to multiples of [`SCALE`]; each delta between
/// neighbouring quantised heights must fit a signed byte, otherwise it is clamped (a synthetic fixture should not
/// ask for more).
#[must_use]
pub fn encode_vhgt(heights: &Heights) -> Vec<u8> {
    let steps: Vec<i32> = heights.0.iter().map(|h| quantise(*h)).collect();
    let offset = f32::from(i16::try_from(steps[0]).unwrap_or(i16::MAX)) * SCALE;
    let mut out = Vec::with_capacity(VHGT_LEN);
    out.extend_from_slice(&offset.to_le_bytes());
    let base = steps[0];
    for y in 0..SIDE {
        for x in 0..SIDE {
            let here = steps[y * SIDE + x];
            let previous = match (x, y) {
                (0, 0) => base,
                (0, _) => steps[(y - 1) * SIDE],
                _ => steps[y * SIDE + x - 1],
            };
            let delta = i8::try_from((here - previous).clamp(-128, 127)).unwrap_or(0);
            out.push(delta.to_le_bytes()[0]);
        }
    }
    out.extend_from_slice(&[0, 0, 0]);
    out
}

/// Decode VHGT bytes made by [`encode_vhgt`]. `None` if the length is wrong.
#[must_use]
pub fn reference_decode(bytes: &[u8]) -> Option<Heights> {
    if bytes.len() != VHGT_LEN {
        return None;
    }
    let offset = f32::from_le_bytes(bytes[0..4].try_into().ok()?);
    let mut heights = vec![0.0_f32; SIDE * SIDE];
    let mut column_start = offset / SCALE;
    for y in 0..SIDE {
        let mut running = column_start;
        for x in 0..SIDE {
            let delta = f32::from(i8::from_le_bytes([bytes[4 + y * SIDE + x]]));
            running += delta;
            if x == 0 {
                column_start = running;
            }
            heights[y * SIDE + x] = running * SCALE;
        }
    }
    Some(Heights(heights))
}

#[allow(clippy::cast_possible_truncation)]
fn quantise(height: f32) -> i32 {
    (height / SCALE).round() as i32
}

#[cfg(test)]
mod tests {
    use super::{Heights, SCALE, SIDE, VHGT_LEN, encode_vhgt, reference_decode};

    fn step(n: usize) -> f32 {
        f32::from(u16::try_from(n).unwrap())
    }

    #[test]
    fn flat_terrain_round_trips() {
        let h = Heights::flat(64.0);
        let bytes = encode_vhgt(&h);
        assert_eq!(bytes.len(), VHGT_LEN);
        assert_eq!(reference_decode(&bytes), Some(h));
    }

    #[test]
    fn a_slope_round_trips_within_quantisation() {
        let h = Heights::from_fn(|x, y| 100.0 + 8.0 * step(x) + 16.0 * step(y));
        let back = reference_decode(&encode_vhgt(&h)).unwrap();
        for (a, b) in h.0.iter().zip(&back.0) {
            assert!((a - b).abs() <= SCALE / 2.0, "{a} vs {b}");
        }
    }

    #[test]
    fn negative_heights_round_trip() {
        let h = Heights::from_fn(|x, _| -200.0 + 8.0 * step(x));
        let back = reference_decode(&encode_vhgt(&h)).unwrap();
        for (a, b) in h.0.iter().zip(&back.0) {
            assert!((a - b).abs() <= SCALE / 2.0);
        }
    }

    #[test]
    fn the_first_column_integrates_downwards() {
        let h = Heights::from_fn(|_, y| 8.0 * step(y));
        let bytes = encode_vhgt(&h);
        // first delta of every row after the first is +1 step
        assert_eq!(bytes[4 + SIDE], 1);
        assert_eq!(bytes[4 + 2 * SIDE], 1);
    }

    #[test]
    fn a_wrong_length_is_rejected() {
        assert_eq!(reference_decode(&[0; 10]), None);
    }
}
