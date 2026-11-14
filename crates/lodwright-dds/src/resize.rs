// SPDX-License-Identifier: MPL-2.0
//! Resampling and mip chains, in plain `f32` on the 8-bit channel values.
//!
//! The filters are the textbook area-average (box) filter and the Lanczos filter with three lobes, applied
//! separably. Alpha is resampled like a colour channel (straight alpha, no premultiplication).

use crate::error::Result;
use crate::image::{Image, check_dimensions};

/// The resampling filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    /// Area average: each output pixel is the mean of the source area it covers.
    Box,
    /// Lanczos with three lobes: sharper, with slight ringing.
    Lanczos3,
}

/// The size of mip level `level` (0 is the base): each level halves the previous one, rounding down, with a
/// minimum of one pixel in each direction.
// source: dds-mip-chain
#[must_use]
pub fn mip_size(width: u32, height: u32, level: u32) -> (u32, u32) {
    (
        (width >> level.min(31)).max(1),
        (height >> level.min(31)).max(1),
    )
}

/// The number of levels of a full chain down to 1 by 1: `floor(log2(max(width, height))) + 1`.
// source: dds-mip-chain
#[must_use]
pub fn mip_count(width: u32, height: u32) -> u32 {
    32 - width.max(height).max(1).leading_zeros()
}

struct Tap {
    start: usize,
    weights: Vec<f32>,
}

fn lanczos3(x: f32) -> f32 {
    if x == 0.0 {
        return 1.0;
    }
    if x.abs() >= 3.0 {
        return 0.0;
    }
    let px = std::f32::consts::PI * x;
    3.0 * px.sin() * (px / 3.0).sin() / (px * px)
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn taps(src: usize, dst: usize, filter: Filter) -> Vec<Tap> {
    let scale = src as f32 / dst as f32;
    (0..dst)
        .map(|i| {
            let lo = i as f32 * scale;
            let hi = lo + scale;
            let centre = lo + scale / 2.0;
            let (from, to) = match filter {
                Filter::Box => (lo.floor(), hi.ceil()),
                Filter::Lanczos3 => {
                    let s = 3.0 * scale.max(1.0);
                    ((centre - s).floor(), (centre + s).ceil())
                }
            };
            let first = from.max(0.0) as usize;
            let last = (to.max(0.0) as usize).min(src);
            let mut weights: Vec<f32> = (first..last)
                .map(|j| match filter {
                    Filter::Box => {
                        let a = (j as f32).max(lo);
                        let b = ((j + 1) as f32).min(hi);
                        (b - a).max(0.0)
                    }
                    Filter::Lanczos3 => lanczos3((j as f32 + 0.5 - centre) / scale.max(1.0)),
                })
                .collect();
            let sum: f32 = weights.iter().sum();
            if sum.abs() > f32::EPSILON {
                for w in &mut weights {
                    *w /= sum;
                }
            } else {
                // Cannot happen for sizes inside the cap; fall back to the nearest source pixel.
                weights = vec![1.0];
            }
            Tap {
                start: first.min(src.saturating_sub(1)),
                weights,
            }
        })
        .collect()
}

fn to_u8(v: f32) -> u8 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        v.round().clamp(0.0, 255.0) as u8
    }
}

/// Resample `image` to `width` by `height`.
///
/// # Errors
/// [`crate::DdsError::BadDimensions`] for a target of zero or above the cap.
pub fn resize(image: &Image, width: u32, height: u32, filter: Filter) -> Result<Image> {
    check_dimensions(width, height)?;
    let (sw, sh) = (image.width() as usize, image.height() as usize);
    let (dw, dh) = (width as usize, height as usize);
    let src = image.rgba();

    let htaps = taps(sw, dw, filter);
    let mut mid = vec![0.0_f32; dw * sh * 4];
    for y in 0..sh {
        for (x, tap) in htaps.iter().enumerate() {
            let mut acc = [0.0_f32; 4];
            for (k, w) in tap.weights.iter().enumerate() {
                let at = (y * sw + tap.start + k) * 4;
                for c in 0..4 {
                    acc[c] += w * f32::from(src[at + c]);
                }
            }
            mid[(y * dw + x) * 4..(y * dw + x) * 4 + 4].copy_from_slice(&acc);
        }
    }

    let vtaps = taps(sh, dh, filter);
    let mut out = vec![0_u8; dw * dh * 4];
    for (y, tap) in vtaps.iter().enumerate() {
        for x in 0..dw {
            let mut acc = [0.0_f32; 4];
            for (k, w) in tap.weights.iter().enumerate() {
                let at = ((tap.start + k) * dw + x) * 4;
                for c in 0..4 {
                    acc[c] += w * mid[at + c];
                }
            }
            for c in 0..4 {
                out[(y * dw + x) * 4 + c] = to_u8(acc[c]);
            }
        }
    }
    Image::new(width, height, out)
}

/// The full mip chain: the image itself, then each level resampled from the one before, down to 1 by 1.
///
/// # Errors
/// Never for a valid [`Image`]; the signature carries the resize error.
pub fn mip_chain(image: &Image, filter: Filter) -> Result<Vec<Image>> {
    let levels = mip_count(image.width(), image.height());
    let mut chain = Vec::with_capacity(levels as usize);
    chain.push(image.clone());
    for level in 1..levels {
        let (w, h) = mip_size(image.width(), image.height(), level);
        let next = resize(&chain[level as usize - 1], w, h, filter)?;
        chain.push(next);
    }
    Ok(chain)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: u32, h: u32, v: u8) -> Image {
        Image::new(w, h, vec![v; w as usize * h as usize * 4]).unwrap()
    }

    #[test]
    fn mip_sizes_halve_rounding_down_with_a_floor_of_one() {
        let sizes: Vec<_> = (0..mip_count(37, 19))
            .map(|l| mip_size(37, 19, l))
            .collect();
        assert_eq!(sizes, [(37, 19), (18, 9), (9, 4), (4, 2), (2, 1), (1, 1)]);
        let sizes: Vec<_> = (0..mip_count(256, 64))
            .map(|l| mip_size(256, 64, l))
            .collect();
        assert_eq!(sizes.last(), Some(&(1, 1)));
        assert_eq!(sizes[6], (4, 1));
        assert_eq!(mip_count(1, 1), 1);
    }

    #[test]
    fn a_flat_image_stays_flat_under_both_filters_at_any_ratio() {
        for filter in [Filter::Box, Filter::Lanczos3] {
            for (w, h) in [(1, 1), (3, 5), (7, 2), (20, 20)] {
                let out = resize(&flat(13, 9, 77), w, h, filter).unwrap();
                assert!(out.rgba().iter().all(|v| *v == 77), "{filter:?} {w}x{h}");
            }
        }
    }

    #[test]
    fn the_box_filter_averages_a_two_by_two_block() {
        let img = Image::new(
            2,
            2,
            vec![0, 0, 0, 255, 100, 0, 0, 255, 200, 0, 0, 255, 100, 0, 0, 255],
        )
        .unwrap();
        let out = resize(&img, 1, 1, Filter::Box).unwrap();
        assert_eq!(out.pixel(0, 0), Some([100, 0, 0, 255]));
    }

    #[test]
    fn resizing_to_the_same_size_with_box_is_the_identity() {
        let img = Image::new(
            5,
            3,
            (0..60).map(|v| u8::try_from(v * 4).unwrap()).collect(),
        )
        .unwrap();
        assert_eq!(resize(&img, 5, 3, Filter::Box).unwrap(), img);
    }

    #[test]
    fn a_target_over_the_cap_or_of_zero_is_refused() {
        let img = flat(2, 2, 1);
        assert!(resize(&img, 0, 2, Filter::Box).is_err());
        assert!(resize(&img, crate::MAX_DIMENSION + 1, 2, Filter::Lanczos3).is_err());
    }

    #[test]
    fn lanczos_keeps_a_gradient_monotone_enough_to_stay_in_range() {
        let img = Image::new(
            8,
            1,
            (0..8_u8)
                .flat_map(|v| [v * 36, v * 36, v * 36, 255])
                .collect(),
        )
        .unwrap();
        let out = resize(&img, 4, 1, Filter::Lanczos3).unwrap();
        let reds: Vec<u8> = out.rgba().chunks(4).map(|p| p[0]).collect();
        assert!(reds.windows(2).all(|w| w[0] <= w[1]), "{reds:?}");
    }

    #[test]
    fn the_chain_has_the_expected_sizes_for_a_non_power_of_two_image() {
        let chain = mip_chain(&flat(37, 19, 9), Filter::Box).unwrap();
        let sizes: Vec<_> = chain.iter().map(|i| (i.width(), i.height())).collect();
        assert_eq!(sizes, [(37, 19), (18, 9), (9, 4), (4, 2), (2, 1), (1, 1)]);
    }

    #[test]
    fn a_one_by_one_image_has_a_chain_of_one() {
        assert_eq!(mip_chain(&flat(1, 1, 3), Filter::Box).unwrap().len(), 1);
    }
}
