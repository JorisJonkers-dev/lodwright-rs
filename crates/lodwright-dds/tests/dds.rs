// SPDX-License-Identifier: MPL-2.0
//! Header goldens, quality floors, mip sizes, the DX10 ban and hostile input for the DDS writer and reader.

use lodwright_dds::{
    DdsError, EncodeOptions, Filter, Format, HEADER_LEN, Image, MAX_DIMENSION, Quality, decode,
    encode, mip_count, mip_size, psnr, read_header,
};
use lodwright_testkit::golden::assert_golden;
use lodwright_testkit::image::{gradient_rgba, noise_rgba};
use lodwright_testkit::rng::Rng;
use std::fmt::Write as _;
use std::path::Path;

const FORMATS: [Format; 3] = [Format::Bc1, Format::Bc3, Format::Bc5];

fn options(format: Format, mips: bool) -> EncodeOptions {
    EncodeOptions {
        format,
        mips,
        filter: Filter::Box,
        quality: Quality::Normal,
    }
}

fn gradient(w: u32, h: u32) -> Image {
    Image::new(w, h, gradient_rgba(w as usize, h as usize)).unwrap()
}

fn noise(w: u32, h: u32) -> Image {
    Image::new(w, h, noise_rgba(w as usize, h as usize, 7)).unwrap()
}

fn channels(format: Format) -> &'static [usize] {
    match format {
        Format::Bc1 => &[0, 1, 2],
        Format::Bc3 => &[0, 1, 2, 3],
        Format::Bc5 => &[0, 1],
    }
}

fn header_dump(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (i, chunk) in bytes[..HEADER_LEN].chunks(16).enumerate() {
        let _ = write!(out, "{:04x} ", i * 16);
        for b in chunk {
            let _ = write!(out, " {b:02x}");
        }
        out.push('\n');
    }
    let _ = writeln!(out, "total {} bytes", bytes.len());
    out
}

fn golden_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

#[test]
fn the_bc1_header_for_a_fixed_input_equals_the_stored_dump() {
    let bytes = encode(&gradient(10, 6), &options(Format::Bc1, true)).unwrap();
    assert_golden(&golden_path("header_bc1.txt"), &header_dump(&bytes));
}

#[test]
fn the_bc3_header_for_a_fixed_input_equals_the_stored_dump() {
    let bytes = encode(&gradient(10, 6), &options(Format::Bc3, true)).unwrap();
    assert_golden(&golden_path("header_bc3.txt"), &header_dump(&bytes));
}

#[test]
fn the_bc5_header_for_a_fixed_input_equals_the_stored_dump() {
    let bytes = encode(&gradient(10, 6), &options(Format::Bc5, true)).unwrap();
    assert_golden(&golden_path("header_bc5.txt"), &header_dump(&bytes));
}

#[test]
fn a_file_without_mips_has_no_mip_flag_and_only_the_base_surface() {
    let bytes = encode(&gradient(8, 8), &options(Format::Bc1, false)).unwrap();
    assert_eq!(bytes.len(), HEADER_LEN + 32);
    let flags = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    assert_eq!(flags & 0x2_0000, 0);
    assert_eq!(read_header(&bytes).unwrap().mip_count, 1);
}

#[test]
fn the_header_flags_and_caps_follow_the_documented_values() {
    let bytes = encode(&gradient(16, 16), &options(Format::Bc3, true)).unwrap();
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    assert_eq!(
        u32_at(8) & 0x0008_1007,
        0x0008_1007,
        "caps, height, width, pixel format, linear size"
    );
    assert_ne!(u32_at(8) & 0x2_0000, 0, "mipmap count flag");
    assert_eq!(u32_at(20), 16 * 16, "linear size of the top level");
    assert_eq!(u32_at(28), 5, "mip count");
    assert_eq!(u32_at(80) & 0x4, 0x4, "FourCC flag");
    assert_eq!(
        u32_at(108) & 0x0040_1008,
        0x0040_1008,
        "texture, complex, mipmap caps"
    );
}

#[test]
fn encoded_files_never_carry_a_dx10_header() {
    for format in FORMATS {
        for mips in [false, true] {
            let bytes = encode(&gradient(13, 9), &options(format, mips)).unwrap();
            assert_ne!(&bytes[84..88], b"DX10");
            assert_eq!(&bytes[84..88], &format.four_cc());
            let header = read_header(&bytes).unwrap();
            assert_eq!(
                bytes.len(),
                HEADER_LEN + header.data_bytes(),
                "no extension header, no padding"
            );
        }
    }
}

#[test]
fn a_dx10_file_is_refused_on_reading() {
    let mut bytes = encode(&gradient(8, 8), &options(Format::Bc1, false)).unwrap();
    bytes[84..88].copy_from_slice(b"DX10");
    bytes.extend_from_slice(&[0; 20]);
    assert_eq!(read_header(&bytes), Err(DdsError::Dx10Header));
}

#[test]
fn mip_sizes_and_the_file_length_are_right_for_non_power_of_two_input() {
    for format in FORMATS {
        let bytes = encode(&gradient(37, 19), &options(format, true)).unwrap();
        let decoded = decode(&bytes).unwrap();
        let sizes: Vec<_> = decoded
            .levels
            .iter()
            .map(|i| (i.width(), i.height()))
            .collect();
        assert_eq!(sizes, [(37, 19), (18, 9), (9, 4), (4, 2), (2, 1), (1, 1)]);
        let surfaces: usize = sizes
            .iter()
            .map(|(w, h)| format.surface_bytes(*w, *h))
            .sum();
        assert_eq!(bytes.len(), HEADER_LEN + surfaces);
    }
}

#[test]
fn surface_bytes_round_partial_blocks_up_with_a_minimum_of_one_block() {
    assert_eq!(Format::Bc1.surface_bytes(1, 1), 8);
    assert_eq!(Format::Bc1.surface_bytes(5, 5), 4 * 8);
    assert_eq!(Format::Bc3.surface_bytes(4, 8), 2 * 16);
    assert_eq!(Format::Bc5.surface_bytes(2, 2), 16);
}

#[test]
fn a_size_over_the_cap_is_refused_when_building_and_when_reading() {
    assert_eq!(
        Image::new(MAX_DIMENSION + 1, 1, vec![]),
        Err(DdsError::BadDimensions {
            width: MAX_DIMENSION + 1,
            height: 1
        })
    );
    let mut bytes = encode(&gradient(8, 8), &options(Format::Bc1, false)).unwrap();
    bytes[16..20].copy_from_slice(&(MAX_DIMENSION + 1).to_le_bytes());
    assert!(matches!(
        read_header(&bytes),
        Err(DdsError::BadDimensions { .. })
    ));
}

#[test]
fn reading_refuses_each_kind_of_bad_header() {
    let good = encode(&gradient(8, 8), &options(Format::Bc3, true)).unwrap();
    let patch = |at: usize, v: &[u8]| {
        let mut b = good.clone();
        b[at..at + v.len()].copy_from_slice(v);
        b
    };
    assert_eq!(read_header(&good[..100]), Err(DdsError::Truncated));
    assert_eq!(read_header(&patch(0, b"XDS ")), Err(DdsError::BadHeader));
    assert_eq!(
        read_header(&patch(4, &123_u32.to_le_bytes())),
        Err(DdsError::BadHeader)
    );
    assert_eq!(
        read_header(&patch(76, &31_u32.to_le_bytes())),
        Err(DdsError::BadHeader)
    );
    assert_eq!(
        read_header(&patch(84, b"DXT3")),
        Err(DdsError::UnsupportedFormat(*b"DXT3"))
    );
    assert_eq!(
        read_header(&patch(80, &0_u32.to_le_bytes())),
        Err(DdsError::UnsupportedFormat([0; 4]))
    );
    assert_eq!(
        read_header(&patch(28, &9_u32.to_le_bytes())),
        Err(DdsError::BadMipCount(9))
    );
    assert_eq!(
        read_header(&patch(12, &0_u32.to_le_bytes())),
        Err(DdsError::BadDimensions {
            width: 8,
            height: 0
        })
    );
    assert_eq!(
        read_header(&good[..good.len() - 1]),
        Err(DdsError::Truncated)
    );
    assert_eq!(
        read_header(&patch(28, &0_u32.to_le_bytes()))
            .unwrap()
            .mip_count,
        1
    );
}

#[test]
fn random_bytes_and_bit_flips_never_panic_decode() {
    let mut rng = Rng::new(31);
    for _ in 0..500 {
        let n = rng.below(400) as usize;
        let junk: Vec<u8> = (0..n)
            .map(|_| u8::try_from(rng.below(256)).unwrap())
            .collect();
        let _ = decode(&junk);
    }
    let good = encode(&gradient(9, 5), &options(Format::Bc5, true)).unwrap();
    for i in 0..HEADER_LEN {
        for bit in [0x01_u8, 0x10, 0x80] {
            let mut m = good.clone();
            m[i] ^= bit;
            let _ = decode(&m);
        }
    }
}

#[test]
fn encoding_then_decoding_gives_back_every_level_at_random_sizes() {
    let mut rng = Rng::new(5);
    for round in 0..40_u32 {
        let (w, h) = (rng.below(40) + 1, rng.below(40) + 1);
        let format = FORMATS[(round % 3) as usize];
        let image = noise(w, h);
        let bytes = encode(
            &image,
            &EncodeOptions {
                quality: Quality::Fast,
                ..options(format, true)
            },
        )
        .unwrap();
        let header = read_header(&bytes).unwrap();
        assert_eq!(
            (header.width, header.height, header.mip_count),
            (w, h, mip_count(w, h))
        );
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.levels.len(), header.mip_count as usize);
        for (l, level) in decoded.levels.iter().enumerate() {
            assert_eq!(
                (level.width(), level.height()),
                mip_size(w, h, u32::try_from(l).unwrap())
            );
        }
    }
}

// Quality floors in decibels, measured from the first implementation (64 by 64 synthetic inputs, cluster fit, no
// mips, `texpresso` 2.0.2) and committed here: the tests below fail when the decoded PSNR drops more than one decibel under them.
const FLOOR_TOLERANCE_DB: f64 = 1.0;

fn measured(image: &Image, format: Format) -> f64 {
    let bytes = encode(image, &options(format, false)).unwrap();
    let decoded = decode(&bytes).unwrap();
    psnr(image, &decoded.levels[0], channels(format))
}

fn assert_floor(image: &Image, format: Format, floor: f64) {
    let db = measured(image, format);
    assert!(
        db >= floor - FLOOR_TOLERANCE_DB,
        "{format:?}: {db:.2} dB under the floor {floor:.2} dB"
    );
}

#[test]
fn bc1_keeps_its_quality_floor_on_a_gradient() {
    assert_floor(&gradient(64, 64), Format::Bc1, GRADIENT_BC1);
}

#[test]
fn bc3_keeps_its_quality_floor_on_a_gradient() {
    assert_floor(&gradient(64, 64), Format::Bc3, GRADIENT_BC3);
}

#[test]
fn bc5_keeps_its_quality_floor_on_a_gradient() {
    assert_floor(&gradient(64, 64), Format::Bc5, GRADIENT_BC5);
}

#[test]
fn bc1_keeps_its_quality_floor_on_noise() {
    assert_floor(&noise(64, 64), Format::Bc1, NOISE_BC1);
}

#[test]
fn bc3_keeps_its_quality_floor_on_noise() {
    assert_floor(&noise(64, 64), Format::Bc3, NOISE_BC3);
}

#[test]
fn bc5_keeps_its_quality_floor_on_noise() {
    assert_floor(&noise(64, 64), Format::Bc5, NOISE_BC5);
}

const GRADIENT_BC1: f64 = 38.1;
const GRADIENT_BC3: f64 = 39.3;
const GRADIENT_BC5: f64 = 54.4;
const NOISE_BC1: f64 = 13.4;
const NOISE_BC3: f64 = 14.6;
const NOISE_BC5: f64 = 29.1;
