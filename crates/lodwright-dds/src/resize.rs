// SPDX-License-Identifier: GPL-3.0-only
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
///
/// Halving and the one-pixel floor are `dds-mip-chain`. Rounding an odd size down is this project's choice: the one
/// odd-size example found in an allowed source (`dds-mip-odd-size`, a 5 by 1 texture whose next level is 2 by 1)
/// agrees with it, the row is verified for the example only.
// source: dds-mip-chain, dds-mip-odd-size
#[must_use]
pub fn mip_size(width: u32, height: u32, level: u32) -> (u32, u32) {
    (
        (width >> level.min(31)).max(1),
        (height >> level.min(31)).max(1),
    )
}

/// The number of levels of a full chain down to 1 by 1: `floor(log2(max(width, height))) + 1`. The count follows
/// from rounding odd sizes down (see [`mip_size`]), which is this project's choice.
// source: dds-mip-chain, dds-mip-odd-size
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

/// Rows (of source or target) below which a pass runs on the calling thread: starting threads would cost more.
const MIN_ROWS_PER_BAND: usize = 64;

/// Run `fill(first_row, rows)` over `buffer` cut into bands of whole rows (`row_len` values each), on up to `threads`
/// threads. Every row is made on its own from the same inputs, so the values do not depend on the cut.
fn fill_rows<T: Send>(
    buffer: &mut [T],
    row_len: usize,
    threads: usize,
    fill: impl Fn(usize, &mut [T]) + Sync,
) {
    let rows = buffer.len().checked_div(row_len).unwrap_or(0);
    let bands = threads.min(rows / MIN_ROWS_PER_BAND).max(1);
    if bands == 1 {
        fill(0, buffer);
        return;
    }
    let per_band = rows.div_ceil(bands);
    std::thread::scope(|scope| {
        for (i, chunk) in buffer.chunks_mut(per_band * row_len).enumerate() {
            let fill = &fill;
            scope.spawn(move || fill(i * per_band, chunk));
        }
    });
}

/// Resample `image` to `width` by `height`.
///
/// # Errors
/// [`crate::DdsError::BadDimensions`] for a target of zero or above the cap.
pub fn resize(image: &Image, width: u32, height: u32, filter: Filter) -> Result<Image> {
    resize_with_threads(image, width, height, filter, 1)
}

/// [`resize`] with the rows of each pass made on up to `threads` threads. The bytes are the same for any thread count:
/// every output row is computed on its own, in the same order of operations.
///
/// # Errors
/// As [`resize`].
pub fn resize_with_threads(
    image: &Image,
    width: u32,
    height: u32,
    filter: Filter,
    threads: usize,
) -> Result<Image> {
    check_dimensions(width, height)?;
    let (sw, sh) = (image.width() as usize, image.height() as usize);
    let (dw, dh) = (width as usize, height as usize);
    let src = image.rgba();

    let htaps = taps(sw, dw, filter);
    let mut mid = vec![0.0_f32; dw * sh * 4];
    fill_rows(&mut mid, dw * 4, threads, |first, band| {
        for (r, row) in band.chunks_mut(dw * 4).enumerate() {
            let y = first + r;
            for (x, tap) in htaps.iter().enumerate() {
                let mut acc = [0.0_f32; 4];
                for (k, w) in tap.weights.iter().enumerate() {
                    let at = (y * sw + tap.start + k) * 4;
                    for c in 0..4 {
                        acc[c] += w * f32::from(src[at + c]);
                    }
                }
                row[x * 4..x * 4 + 4].copy_from_slice(&acc);
            }
        }
    });

    let vtaps = taps(sh, dh, filter);
    let mut out = vec![0_u8; dw * dh * 4];
    fill_rows(&mut out, dw * 4, threads, |first, band| {
        for (r, row) in band.chunks_mut(dw * 4).enumerate() {
            let tap = &vtaps[first + r];
            for x in 0..dw {
                let mut acc = [0.0_f32; 4];
                for (k, w) in tap.weights.iter().enumerate() {
                    let at = ((tap.start + k) * dw + x) * 4;
                    for c in 0..4 {
                        acc[c] += w * mid[at + c];
                    }
                }
                for c in 0..4 {
                    row[x * 4 + c] = to_u8(acc[c]);
                }
            }
        }
    });
    Image::new(width, height, out)
}

/// The full mip chain: the image itself, then each level resampled from the one before, down to 1 by 1.
///
/// # Errors
/// Never for a valid [`Image`]; the signature carries the resize error.
pub fn mip_chain(image: &Image, filter: Filter) -> Result<Vec<Image>> {
    mip_chain_with_threads(image, filter, 1)
}

/// [`mip_chain`] with each level resampled on up to `threads` threads (the same bytes for any thread count).
///
/// # Errors
/// As [`mip_chain`].
pub fn mip_chain_with_threads(image: &Image, filter: Filter, threads: usize) -> Result<Vec<Image>> {
    let levels = mip_count(image.width(), image.height());
    let mut chain = Vec::with_capacity(levels as usize);
    chain.push(image.clone());
    for level in 1..levels {
        let (w, h) = mip_size(image.width(), image.height(), level);
        let next = resize_with_threads(&chain[level as usize - 1], w, h, filter, threads)?;
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

    #[test]
    fn resizing_and_mip_chains_give_the_same_bytes_at_any_thread_count() {
        // 300 by 200 has enough rows to be cut into several bands, and a pattern that every filter tap sees
        let pixels: Vec<u8> = (0..300 * 200 * 4)
            .map(|i: u32| u8::try_from((i.wrapping_mul(2_654_435_761) >> 13) & 0xFF).unwrap())
            .collect();
        let image = Image::new(300, 200, pixels).unwrap();
        for filter in [Filter::Box, Filter::Lanczos3] {
            let one = resize_with_threads(&image, 150, 100, filter, 1).unwrap();
            let odd = resize_with_threads(&image, 97, 61, filter, 1).unwrap();
            let chain = mip_chain_with_threads(&image, filter, 1).unwrap();
            for threads in [0, 2, 3, 16, 1000] {
                assert_eq!(
                    resize_with_threads(&image, 150, 100, filter, threads).unwrap(),
                    one
                );
                assert_eq!(
                    resize_with_threads(&image, 97, 61, filter, threads).unwrap(),
                    odd
                );
                assert_eq!(
                    mip_chain_with_threads(&image, filter, threads).unwrap(),
                    chain
                );
            }
        }
    }
}
