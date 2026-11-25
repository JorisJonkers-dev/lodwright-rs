// SPDX-License-Identifier: GPL-3.0-only
//! The thread count never changes a byte of an encoded file.

use lodwright_dds::{EncodeOptions, Filter, Format, Image, Quality, encode, encode_with_threads};
use lodwright_testkit::{covers, ids};

/// A picture with edges, gradients and a transparent corner, from a fixed generator (no data files).
fn picture(width: u32, height: u32) -> Image {
    let mut state = 0x1234_5678_u32;
    let mut px = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (state >> 24) as u8;
            let edge = if (x / 7 + y / 5) % 2 == 0 { 220 } else { 30 };
            px.extend_from_slice(&[
                edge,
                u8::try_from(x * 255 / width.max(1)).unwrap_or(255),
                noise,
                if x < width / 8 && y < height / 8 {
                    0
                } else {
                    255
                },
            ]);
        }
    }
    Image::new(width, height, px).expect("an image")
}

#[test]
fn every_thread_count_gives_the_bytes_of_one_thread() {
    covers(&[ids::generate::run::SAME_BYTES_AT_ANY_THREAD_COUNT]);
    // sizes that are and are not a multiple of the band height, and one with a partial block row
    for (w, h) in [(256, 256), (200, 330), (64, 1024), (33, 513)] {
        let image = picture(w, h);
        for format in [Format::Bc1, Format::Bc3, Format::Bc5] {
            let options = EncodeOptions {
                format,
                mips: true,
                filter: Filter::Box,
                quality: Quality::Fast,
            };
            let one = encode(&image, &options).expect("encode");
            for threads in [2, 3, 8, 64] {
                assert_eq!(
                    encode_with_threads(&image, &options, threads).expect("encode"),
                    one,
                    "{format:?} {w}x{h} on {threads} threads"
                );
            }
        }
    }
}
