// SPDX-License-Identifier: GPL-3.0-only
//! Per-channel texture correlation of a generated terrain LOD folder against a reference folder.

#![allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "the names are lower-cased before the comparison"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lodwright_dds::{Image, decode_image};

use crate::verify::files_below;

/// The correlation of the textures of one terrain block with the reference's.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureCorrelation {
    /// The diffuse texture, relative to the folder, lower case.
    pub path: String,
    /// Pearson correlation of the red, green and blue channels of the diffuse texture, unrounded.
    pub diffuse: [f64; 3],
    /// The same for the normal map; `None` when it is missing on a side or cannot be compared.
    pub normals: Option<[f64; 3]>,
}

/// The Pearson correlation of each of the red, green and blue channels of two images (alpha is not compared).
///
/// A channel that is flat in both images has no variance: it counts as 1 when both hold the same value and 0
/// otherwise. A channel that is flat in only one image is 0. `None` when the sizes differ.
#[must_use]
pub fn channel_correlation(a: &Image, b: &Image) -> Option<[f64; 3]> {
    if (a.width(), a.height()) != (b.width(), b.height()) {
        return None;
    }
    let (pa, pb) = (a.rgba(), b.rgba());
    let n = i128::try_from(pa.len() / 4).ok()?;
    let mut out = [0.0; 3];
    for (c, slot) in out.iter_mut().enumerate() {
        let (mut sa, mut sb, mut saa, mut sbb, mut sab) = (0i128, 0i128, 0i128, 0i128, 0i128);
        for (x, y) in pa
            .iter()
            .skip(c)
            .step_by(4)
            .zip(pb.iter().skip(c).step_by(4))
        {
            let (x, y) = (i128::from(*x), i128::from(*y));
            sa += x;
            sb += y;
            saa += x * x;
            sbb += y * y;
            sab += x * y;
        }
        // n times the variances and the covariance, exact in integers
        let (va, vb, cov) = (n * saa - sa * sa, n * sbb - sb * sb, n * sab - sa * sb);
        *slot = match (va == 0, vb == 0) {
            (true, true) => f64::from(u8::from(sa == sb)),
            (true, false) | (false, true) => 0.0,
            #[allow(
                clippy::cast_precision_loss,
                reason = "sums of 8-bit samples fit f64 closely"
            )]
            (false, false) => {
                (cov as f64 / ((va as f64).sqrt() * (vb as f64).sqrt())).clamp(-1.0, 1.0)
            }
        };
    }
    Some(out)
}

fn decode(path: &Path) -> Option<Image> {
    decode_image(&std::fs::read(path).ok()?).ok()
}

/// Correlate the terrain diffuse texture, and the normal map where both sides have one, of every block present in
/// both `folder` and `reference` (names compared in lower case), sorted by path. A diffuse texture that cannot be
/// read on either side, or whose size differs, leaves the block out; [`crate::verify_folder`] and
/// [`crate::compare_folders`] report those.
#[must_use]
pub fn compare_textures(folder: &Path, reference: &Path) -> Vec<TextureCorrelation> {
    let lower = |root: &Path| -> BTreeMap<String, PathBuf> {
        files_below(root)
            .into_iter()
            .map(|(n, p)| (n.to_ascii_lowercase(), p))
            .filter(|(n, _)| n.starts_with("textures/landscape/lod/") && n.ends_with(".dds"))
            .collect()
    };
    let (mine, theirs) = (lower(folder), lower(reference));
    let mut out = Vec::new();
    for (name, path) in &mine {
        if !name.contains("/diffuse/") {
            continue;
        }
        let Some(other) = theirs.get(name) else {
            continue;
        };
        let Some(diffuse) = decode(path)
            .zip(decode(other))
            .and_then(|(a, b)| channel_correlation(&a, &b))
        else {
            continue;
        };
        let normal = name.replacen("/diffuse/", "/normals/", 1);
        let normals = mine
            .get(&normal)
            .zip(theirs.get(&normal))
            .and_then(|(a, b)| decode(a).zip(decode(b)))
            .and_then(|(a, b)| channel_correlation(&a, &b));
        out.push(TextureCorrelation {
            path: name.clone(),
            diffuse,
            normals,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{channel_correlation, compare_textures};
    use lodwright_dds::{EncodeOptions, Filter, Format, Image, Quality, encode};
    use lodwright_testkit::{covers, ids};

    /// An 8 by 8 image whose channels come from `f(x, y, channel)`.
    fn image(f: impl Fn(u32, u32, usize) -> u8) -> Image {
        let mut rgba = Vec::new();
        for y in 0..8 {
            for x in 0..8 {
                rgba.extend([f(x, y, 0), f(x, y, 1), f(x, y, 2), 255]);
            }
        }
        Image::new(8, 8, rgba).unwrap()
    }

    fn ramp(x: u32, y: u32, c: usize) -> u8 {
        u8::try_from((x * 8 + y) * 3 + u32::try_from(c).unwrap()).unwrap()
    }

    fn close(got: [f64; 3], want: [f64; 3]) {
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() < 1e-9, "{got:?} is not {want:?}");
        }
    }

    #[test]
    fn identical_varying_images_correlate_at_one_in_every_channel() {
        covers(&[ids::verify::folder::TEXTURE_CORRELATION]);
        let a = image(ramp);
        close(channel_correlation(&a, &a).unwrap(), [1.0; 3]);
    }

    #[test]
    fn an_inverted_image_correlates_at_minus_one() {
        covers(&[ids::verify::folder::TEXTURE_CORRELATION]);
        let (a, b) = (image(ramp), image(|x, y, c| 255 - ramp(x, y, c)));
        close(channel_correlation(&a, &b).unwrap(), [-1.0; 3]);
    }

    #[test]
    fn a_brightness_shift_does_not_change_the_correlation() {
        covers(&[ids::verify::folder::TEXTURE_CORRELATION]);
        let (a, b) = (
            image(|x, y, _| ramp(x, y, 0) / 2),
            image(|x, y, _| ramp(x, y, 0) / 2 + 100),
        );
        close(channel_correlation(&a, &b).unwrap(), [1.0; 3]);
    }

    #[test]
    fn a_channel_is_judged_on_its_own() {
        let a = image(ramp);
        let b = image(|x, y, c| {
            if c == 1 {
                255 - ramp(x, y, c)
            } else {
                ramp(x, y, c)
            }
        });
        close(channel_correlation(&a, &b).unwrap(), [1.0, -1.0, 1.0]);
    }

    #[test]
    fn two_flat_images_of_the_same_colour_correlate_at_one() {
        let a = image(|_, _, c| 10 + u8::try_from(c).unwrap());
        close(channel_correlation(&a, &a.clone()).unwrap(), [1.0; 3]);
    }

    #[test]
    fn two_flat_images_of_different_colours_correlate_at_zero() {
        covers(&[ids::verify::folder::TEXTURE_CORRELATION]);
        let (a, b) = (image(|_, _, _| 10), image(|_, _, _| 200));
        close(channel_correlation(&a, &b).unwrap(), [0.0; 3]);
    }

    #[test]
    fn a_flat_image_against_a_varying_one_correlates_at_zero() {
        let (a, b) = (image(|_, _, _| 10), image(ramp));
        close(channel_correlation(&a, &b).unwrap(), [0.0; 3]);
        close(channel_correlation(&b, &a).unwrap(), [0.0; 3]);
    }

    #[test]
    fn images_of_different_sizes_have_no_correlation() {
        covers(&[ids::verify::folder::TEXTURE_CORRELATION]);
        let a = image(ramp);
        let b = Image::new(4, 4, vec![0; 64]).unwrap();
        assert_eq!(channel_correlation(&a, &b), None);
    }

    fn write_dds(root: &std::path::Path, rel: &str, img: &Image) {
        let options = EncodeOptions {
            format: Format::Bc1,
            mips: false,
            filter: Filter::Box,
            quality: Quality::Fast,
        };
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, encode(img, &options).unwrap()).unwrap();
    }

    fn folder(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lodwright-corr-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    const DIFFUSE: &str = "Textures/Landscape/LOD/W/Diffuse/W.n.level4.x0.y0.dds";
    const NORMALS: &str = "textures/landscape/lod/w/normals/w.n.level4.x0.y0.dds";

    #[test]
    fn blocks_are_matched_by_name_in_lower_case_and_normals_follow_the_diffuse() {
        covers(&[ids::verify::folder::TEXTURE_CORRELATION]);
        let (mine, theirs) = (folder("mine"), folder("theirs"));
        let a = image(ramp);
        write_dds(&mine, DIFFUSE, &a);
        write_dds(&mine, NORMALS, &a);
        write_dds(&theirs, &DIFFUSE.to_ascii_lowercase(), &a);
        write_dds(&theirs, NORMALS, &a);
        let got = compare_textures(&mine, &theirs);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, DIFFUSE.to_ascii_lowercase());
        close(got[0].diffuse, [1.0; 3]);
        close(got[0].normals.unwrap(), [1.0; 3]);
    }

    #[test]
    fn a_block_without_a_normal_map_on_one_side_has_no_normal_correlation() {
        let (mine, theirs) = (folder("n-mine"), folder("n-theirs"));
        let a = image(ramp);
        write_dds(&mine, DIFFUSE, &a);
        write_dds(&theirs, DIFFUSE, &a);
        write_dds(&theirs, NORMALS, &a);
        let got = compare_textures(&mine, &theirs);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].normals, None);
    }

    const DIR: &str = "textures/landscape/lod/w/diffuse/";

    #[test]
    fn a_block_missing_from_the_reference_is_left_out() {
        let (mine, theirs) = (folder("m-mine"), folder("m-theirs"));
        write_dds(&mine, &format!("{DIR}w.n.level4.x0.y0.dds"), &image(ramp));
        std::fs::create_dir_all(&theirs).unwrap();
        assert_eq!(compare_textures(&mine, &theirs), Vec::new());
    }

    #[test]
    fn a_block_of_another_size_is_left_out() {
        let (mine, theirs) = (folder("s-mine"), folder("s-theirs"));
        write_dds(&mine, &format!("{DIR}w.n.level4.x0.y0.dds"), &image(ramp));
        let small = Image::new(4, 4, vec![7; 64]).unwrap();
        write_dds(&theirs, &format!("{DIR}w.n.level4.x0.y0.dds"), &small);
        assert_eq!(compare_textures(&mine, &theirs), Vec::new());
    }

    #[test]
    fn a_block_with_an_unreadable_texture_is_left_out() {
        covers(&[ids::verify::folder::TEXTURE_CORRELATION]);
        let (mine, theirs) = (folder("u-mine"), folder("u-theirs"));
        write_dds(&mine, &format!("{DIR}w.n.level4.x0.y0.dds"), &image(ramp));
        std::fs::create_dir_all(theirs.join(DIR)).unwrap();
        std::fs::write(theirs.join(DIR).join("w.n.level4.x0.y0.dds"), b"not a dds").unwrap();
        assert_eq!(compare_textures(&mine, &theirs), Vec::new());
    }
}
