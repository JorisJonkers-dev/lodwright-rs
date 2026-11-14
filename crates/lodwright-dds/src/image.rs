// SPDX-License-Identifier: MPL-2.0
//! The image buffer type, and the pure colour adjustments on it.

use crate::error::{DdsError, Result};

/// The largest width or height accepted anywhere in this crate. Larger sizes are refused, so a hostile header or a
/// mistaken request cannot ask for gigabytes.
pub const MAX_DIMENSION: u32 = 16384;

/// An 8-bit RGBA image, rows top to bottom, straight (not premultiplied) alpha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// Check a size against the cap.
///
/// # Errors
/// [`DdsError::BadDimensions`] for zero or more than [`MAX_DIMENSION`].
pub fn check_dimensions(width: u32, height: u32) -> Result<()> {
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(DdsError::BadDimensions { width, height });
    }
    Ok(())
}

impl Image {
    /// Wrap `rgba` (four bytes per pixel).
    ///
    /// # Errors
    /// [`DdsError::BadDimensions`] or [`DdsError::BadPixelBuffer`].
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self> {
        check_dimensions(width, height)?;
        if rgba.len() != width as usize * height as usize * 4 {
            return Err(DdsError::BadPixelBuffer);
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    /// Width in pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The pixels, four bytes each.
    #[must_use]
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    /// Give back the pixel buffer.
    #[must_use]
    pub fn into_rgba(self) -> Vec<u8> {
        self.rgba
    }

    /// One pixel, or `None` outside the image.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let at = (y as usize * self.width as usize + x as usize) * 4;
        self.rgba[at..at + 4].try_into().ok()
    }
}

fn quantise(v: f32) -> u8 {
    // The value is clamped to 0..=255 first, so the cast cannot truncate or wrap.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        v.round().clamp(0.0, 255.0) as u8
    }
}

fn map_colour(image: &Image, lut: &[u8; 256]) -> Image {
    let mut rgba = image.rgba.clone();
    for px in rgba.as_chunks_mut::<4>().0.iter_mut() {
        for c in &mut px[..3] {
            *c = lut[usize::from(*c)];
        }
    }
    Image {
        width: image.width,
        height: image.height,
        rgba,
    }
}

/// Apply a gamma curve to the colour channels (alpha is left alone): `out = 255 * (in / 255) ^ (1 / gamma)`.
/// A gamma of 1.0 returns the image unchanged; a non-positive or non-finite gamma is treated as 1.0.
#[must_use]
pub fn apply_gamma(image: &Image, gamma: f32) -> Image {
    let g = if gamma.is_finite() && gamma > 0.0 {
        gamma
    } else {
        1.0
    };
    let mut lut = [0_u8; 256];
    for (i, out) in lut.iter_mut().enumerate() {
        let v = f32::from(u8::try_from(i).unwrap_or(255)) / 255.0;
        *out = quantise(255.0 * v.powf(1.0 / g));
    }
    map_colour(image, &lut)
}

/// Adjust brightness and contrast of the colour channels (alpha is left alone):
/// `out = (in / 255 - 0.5) * contrast + 0.5 + brightness`, clamped, with `brightness` in the unit range
/// (0.1 adds about 25 levels) and `contrast` 1.0 for no change. Non-finite arguments are treated as the identity.
#[must_use]
pub fn adjust_brightness_contrast(image: &Image, brightness: f32, contrast: f32) -> Image {
    let b = if brightness.is_finite() {
        brightness
    } else {
        0.0
    };
    let c = if contrast.is_finite() { contrast } else { 1.0 };
    let mut lut = [0_u8; 256];
    for (i, out) in lut.iter_mut().enumerate() {
        let v = f32::from(u8::try_from(i).unwrap_or(255)) / 255.0;
        *out = quantise(255.0 * ((v - 0.5) * c + 0.5 + b));
    }
    map_colour(image, &lut)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp() -> Image {
        let rgba: Vec<u8> = (0..=255_u8).flat_map(|v| [v, v, v, 200]).collect();
        Image::new(256, 1, rgba).unwrap()
    }

    #[test]
    fn dimensions_of_zero_or_over_the_cap_are_refused() {
        assert!(Image::new(0, 1, vec![]).is_err());
        assert!(Image::new(1, 0, vec![]).is_err());
        assert_eq!(
            check_dimensions(MAX_DIMENSION + 1, 1),
            Err(DdsError::BadDimensions {
                width: MAX_DIMENSION + 1,
                height: 1
            })
        );
        assert!(check_dimensions(MAX_DIMENSION, MAX_DIMENSION).is_ok());
    }

    #[test]
    fn a_buffer_of_the_wrong_length_is_refused() {
        assert_eq!(Image::new(2, 2, vec![0; 15]), Err(DdsError::BadPixelBuffer));
    }

    #[test]
    fn pixels_are_read_by_position_and_outside_is_none() {
        let img = Image::new(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        assert_eq!(img.pixel(1, 0), Some([5, 6, 7, 8]));
        assert_eq!(img.pixel(2, 0), None);
        assert_eq!(img.pixel(0, 1), None);
    }

    #[test]
    fn gamma_one_is_the_identity_and_alpha_is_untouched() {
        let img = ramp();
        assert_eq!(apply_gamma(&img, 1.0), img);
        let g = apply_gamma(&img, 2.2);
        assert!(g.rgba().chunks(4).all(|p| p[3] == 200));
    }

    #[test]
    fn gamma_above_one_brightens_and_is_monotone() {
        let g = apply_gamma(&ramp(), 2.0);
        let reds: Vec<u8> = g.rgba().chunks(4).map(|p| p[0]).collect();
        assert!(reds.windows(2).all(|w| w[0] <= w[1]));
        assert!(reds[64] > 64);
        assert_eq!((reds[0], reds[255]), (0, 255));
    }

    #[test]
    fn a_bad_gamma_is_the_identity() {
        let img = ramp();
        assert_eq!(apply_gamma(&img, 0.0), img);
        assert_eq!(apply_gamma(&img, f32::NAN), img);
    }

    #[test]
    fn neutral_brightness_and_contrast_are_the_identity() {
        let img = ramp();
        assert_eq!(adjust_brightness_contrast(&img, 0.0, 1.0), img);
        assert_eq!(
            adjust_brightness_contrast(&img, f32::NAN, f32::INFINITY),
            img
        );
    }

    #[test]
    fn brightness_shifts_and_contrast_stretches_around_the_middle() {
        let img = ramp();
        let bright = adjust_brightness_contrast(&img, 0.1, 1.0);
        assert!((125..=126).contains(&bright.pixel(100, 0).unwrap()[0]));
        assert_eq!(bright.pixel(250, 0).unwrap()[0], 255);
        let flat = adjust_brightness_contrast(&img, 0.0, 0.0);
        assert!(flat.rgba().chunks(4).all(|p| p[0] == 128));
        let harsh = adjust_brightness_contrast(&img, 0.0, 4.0);
        assert_eq!(
            (
                harsh.pixel(0, 0).unwrap()[0],
                harsh.pixel(255, 0).unwrap()[0]
            ),
            (0, 255)
        );
    }
}
