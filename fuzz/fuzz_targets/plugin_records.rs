// SPDX-License-Identifier: GPL-3.0-only
//! Fuzz harness for the plugin reader: arbitrary bytes never panic, and what is read has sane heights.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(plugin) = lodwright_core::plugin::read(data) {
        for world in &plugin.worlds {
            for cell in &world.cells {
                assert!(cell.heights.as_slice().iter().all(|h| h.is_finite()));
            }
        }
    }
    let _ = lodwright_testkit::plugin::parse_records(data);
});
