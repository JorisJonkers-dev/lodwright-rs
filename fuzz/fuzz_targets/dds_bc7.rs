// SPDX-License-Identifier: GPL-3.0-only
//! Fuzz harness for the BC7 reader: any DX10 file built around arbitrary block data and a small arbitrary size decodes
//! without a panic to an image of that size, and arbitrary bytes given to `decode_image` never panic.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = lodwright_dds::decode_image(data);
    let Some((&size, blocks)) = data.split_first() else {
        return;
    };
    let (width, height) = (u32::from(size & 0x0f) + 1, u32::from(size >> 4) + 1);
    let mut file = vec![0_u8; 148];
    file[0..4].copy_from_slice(b"DDS ");
    file[4..8].copy_from_slice(&124_u32.to_le_bytes());
    file[12..16].copy_from_slice(&height.to_le_bytes());
    file[16..20].copy_from_slice(&width.to_le_bytes());
    file[76..80].copy_from_slice(&32_u32.to_le_bytes());
    file[80..84].copy_from_slice(&4_u32.to_le_bytes());
    file[84..88].copy_from_slice(b"DX10");
    file[128..132].copy_from_slice(&98_u32.to_le_bytes());
    file.extend_from_slice(blocks);
    let need = (width as usize).div_ceil(4) * (height as usize).div_ceil(4) * 16;
    match lodwright_dds::decode_image(&file) {
        Ok(image) => {
            assert!(blocks.len() >= need);
            assert_eq!((image.width(), image.height()), (width, height));
        }
        Err(e) => assert!(blocks.len() < need, "{e}"),
    }
});
