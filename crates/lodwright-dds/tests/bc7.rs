// SPDX-License-Identifier: GPL-3.0-only
//! Reading BC7 files with a DX10 extension header, built here block by block.

use lodwright_dds::{DdsError, decode, decode_image};

/// A DDS file with a DX10 extension header naming `dxgi`, followed by `data`.
fn dx10_file(width: u32, height: u32, dxgi: u32, data: &[u8]) -> Vec<u8> {
    let mut f = vec![0_u8; 148];
    f[0..4].copy_from_slice(b"DDS ");
    f[4..8].copy_from_slice(&124_u32.to_le_bytes());
    f[8..12].copy_from_slice(&0x1007_u32.to_le_bytes());
    f[12..16].copy_from_slice(&height.to_le_bytes());
    f[16..20].copy_from_slice(&width.to_le_bytes());
    f[28..32].copy_from_slice(&1_u32.to_le_bytes());
    f[76..80].copy_from_slice(&32_u32.to_le_bytes());
    f[80..84].copy_from_slice(&4_u32.to_le_bytes());
    f[84..88].copy_from_slice(b"DX10");
    f[108..112].copy_from_slice(&0x1000_u32.to_le_bytes());
    f[128..132].copy_from_slice(&dxgi.to_le_bytes());
    f[132..136].copy_from_slice(&3_u32.to_le_bytes());
    f[140..144].copy_from_slice(&1_u32.to_le_bytes());
    f.extend_from_slice(data);
    f
}

/// A BC7 mode 6 block of one colour: both endpoints the same 7-bit values with p-bits of 1 (so each channel is
/// `value * 2 + 1`), every index 0.
fn solid_block(rgba7: [u8; 4]) -> [u8; 16] {
    let mut bits: u128 = 1 << 6; // mode 6
    let mut at = 7;
    for c in rgba7 {
        for _ in 0..2 {
            bits |= u128::from(c & 0x7f) << at;
            at += 7;
        }
    }
    bits |= 0b11 << at; // the two p-bits
    bits.to_le_bytes()
}

#[test]
fn a_bc7_file_with_a_dx10_header_is_decoded_block_by_block() {
    // two blocks side by side: 8 by 4 pixels
    let mut data = Vec::new();
    data.extend_from_slice(&solid_block([100, 50, 25, 127]));
    data.extend_from_slice(&solid_block([10, 60, 110, 63]));
    for dxgi in [98, 99] {
        let image = decode_image(&dx10_file(8, 4, dxgi, &data)).unwrap();
        assert_eq!((image.width(), image.height()), (8, 4));
        for y in 0..4 {
            assert_eq!(image.pixel(0, y), Some([201, 101, 51, 255]));
            assert_eq!(image.pixel(3, y), Some([201, 101, 51, 255]));
            assert_eq!(image.pixel(4, y), Some([21, 121, 221, 127]));
            assert_eq!(image.pixel(7, y), Some([21, 121, 221, 127]));
        }
    }
}

#[test]
fn a_dx10_header_with_another_format_is_named_in_the_error() {
    let data = [0_u8; 16];
    assert_eq!(
        decode_image(&dx10_file(4, 4, 71, &data)).unwrap_err(),
        DdsError::UnsupportedDxgiFormat(71)
    );
    assert_eq!(
        DdsError::UnsupportedDxgiFormat(71).to_string(),
        "DX10 format 71 is not supported (only BC7 is read)"
    );
}

#[test]
fn a_bc7_file_shorter_than_its_blocks_is_truncated() {
    let one_block = solid_block([1, 2, 3, 4]);
    assert_eq!(
        decode_image(&dx10_file(8, 4, 98, &one_block)).unwrap_err(),
        DdsError::Truncated
    );
    assert_eq!(
        decode_image(&dx10_file(4, 4, 98, &[])[..140]).unwrap_err(),
        DdsError::Truncated
    );
}

#[test]
fn the_strict_reader_still_refuses_a_dx10_header() {
    let file = dx10_file(4, 4, 98, &solid_block([1, 2, 3, 4]));
    assert_eq!(decode(&file).unwrap_err(), DdsError::Dx10Header);
}
