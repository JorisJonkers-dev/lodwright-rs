// SPDX-License-Identifier: GPL-3.0-only
//! Fuzz harness for the LAND VHGT heightfield decoder. The only property: arbitrary bytes never panic, and a
//! payload that decodes has only finite heights.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(cell) = lodwright_core::land::decode_vhgt(data) {
        assert!(cell.as_slice().iter().all(|h| h.is_finite()));
    }
    let _ = lodwright_testkit::land::reference_decode(data);
});
