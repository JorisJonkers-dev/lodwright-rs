// SPDX-License-Identifier: GPL-3.0-only
//! Hand-run encoder benchmark: `cargo bench -p lodwright-dds --bench encode`.
//!
//! Synthetic 2048 by 2048 inputs from the testkit, one timing per format, quality and picture kind, with the
//! PSNR of the decoded result. Not run in CI (docs/TESTING.md, benchmarks policy).

use lodwright_dds::{EncodeOptions, Filter, Format, Image, Quality, decode, encode, psnr};
use lodwright_testkit::image::{gradient_rgba, noise_rgba};
use std::time::Instant;

fn main() {
    let side = 2048_u32;
    let n = side as usize;
    for (kind, pixels) in [
        ("gradient", gradient_rgba(n, n)),
        ("noise", noise_rgba(n, n, 1)),
    ] {
        let image = Image::new(side, side, pixels).expect("synthetic image");
        for format in [Format::Bc1, Format::Bc2, Format::Bc3, Format::Bc5] {
            for quality in [Quality::Fast, Quality::Normal] {
                let options = EncodeOptions {
                    format,
                    mips: false,
                    filter: Filter::Box,
                    quality,
                };
                let start = Instant::now();
                let bytes = encode(&image, &options).expect("encode");
                let ms = start.elapsed().as_secs_f64() * 1e3;
                let decoded = decode(&bytes).expect("decode");
                let channels: &[usize] = match format {
                    Format::Bc1 => &[0, 1, 2],
                    Format::Bc2 | Format::Bc3 => &[0, 1, 2, 3],
                    Format::Bc5 => &[0, 1],
                };
                let db = psnr(&image, &decoded.levels[0], channels);
                println!(
                    "{kind:8} {format:?} {quality:?}: {ms:9.1} ms, {db:.2} dB ({side}x{side}, {} bytes)",
                    bytes.len()
                );
            }
        }
    }
}
