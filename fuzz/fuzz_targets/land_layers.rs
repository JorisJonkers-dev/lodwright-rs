// SPDX-License-Identifier: MPL-2.0
//! Fuzz harness for the LAND layer decoders (VNML, VCLR, BTXT, ATXT, VTXT) and the plugin reader with layers on.
//! The only property: arbitrary bytes never panic, and decoded opacities are finite.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lodwright_core::layers::{LayerReader, decode_vtxt};
use lodwright_core::plugin::{ReadOptions, read_with};

fuzz_target!(|data: &[u8]| {
    if let Ok(grid) = decode_vtxt(data) {
        assert!(grid.iter().all(|v| v.is_finite()));
    }
    let mut reader = LayerReader::default();
    for signature in [*b"VNML", *b"VCLR", *b"BTXT", *b"ATXT", *b"VTXT", *b"ZZZZ"] {
        let _ = reader.subrecord(signature, data);
    }
    let _ = reader.finish();
    let _ = read_with(data, ReadOptions {
            land_layers: true,
            objects: true,
        });
});
