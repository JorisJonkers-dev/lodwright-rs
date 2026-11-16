// SPDX-License-Identifier: GPL-3.0-only
//! Synthetic RGBA8 pixel buffers for texture tests and benchmarks: gradients and noise, built from a seed.

use crate::rng::Rng;

fn scale(v: usize, of: usize) -> u8 {
    u8::try_from(v * 255 / of.max(1)).unwrap_or(255)
}

/// A smooth RGBA gradient: red grows along x, green along y, blue along both, alpha is opaque.
#[must_use]
pub fn gradient_rgba(width: usize, height: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            out.extend_from_slice(&[
                scale(x, width),
                scale(y, height),
                scale(x + y, width + height),
                255,
            ]);
        }
    }
    out
}

/// Uniform noise in every channel, alpha included.
#[must_use]
pub fn noise_rgba(width: usize, height: usize, seed: u64) -> Vec<u8> {
    let mut rng = Rng::new(seed);
    (0..width * height * 4)
        .map(|_| u8::try_from(rng.below(256)).unwrap_or(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{gradient_rgba, noise_rgba};

    #[test]
    fn a_gradient_has_four_bytes_per_pixel_and_runs_from_dark_to_bright() {
        let g = gradient_rgba(8, 4);
        assert_eq!(g.len(), 8 * 4 * 4);
        assert_eq!(&g[0..4], &[0, 0, 0, 255]);
        assert_eq!(g[7 * 4], 223);
    }

    #[test]
    fn noise_is_repeatable_per_seed() {
        assert_eq!(noise_rgba(4, 4, 9), noise_rgba(4, 4, 9));
        assert_ne!(noise_rgba(4, 4, 9), noise_rgba(4, 4, 10));
    }
}
